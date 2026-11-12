//! LiveKit/libwebrtc media, independent of Matrix and GPUI.
//! Decoders run on SDK threads. One Tokio consumer per video track converts to
//! BGRA, replacing a single Arc slot; UI readers never receive queued frames.
pub use livekit;
pub mod camera;
pub mod screen;
mod transport;
use futures_util::StreamExt;
use livekit::options::{TrackPublishOptions, VideoCodec};
#[cfg(feature = "evidence")]
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::{
    e2ee::{
        E2eeOptions, EncryptionType,
        key_provider::{KeyDerivationAlgorithm, KeyProvider, KeyProviderOptions},
    },
    prelude::*,
    webrtc::{
        audio_source::RtcAudioSource, video_frame::VideoFormatType,
        video_stream::native::NativeVideoStream,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};
use tokio::{sync::mpsc, task::JoinHandle};

/// Video handle IDs: the local camera is `local:<sid>`, the local screen
/// share `local:screen:<sid>`, remote tracks their bare track SID.
pub const LOCAL_SCREEN: &str = "local:screen:";
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    #[error("media_connection_failed")]
    Connection,
    #[error("media_e2ee_required")]
    E2eeRequired,
    #[error("media_device_unavailable")]
    DeviceUnavailable,
    #[error("media_invalid_frame")]
    InvalidFrame,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Devices {
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
    pub cameras: Vec<Device>,
}

/// Tightly packed BGRA8, top-to-bottom, stride = width * 4, premultiplied alpha
/// (decoded opaque frames always have alpha 255). Immutable and Send + Sync.
/// Rotation is clockwise degrees to apply during presentation.
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rotation: u16,
    pub timestamp_us: i64,
    pub bgra: Vec<u8>,
}
impl VideoFrame {
    pub fn new(
        width: u32,
        height: u32,
        rotation: u16,
        timestamp_us: i64,
        bgra: Vec<u8>,
    ) -> Result<Self, MediaError> {
        if width == 0
            || height == 0
            || width > 3840
            || height > 2160
            || ![0, 90, 180, 270].contains(&rotation)
            || width as usize * height as usize * 4 != bgra.len()
        {
            return Err(MediaError::InvalidFrame);
        }
        Ok(Self {
            width,
            height,
            rotation,
            timestamp_us,
            bgra,
        })
    }
}
#[derive(Default)]
struct FrameSlot {
    frame: Option<Arc<VideoFrame>>,
    retired: bool,
}
#[derive(Clone, Default)]
pub struct LatestFrame(Arc<Mutex<FrameSlot>>);
impl LatestFrame {
    /// A handle holding one fixed frame, for previews and fixtures.
    pub fn fixed(frame: VideoFrame) -> Self {
        let handle = Self::default();
        handle.replace(frame);
        handle
    }
    pub fn latest(&self) -> Option<Arc<VideoFrame>> {
        self.0.lock().ok()?.frame.clone()
    }
    fn replace(&self, frame: VideoFrame) {
        if let Ok(mut slot) = self.0.lock()
            && !slot.retired
            && slot
                .frame
                .as_ref()
                .is_none_or(|f| frame.timestamp_us >= f.timestamp_us)
        {
            slot.frame = Some(Arc::new(frame));
        }
    }
    fn clear(&self) {
        if let Ok(mut slot) = self.0.lock() {
            slot.retired = true;
            slot.frame = None;
        }
    }
}
#[derive(Clone, Default)]
pub struct VideoTracks(Arc<Mutex<HashMap<String, LatestFrame>>>);
impl VideoTracks {
    pub fn get(&self, id: &str) -> Option<LatestFrame> {
        self.0.lock().ok()?.get(id).cloned()
    }
    pub fn ids(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|v| v.keys().cloned().collect())
            .unwrap_or_default()
    }
    fn insert(&self, id: String) -> LatestFrame {
        let frame = LatestFrame::default();
        if let Ok(mut tracks) = self.0.lock()
            && let Some(previous) = tracks.insert(id, frame.clone())
        {
            previous.clear();
        }
        frame
    }
    fn remove(&self, id: &str) {
        if let Ok(mut tracks) = self.0.lock()
            && let Some(frame) = tracks.remove(id)
        {
            frame.clear();
        }
    }
    fn clear(&self) {
        if let Ok(mut tracks) = self.0.lock() {
            for frame in tracks.values() {
                frame.clear();
            }
            tracks.clear();
        }
    }
}
/// Aggregate evidence only. No samples, images or secrets enter snapshots/logs.
#[derive(Clone, Default)]
pub struct Evidence {
    pub audio_frames: Arc<AtomicU64>,
    pub audio_signal_frames: Arc<AtomicU64>,
    pub video_frames: Arc<AtomicU64>,
    pub video_pattern_frames: Arc<AtomicU64>,
}

