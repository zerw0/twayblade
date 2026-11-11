//! One native call per account. A core task owns signalling and media; the
//! backend watches confirmed state without cancelling in-flight operations.
use super::{
    keys::{KeyReceiver, SenderKeys},
    native::{Member, Signalling, members},
    *,
};
use crate::auth::Account;
use tokio::sync::OwnedSemaphorePermit;
use twayblade_media::{
    MediaError, NativeMedia,
    livekit::{
        self,
        prelude::{RoomEvent, TrackSource},
    },
};

pub enum NativeControl {
    Muted(bool),
    Video(bool),
    Input(String),
    Output(String),
    Camera(String),
    Leave,
}

pub struct NativeCall {
    signalling: Signalling,
    keys: SenderKeys,
    receiver: KeyReceiver,
    pub(crate) media: NativeMedia,
    pub state: CallState,
    tick: tokio::time::Interval,
    pub keys_received: u64,
    pub keys_sent: u64,
}
impl From<MediaError> for CallError {
    fn from(value: MediaError) -> Self {
        match value {
            MediaError::E2eeRequired => Self::MediaE2eeRequired,
            MediaError::DeviceUnavailable => Self::DeviceUnavailable,
            _ => Self::TransportFailed,
        }
    }
}
impl From<twayblade_platform::media_permissions::PermissionError> for CallError {
    fn from(value: twayblade_platform::media_permissions::PermissionError) -> Self {
        match value {
            twayblade_platform::media_permissions::PermissionError::Restricted(_) => {
                Self::PermissionRestricted
            }
            _ => Self::PermissionDenied,
        }
    }
}
impl NativeCall {
    pub async fn start(account: &Account, room: &str, video: bool) -> Result<Self, CallError> {
        let lease = account
            .call_slot
            .clone()
            .try_acquire_owned()
            .map_err(|_| CallError::AlreadyActive)?;
        twayblade_platform::media_permissions::request(
            twayblade_platform::media_permissions::CapturePermission::Microphone,
        )
        .await?;
        if video {
            twayblade_platform::media_permissions::request(
                twayblade_platform::media_permissions::CapturePermission::Camera,
            )
            .await?;
        }
        let mut call = Self::connect(account, room, lease).await?;
        call.media.start_microphone().await?;
        // Camera capture is attached below through the native capture adapter.
        if video {
            call.control(NativeControl::Video(true)).await?;
        }
        call.state.local.muted = false;
        call.state.muted = Some(false);
        Ok(call)
    }
    pub(crate) async fn connect(
        account: &Account,
        room: &str,
        lease: OwnedSemaphorePermit,
    ) -> Result<Self, CallError> {
        let (mut signalling, credentials) = account.prepare_native_call(room, true).await?;
        signalling.lease = Some(lease);
        #[cfg(test)]
        eprintln!("Native fixture: focus and JWT obtained");
        let keys = SenderKeys::new()?;
        let provider = twayblade_media::matrix_key_provider();
        if !provider.set_key(
            &signalling.own.identity().into(),
            keys.index as i32,
            keys.key.bytes().to_vec(),
        ) {
            return Err(CallError::MediaE2eeRequired);
        }
        let receiver = KeyReceiver::new(signalling.room.clone());
        signalling.join().await?;
        #[cfg(test)]
        eprintln!("Native fixture: membership published");
        let media = NativeMedia::connect(
            &credentials.url,
            &credentials.jwt,
            provider,
            credentials.http,
        )
        .await?;
        #[cfg(test)]
        eprintln!("Native fixture: encrypted SFU transport connected");
        let direct = signalling.room.is_direct().await.unwrap_or(false);
        let state = CallState {
            room_id: room.into(),
            kind: if direct {
                CallKind::Direct
            } else {
                CallKind::Group
            },
            phase: CallPhase::Joining,
            participants: Vec::new(),
            local: LocalCallState {
                muted: true,
                ..Default::default()
            },
            devices: Default::default(),
            video_tracks: Vec::new(),
            muted: Some(true),
            video: Some(false),
            screen_sharing: Some(false),
            error: None,
        };
        Ok(Self {
            signalling,
            keys,
            receiver,
            media,
            state,
            tick: tokio::time::interval(native::KEEPALIVE),
            keys_received: 0,
            keys_sent: 0,
        })
    }
    pub fn video_tracks(&self) -> VideoTracks {
        self.media.video.clone()
    }
    pub fn evidence(&self) -> twayblade_media::Evidence {
        self.media.evidence.clone()
    }
    pub async fn control(&mut self, control: NativeControl) -> Result<(), CallError> {
        match control {
            NativeControl::Leave => return self.leave().await,
            NativeControl::Muted(value) => {
                self.media.set_muted(value)?;
                self.state.local.muted = value;
                self.state.muted = Some(value);
            }
            NativeControl::Video(value) => {
                if value {
                    twayblade_platform::media_permissions::request(
                        twayblade_platform::media_permissions::CapturePermission::Camera,
                    )
                    .await?;
                }
                let id = if value {
                    Some(
                        self.state
                            .local
                            .camera
                            .clone()
                            .or_else(|| {
                                twayblade_media::camera::devices()
                                    .first()
                                    .map(|d| d.id.clone())
                            })
                            .ok_or(CallError::DeviceUnavailable)?,
                    )
                } else {
                    None
                };
                self.media.configure_camera(id.as_deref()).await?;
                if id.is_some() {
                    self.state.local.camera = id;
                }
                self.state.local.video = value;
                self.state.video = Some(value);
            }
            NativeControl::Input(id) => {
                self.media.select_input(&id)?;
                self.state.local.input_device = Some(id);
            }
            NativeControl::Output(id) => {
                self.media.select_output(&id)?;
                self.state.local.output_device = Some(id);
            }
            NativeControl::Camera(id) => {
                if !twayblade_media::camera::devices()
                    .iter()
                    .any(|d| d.id == id)
                {
                    return Err(CallError::DeviceUnavailable);
                }
                if self.state.local.video {
                    self.media.configure_camera(Some(&id)).await?;
                }
                self.state.local.camera = Some(id);
            }
        }
        Ok(())
    }
    /// Poll on the backend runtime. Every accepted key and participant/device
    /// projection is room/call-scoped. No frame payloads enter CallState.
    async fn next_step(
        &mut self,
        commands: &mut tokio::sync::mpsc::Receiver<DriverCommand>,
    ) -> Result<(), CallError> {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(DriverCommand::Control(NativeControl::Leave)) | None => {
                        self.leave().await?;
                    }
                    Some(DriverCommand::Control(control)) => {
                        self.state.error = self.control(control).await.err();
                    }
                    #[cfg(test)]
                    Some(DriverCommand::Video(source, response)) => {
                        let result = self.media.publish_video(source).await.map_err(CallError::from);
                        let _ = response.send(result);
                    }
                }
            }
            _ = self.tick.tick() => {
                self.signalling.maintain().await?;
                self.refresh_members(true).await?;
            }
            changed = self.signalling.membership_updates.changed() => {
                changed.map_err(|_| CallError::TransportFailed)?;
                self.refresh_members(false).await?;
            }
            key = self.receiver.receive.recv() => {
                let key = key.ok_or(CallError::KeyRejected)?;
                // Recheck current membership after bounded queue residence.
                let current = members(&self.signalling.room).await?;
                if current.iter().any(|m| m.identity() == key.identity) {
                    self.media.install_key(&key.identity, key.index, key.key.bytes())?;
                    self.keys_received += 1;
                }
            }
            event = self.media.events.recv() => {
                let event = event.ok_or(CallError::TransportFailed)?;
                if matches!(event, RoomEvent::Disconnected { .. }) {
                    return Err(CallError::TransportFailed);
                }
                if let RoomEvent::TrackSubscribed { publication, .. } = &event
                    && publication.encryption_type() != livekit::e2ee::EncryptionType::Gcm
                {
                    return Err(CallError::MediaE2eeRequired);
                }
                if let RoomEvent::TrackPublished { publication, .. } = &event {
                    if publication.encryption_type() != livekit::e2ee::EncryptionType::Gcm {
                        return Err(CallError::MediaE2eeRequired);
                    }
                    publication.set_subscribed(true);
                }
                self.media.handle_event(&event);
                let current = members(&self.signalling.room).await?;
                self.project(&current).await;
            }
        }
        // Connected means encrypted transport is established, not that a person
        // has accepted microphone/camera quality. Evidence counts are separate.
        if !matches!(self.state.phase, CallPhase::Ended(_)) {
            self.state.phase = CallPhase::Connected;
        }
        Ok(())
    }
    async fn refresh_members(&mut self, resend: bool) -> Result<(), CallError> {
        let current = members(&self.signalling.room).await?;
        let changed = self.keys.update_members(&self.signalling.own, &current)?;
        if changed || resend {
            self.media.install_key(
                &self.signalling.own.identity(),
                self.keys.index,
                self.keys.key.bytes(),
            )?;
            // Fail closed on departure, even while a remaining device's key
            // delivery is retrying. Never keep encrypting with the departed key.
            self.media.set_sender_index(self.keys.index);
            if tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.keys.send(&self.signalling.room, &self.signalling.own),
            )
            .await
            .is_ok_and(|r| r.is_ok())
            {
                self.keys_sent += 1;
            }
        }
        self.project(&current).await;
        Ok(())
    }
    async fn project(&mut self, current: &[Member]) {
        let peers = self.media.room.remote_participants();
        let own = &self.signalling.own;
        self.state.participants.clear();
        self.state.video_tracks.clear();
        for member in current {
            let peer = peers.get(&member.identity().into());
            let local = member.user_id == own.user_id && member.device_id == own.device_id;
            let name = self
                .signalling
                .room
                .get_member_no_sync(&member.user_id)
                .await
                .ok()
                .flatten()
                .and_then(|m| m.display_name().map(str::to_owned));
            let mut muted = None;
            let mut video = None;
            if let Some(peer) = peer {
                for publication in peer.track_publications().values() {
                    match publication.source() {
                        TrackSource::Microphone => muted = Some(publication.is_muted()),
                        TrackSource::Camera => {
                            video = Some(!publication.is_muted());
                            if self
                                .media
                                .video
                                .get(&publication.sid().to_string())
                                .is_some()
                            {
                                self.state.video_tracks.push(CallVideoTrack {
                                    id: publication.sid().to_string(),
                                    user_id: member.user_id.to_string(),
                                    device_id: member.device_id.clone(),
                                    local: false,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            if local {
                muted = Some(self.state.local.muted);
                video = Some(self.state.local.video);
                for id in self
                    .media
                    .video
                    .ids()
                    .into_iter()
                    .filter(|id| id.starts_with("local:"))
                {
                    self.state.video_tracks.push(CallVideoTrack {
                        id,
                        user_id: member.user_id.to_string(),
                        device_id: member.device_id.clone(),
                        local: true,
                    });
                }
            }
            let connection_quality = peer.map(|p| match p.connection_quality() {
                livekit::participant::ConnectionQuality::Excellent => ConnectionQuality::Excellent,
                livekit::participant::ConnectionQuality::Good => ConnectionQuality::Good,
                livekit::participant::ConnectionQuality::Poor => ConnectionQuality::Poor,
                livekit::participant::ConnectionQuality::Lost => ConnectionQuality::Lost,
            });
            self.state.participants.push(Participant {
                user_id: member.user_id.to_string(),
                device_id: member.device_id.clone(),
                display_name: name,
                speaking: peer.map(|p| p.is_speaking()),
                muted,
                video,
                connection_quality,
            });
        }
        self.state.devices = self.media.devices();
    }
    pub async fn leave(&mut self) -> Result<(), CallError> {
        self.media.close().await;
        self.signalling.leave().await?;
        self.state.phase = CallPhase::Ended(EndReason::Left);
        Ok(())
    }
}
impl Drop for NativeCall {
    fn drop(&mut self) {
        self.state.phase = CallPhase::Ended(EndReason::Left);
    }
}

/// Runs signalling/media continuously. Watching snapshots is cancellation-safe;
/// UI queues and fixture polling cannot cancel a membership refresh/key send.
pub struct CallHandle {
    pub updates: tokio::sync::watch::Receiver<CallState>,
    commands: tokio::sync::mpsc::Sender<DriverCommand>,
    video: VideoTracks,
    evidence: twayblade_media::Evidence,
    keys_received: std::sync::Arc<std::sync::atomic::AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}
enum DriverCommand {
    Control(NativeControl),
    #[cfg(test)]
    Video(
        twayblade_media::livekit::webrtc::video_source::RtcVideoSource,
        tokio::sync::oneshot::Sender<Result<(), CallError>>,
    ),
}
impl NativeCall {
    pub fn drive(mut self) -> CallHandle {
        let (commands, mut receive) = tokio::sync::mpsc::channel(32);
        let (send, updates) = tokio::sync::watch::channel(self.state.clone());
        let video = self.video_tracks();
        let evidence = self.evidence();
        let keys_received = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let received = keys_received.clone();
        let task = tokio::spawn(async move {
            loop {
                if let Err(error) = self.next_step(&mut receive).await {
                    self.state.error = Some(error);
                    self.state.phase = CallPhase::Ended(EndReason::Failed);
                }
                received.store(self.keys_received, std::sync::atomic::Ordering::Relaxed);
                send.send_replace(self.state.clone());
                if matches!(self.state.phase, CallPhase::Ended(_)) {
                    break;
                }
            }
        });
        CallHandle {
            updates,
            commands,
            video,
            evidence,
            keys_received,
            task,
        }
    }
}
impl CallHandle {
    pub fn control(&self, control: NativeControl) -> Result<(), CallError> {
        self.commands
            .try_send(DriverCommand::Control(control))
            .map_err(|_| CallError::ControlUnavailable)
    }
    pub fn video_tracks(&self) -> VideoTracks {
        self.video.clone()
    }
    pub fn evidence(&self) -> twayblade_media::Evidence {
        self.evidence.clone()
    }
    pub fn keys_received(&self) -> u64 {
        self.keys_received
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(test)]
    pub(crate) async fn publish_video(
        &self,
        source: twayblade_media::livekit::webrtc::video_source::RtcVideoSource,
    ) -> Result<(), CallError> {
        let (send, receive) = tokio::sync::oneshot::channel();
        self.commands
            .send(DriverCommand::Video(source, send))
            .await
            .map_err(|_| CallError::ControlUnavailable)?;
        receive.await.map_err(|_| CallError::ControlUnavailable)?
    }
    pub async fn leave(&mut self) -> Result<(), CallError> {
        self.control(NativeControl::Leave)?;
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if matches!(self.updates.borrow().phase, CallPhase::Ended(_)) {
                    return self.updates.borrow().error.map_or(Ok(()), Err);
                }
                self.updates
                    .changed()
                    .await
                    .map_err(|_| CallError::SessionClosed)?;
            }
        })
        .await
        .map_err(|_| CallError::TransportFailed)?
    }
}
impl Drop for CallHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}