/// These values match Element Call v0.28.2 MatrixKeyProvider + livekit-client.
pub fn matrix_key_provider() -> KeyProvider {
    KeyProvider::new(KeyProviderOptions {
        ratchet_window_size: 10,
        ratchet_salt: b"LKFrameEncryptionKey".to_vec(),
        failure_tolerance: -1,
        key_ring_size: 256,
        key_derivation_algorithm: KeyDerivationAlgorithm::HKDF,
    })
}

pub struct NativeMedia {
    _trust: transport::TrustBinding,
    pub room: Arc<Room>,
    pub events: mpsc::Receiver<RoomEvent>,
    pub keys: KeyProvider,
    pub video: VideoTracks,
    pub evidence: Evidence,
    audio: Option<livekit::PlatformAudio>,
    microphone: Option<LocalAudioTrack>,
    camera: Option<LocalVideoTrack>,
    capture: Option<camera::Camera>,
    screen: Option<LocalVideoTrack>,
    screen_capture: Option<screen::ScreenCapture>,
    consumers: HashMap<String, JoinHandle<()>>,
    event_pump: JoinHandle<()>,
    sender_index: AtomicU8,
    /// Identities of current call members. Only their encrypted tracks are
    /// subscribed; anyone else in the SFU room is never received.
    allowed: HashSet<String>,
    /// Members with at least one unencrypted publication (never subscribed).
    blocked: HashSet<String>,
    /// Key indices installed per remote identity, overwritten on departure.
    installed: HashMap<String, HashSet<u8>>,
    /// Remote track sid → publishing identity, to stop a departed member's tracks.
    owners: HashMap<String, String>,
}
impl NativeMedia {
    pub async fn connect(
        url: &str,
        jwt: &str,
        keys: KeyProvider,
        http: reqwest::Client,
    ) -> Result<Self, MediaError> {
        let trust = transport::bind(url, jwt, http).map_err(|_| MediaError::Connection)?;
        let mut options = RoomOptions::default();
        options.encryption = Some(E2eeOptions {
            encryption_type: EncryptionType::Gcm,
            key_provider: keys.clone(),
        });
        options.auto_subscribe = false;
        options.join_retries = 0;
        options.single_peer_connection = false;
        let (room, events) = Room::connect(url, jwt, options)
            .await
            .map_err(|_| MediaError::Connection)?;
        if !room.e2ee_manager().enabled() {
            let _ = room.close().await;
            return Err(MediaError::E2eeRequired);
        }
        // Nothing is subscribed until the caller names the current members
        // (`set_members`): SFU presence alone never grants reception.
        let room = Arc::new(room);
        trust.attach(&room);
        let (send, bounded) = mpsc::channel(64);
        let pump_room = room.clone();
        let mut events = events;
        let event_pump = tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                // The native cryptor can report each undecryptable frame while
                // an authenticated participant's key is still in flight. SDK
                // state owns these transitions; they are not UI snapshots.
                if matches!(event, RoomEvent::E2eeStateChanged { .. }) {
                    continue;
                }
                if send.try_send(event).is_err() {
                    drop(send);
                    let _ =
                        tokio::time::timeout(std::time::Duration::from_secs(5), pump_room.close())
                            .await;
                    break;
                }
            }
        });
        Ok(Self {
            event_pump,
            sender_index: AtomicU8::new(0),
            _trust: trust,
            room,
            events: bounded,
            keys,
            video: VideoTracks::default(),
            evidence: Evidence::default(),
            audio: None,
            microphone: None,
            camera: None,
            capture: None,
            screen: None,
            screen_capture: None,
            consumers: HashMap::new(),
            allowed: HashSet::new(),
            blocked: HashSet::new(),
            installed: HashMap::new(),
            owners: HashMap::new(),
        })
    }
    /// Replace the set of call members whose media may be received. Departed
    /// members are unsubscribed and their installed keys overwritten with
    /// fresh random material, so frames sent under old keys stop decrypting.
    pub fn set_members(&mut self, identities: impl IntoIterator<Item = String>) {
        let next: HashSet<String> = identities.into_iter().collect();
        let departed: Vec<String> = self.allowed.difference(&next).cloned().collect();
        self.allowed = next;
        for identity in &departed {
            self.forget(identity);
        }
        for peer in self.room.remote_participants().values() {
            let identity = peer.identity().to_string();
            for publication in peer.track_publications().values() {
                self.decide(&identity, publication);
            }
        }
    }
    /// True when this member published media without end-to-end encryption.
    pub fn is_blocked(&self, identity: &str) -> bool {
        self.blocked.contains(identity)
    }
    fn decide(&mut self, identity: &str, publication: &RemoteTrackPublication) {
        let encrypted = publication.encryption_type() == EncryptionType::Gcm;
        if !encrypted && self.allowed.contains(identity) {
            self.blocked.insert(identity.to_owned());
        }
        let wanted = encrypted && self.allowed.contains(identity);
        if publication.is_subscribed() != wanted {
            publication.set_subscribed(wanted);
        }
        if !wanted {
            self.stop_track(&publication.sid().to_string());
        }
    }
    fn forget(&mut self, identity: &str) {
        self.blocked.remove(identity);
        let tracks: Vec<String> = self
            .owners
            .iter()
            .filter(|(_, owner)| owner.as_str() == identity)
            .map(|(sid, _)| sid.clone())
            .collect();
        for sid in tracks {
            self.stop_track(&sid);
        }
        for index in self.installed.remove(identity).unwrap_or_default() {
            let mut junk = vec![0; 16];
            if getrandom::fill(&mut junk).is_ok() {
                self.keys.set_key(&identity.into(), index as i32, junk);
            }
        }
    }
    pub async fn start_microphone(&mut self) -> Result<(), MediaError> {
        let audio = livekit::PlatformAudio::new().map_err(|_| MediaError::DeviceUnavailable)?;
        let source = audio.rtc_source();
        self.audio = Some(audio);
        self.publish_audio(source).await
    }
    /// Used by isolated fixtures for an explicit generated tone; never selected
    /// by the application device path.
    pub async fn publish_audio(&mut self, source: RtcAudioSource) -> Result<(), MediaError> {
        let track = LocalAudioTrack::create_audio_track("microphone", source);
        track.disable();
        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .map_err(|_| MediaError::Connection)?;
        self.activate_sender(&track.sid()).await?;
        track.enable();
        self.microphone = Some(track);
        Ok(())
    }
    pub async fn publish_video(
        &mut self,
        source: livekit::webrtc::video_source::RtcVideoSource,
    ) -> Result<(), MediaError> {
        let track = LocalVideoTrack::create_video_track("camera", source);
        track.disable();
        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    video_codec: VideoCodec::VP8,
                    ..Default::default()
                },
            )
            .await
            .map_err(|_| MediaError::Connection)?;
        self.activate_sender(&track.sid()).await?;
        track.enable();
        self.consume_video(format!("local:{}", track.sid()), track.rtc_track());
        self.camera = Some(track);
        Ok(())
    }
    pub fn set_muted(&self, muted: bool) -> Result<(), MediaError> {
        let track = self
            .microphone
            .as_ref()
            .ok_or(MediaError::DeviceUnavailable)?;
        if muted {
            track.mute()
        } else {
            track.unmute()
        }
        Ok(())
    }
    pub async fn configure_camera(&mut self, id: Option<&str>) -> Result<(), MediaError> {
        let Some(id) = id else {
            self.configure_camera_off().await;
            return Ok(());
        };
        let capture = camera::Camera::start(id).await?;
        self.configure_camera_off().await;
        self.publish_video(capture.source()).await?;
        self.capture = Some(capture);
        Ok(())
    }
    async fn configure_camera_off(&mut self) {
        if let Some(track) = self.camera.take() {
            let _ = self
                .room
                .local_participant()
                .unpublish_track(&track.sid())
                .await;
            self.stop_track(&format!("local:{}", track.sid()));
        }
        self.capture = None;
    }
    /// Starts screen capture (and the system picker where there is one).
    /// Publish with `publish_screen` once `ScreenStatus::Live` arrives.
    pub async fn begin_screen_share(
        &mut self,
    ) -> Result<mpsc::Receiver<screen::ScreenStatus>, MediaError> {
        self.stop_screen_share().await;
        let (send, receive) = mpsc::channel(4);
        self.screen_capture = Some(screen::ScreenCapture::start(send)?);
        Ok(receive)
    }
    pub async fn publish_screen(&mut self) -> Result<(), MediaError> {
        let source = self
            .screen_capture
            .as_ref()
            .ok_or(MediaError::DeviceUnavailable)?
            .source();
        self.publish_screen_source(source).await
    }
    /// Publishes `source` as the screen share. Fixtures use it for a
    /// generated screen; the application goes through `publish_screen`.
    pub async fn publish_screen_source(
        &mut self,
        source: livekit::webrtc::video_source::RtcVideoSource,
    ) -> Result<(), MediaError> {
        let track = LocalVideoTrack::create_video_track("screen", source);
        track.disable();
        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Screenshare,
                    video_codec: VideoCodec::VP8,
                    simulcast: false,
                    ..Default::default()
                },
            )
            .await
            .map_err(|_| MediaError::Connection)?;
        self.activate_sender(&track.sid()).await?;
        track.enable();
        self.consume_video(format!("{LOCAL_SCREEN}{}", track.sid()), track.rtc_track());
        self.screen = Some(track);
        Ok(())
    }
    pub async fn stop_screen_share(&mut self) {
        self.screen_capture = None;
        if let Some(track) = self.screen.take() {
            let _ = self
                .room
                .local_participant()
                .unpublish_track(&track.sid())
                .await;
            self.stop_track(&format!("{LOCAL_SCREEN}{}", track.sid()));
        }
    }
    pub fn set_video(&self, video: bool) -> Result<(), MediaError> {
        let track = self.camera.as_ref().ok_or(MediaError::DeviceUnavailable)?;
        if video {
            track.unmute()
        } else {
            track.mute()
        }
        Ok(())
    }
    pub fn uses_microphone(&self) -> bool {
        self.audio.is_some()
    }
    pub fn devices(&self) -> Devices {
        let Some(audio) = &self.audio else {
            return Devices {
                cameras: camera::devices(),
                ..Default::default()
            };
        };
        Devices {
            inputs: audio
                .recording_devices()
                .map(|d| Device {
                    id: d.id.to_string(),
                    name: d.name,
                })
                .collect(),
            outputs: audio
                .playout_devices()
                .map(|d| Device {
                    id: d.id.to_string(),
                    name: d.name,
                })
                .collect(),
            cameras: camera::devices(),
        }
    }
    pub fn select_input(&self, id: &str) -> Result<(), MediaError> {
        let audio = self.audio.as_ref().ok_or(MediaError::DeviceUnavailable)?;
        let device = audio
            .recording_devices()
            .find(|d| d.id.as_str() == id)
            .ok_or(MediaError::DeviceUnavailable)?;
        audio
            .switch_recording_device(&device.id)
            .map_err(|_| MediaError::DeviceUnavailable)
    }
    pub fn select_output(&self, id: &str) -> Result<(), MediaError> {
        let audio = self.audio.as_ref().ok_or(MediaError::DeviceUnavailable)?;
        let device = audio
            .playout_devices()
            .find(|d| d.id.as_str() == id)
            .ok_or(MediaError::DeviceUnavailable)?;
        audio
            .switch_playout_device(&device.id)
            .map_err(|_| MediaError::DeviceUnavailable)
    }
    pub fn install_key(&mut self, identity: &str, index: u8, key: &[u8]) -> Result<(), MediaError> {
        if !self
            .keys
            .set_key(&identity.into(), index as i32, key.to_vec())
        {
            return Err(MediaError::E2eeRequired);
        }
        self.installed
            .entry(identity.to_owned())
            .or_default()
            .insert(index);
        Ok(())
    }
    async fn activate_sender(&self, sid: &livekit::id::TrackSid) -> Result<(), MediaError> {
        let own = self.room.local_participant().identity();
        for _ in 0..20 {
            if let Some(cryptor) = self
                .room
                .e2ee_manager()
                .frame_cryptors()
                .get(&(own.clone(), sid.clone()))
            {
                cryptor.set_key_index(self.sender_index.load(Ordering::Relaxed) as i32);
                if cryptor.enabled() {
                    return Ok(());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        Err(MediaError::E2eeRequired)
    }
    pub fn set_sender_index(&self, index: u8) {
        self.sender_index.store(index, Ordering::Relaxed);
        let own = self.room.local_participant().identity();
        for ((identity, _), cryptor) in self.room.e2ee_manager().frame_cryptors() {
            if identity == own {
                cryptor.set_key_index(index as i32);
            }
        }
    }
    fn stop_track(&mut self, id: &str) {
        self.owners.remove(id);
        if let Some(task) = self.consumers.remove(id) {
            task.abort();
        }
        // Retiring the slot prevents an already-running decoder from writing
        // one final frame after removal, even if a UI still holds the handle.
        self.video.remove(id);
    }
    pub fn handle_event(&mut self, event: &RoomEvent) {
        match event {
            RoomEvent::Reconnected => {
                // Republished cryptors must resume the last active Matrix key
                // index, including the old index during a delayed rotation.
                self.set_sender_index(self.sender_index.load(Ordering::Relaxed));
            }
            RoomEvent::TrackPublished {
                publication,
                participant,
            } => self.decide(&participant.identity().to_string(), publication),
            RoomEvent::TrackSubscribed {
                track,
                publication,
                participant,
            } => {
                let identity = participant.identity().to_string();
                // A subscription can race a departure or arrive for an
                // unencrypted publication: drop it instead of consuming it.
                if publication.encryption_type() != EncryptionType::Gcm
                    || !self.allowed.contains(&identity)
                {
                    self.decide(&identity, publication);
                    return;
                }
                let id = track.sid().to_string();
                match track {
                    RemoteTrack::Video(track) => self.consume_video(id.clone(), track.rtc_track()),
                    // Remote audio plays through the platform audio device
                    // module; a sink is only attached to count test evidence.
                    #[cfg(feature = "evidence")]
                    RemoteTrack::Audio(track) => self.consume_audio(id.clone(), track.rtc_track()),
                    #[cfg(not(feature = "evidence"))]
                    RemoteTrack::Audio(_) => {}
                }
                self.owners.insert(id, identity);
            }
            RoomEvent::TrackUnsubscribed { track, .. } => {
                let id = track.sid().to_string();
                self.stop_track(&id);
            }
            _ => {}
        }
    }
    #[cfg(feature = "evidence")]
    fn consume_audio(&mut self, id: String, track: livekit::webrtc::audio_track::RtcAudioTrack) {
        self.stop_track(&id);
        let evidence = self.evidence.clone();
        let mut stream = NativeAudioStream::new(track, 48000, 1);
        self.consumers.insert(
            id,
            tokio::spawn(async move {
                while let Some(frame) = stream.next().await {
                    evidence.audio_frames.fetch_add(1, Ordering::Relaxed);
                    let sum: f64 = frame.data.iter().map(|s| (*s as f64).powi(2)).sum();
                    if !frame.data.is_empty() && (sum / frame.data.len() as f64).sqrt() > 200.0 {
                        evidence.audio_signal_frames.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }),
        );
    }
    fn consume_video(&mut self, id: String, track: livekit::webrtc::video_track::RtcVideoTrack) {
        self.stop_track(&id);
        let slot = self.video.insert(id.clone());
        #[cfg(feature = "evidence")]
        let evidence = self.evidence.clone();
        let mut stream = NativeVideoStream::new(track);
        self.consumers.insert(
            id.clone(),
            tokio::spawn(async move {
                while let Some(frame) = stream.next().await {
                    let width = frame.buffer.width();
                    let height = frame.buffer.height();
                    if width == 0 || height == 0 || width > 3840 || height > 2160 {
                        continue;
                    }
                    let mut bgra = vec![0; width as usize * height as usize * 4];
                    // libyuv ARGB is BGRA byte order on little-endian desktop platforms.
                    frame.buffer.to_argb(
                        VideoFormatType::ARGB,
                        &mut bgra,
                        width * 4,
                        width as i32,
                        height as i32,
                    );
                    if let Ok(frame) = VideoFrame::new(
                        width,
                        height,
                        frame.rotation as u16,
                        frame.timestamp_us,
                        bgra,
                    ) {
                        #[cfg(feature = "evidence")]
                        if !id.starts_with("local:") {
                            evidence.video_frames.fetch_add(1, Ordering::Relaxed);
                            let left = (frame.height as usize / 2 * frame.width as usize
                                + frame.width as usize / 6)
                                * 4;
                            let right = (frame.height as usize / 2 * frame.width as usize
                                + frame.width as usize * 5 / 6)
                                * 4;
                            if frame.bgra[right] as i32 - frame.bgra[left] as i32 > 100 {
                                evidence
                                    .video_pattern_frames
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        slot.replace(frame);
                    }
                }
            }),
        );
    }
    pub async fn close(&self) {
        let _ = self.room.close().await;
    }
}
impl Drop for NativeMedia {
    fn drop(&mut self) {
        self.event_pump.abort();
        for task in self.consumers.values() {
            task.abort();
        }
        self.video.clear();
        let room = self.room.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = room.close().await;
            });
        }
    }
}

/// Camera capture adapters feed an SDK source, independently of the UI.
pub trait CaptureSource: Send {
    fn source(&self) -> livekit::webrtc::video_source::RtcVideoSource;
    fn stop(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_frame_is_bounded_rejects_stale_and_clears_on_track_removal() {
        let tracks = VideoTracks::default();
        let slot = tracks.insert("track".into());
        for i in 0..1000 {
            slot.replace(VideoFrame::new(1, 1, 0, i, vec![0; 4]).unwrap());
        }
        assert_eq!(slot.latest().unwrap().timestamp_us, 999);
        slot.replace(VideoFrame::new(1, 1, 0, 1, vec![0; 4]).unwrap());
        assert_eq!(slot.latest().unwrap().timestamp_us, 999);
        let frame = slot.latest().unwrap();
        assert_eq!(Arc::strong_count(&frame), 2);
        tracks.remove("track");
        assert!(slot.latest().is_none());
        slot.replace(VideoFrame::new(1, 1, 0, 1001, vec![0; 4]).unwrap());
        assert!(
            slot.latest().is_none(),
            "late decoder cannot revive a retired handle"
        );
        let replacement = tracks.insert("track".into());
        replacement.replace(VideoFrame::new(1, 1, 0, 1, vec![0; 4]).unwrap());
        assert_eq!(replacement.latest().unwrap().timestamp_us, 1);
        assert!(slot.latest().is_none());
        assert_eq!(Arc::strong_count(&frame), 1);
        assert!(VideoFrame::new(4000, 1, 0, 0, vec![]).is_err());
    }
}
