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
    LOCAL_SCREEN, MediaError, NativeMedia,
    livekit::{
        self,
        prelude::{RoomEvent, TrackSource},
    },
    screen::ScreenStatus,
};

pub enum NativeControl {
    Muted(bool),
    Video(bool),
    Input(String),
    Output(String),
    Camera(String),
    /// Start (opening the system picker where there is one) or stop sharing.
    Screen(bool),
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
    /// The last key distribution failed or timed out; retry on the next tick.
    keys_pending: bool,
    /// A rotated key index to switch to once members had time to receive it.
    switch_at: Option<(u8, tokio::time::Instant)>,
    ring: Option<super::ringing::OutgoingRing>,
    declines: tokio::sync::mpsc::Receiver<String>,
    _decline_guard: matrix_sdk::event_handler::EventHandlerDropGuard,
    reconnecting: Option<tokio::time::Instant>,
    media_reconnecting: bool,
    signalling_pending: bool,
    device_changes: tokio::sync::watch::Receiver<u64>,
    permission_tick: tokio::time::Interval,
    event_serial: u64,
    /// Capture status while a screen share is being chosen or sent.
    screen_events: Option<tokio::sync::mpsc::Receiver<ScreenStatus>>,
}
/// After a rotation, keep encrypting with the previous key this long so the
/// remaining members receive the new one first (matrix-js-sdk MatrixRTCSession
/// `useKeyDelay` default, as used by Element Call 0.28.2). Departed members
/// are already unsubscribed and cannot send media we accept.
const USE_KEY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);
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
        // CoreAudio UIDs and WebRTC IDs can disagree; keep WebRTC's choice then.
        if let Some(id) = twayblade_platform::media_devices::default_audio(true)
            && call.media.select_input(&id).is_ok()
        {
            call.state.local.input_device = Some(id);
        }
        if let Some(id) = twayblade_platform::media_devices::default_audio(false)
            && call.media.select_output(&id).is_ok()
        {
            call.state.local.output_device = Some(id);
        }
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
            events: Vec::new(),
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
        let (send, declines) = tokio::sync::mpsc::channel(8);
        let handler = signalling.room.client().add_room_event_handler(
            signalling.room.room_id(),
            move |_event: matrix_sdk::ruma::events::AnySyncTimelineEvent,
                  raw: matrix_sdk::event_handler::RawEvent| {
                let send = send.clone();
                async move {
                    if raw.0.get().len() > 16_384 {
                        return;
                    }
                    let Ok(event) = serde_json::from_str::<serde_json::Value>(raw.0.get()) else {
                        return;
                    };
                    if event["type"] == super::ringing::DECLINE_EVENT
                        && let (Some(parent), Some(sender)) = (
                            super::ringing::reference(&event["content"]),
                            event["sender"].as_str(),
                        )
                    {
                        let _ = send.try_send(format!("{parent}\n{sender}"));
                    }
                }
            },
        );
        let decline_guard = signalling.room.client().event_handler_drop_guard(handler);
        Ok(Self {
            device_changes: twayblade_platform::media_devices::changes(),
            permission_tick: tokio::time::interval(std::time::Duration::from_secs(2)),
            event_serial: 0,
            screen_events: None,
            ring: None,
            declines,
            _decline_guard: decline_guard,
            reconnecting: None,
            media_reconnecting: false,
            signalling_pending: false,
            signalling,
            keys,
            receiver,
            media,
            state,
            tick: tokio::time::interval(native::KEEPALIVE),
            keys_received: 0,
            keys_sent: 0,
            keys_pending: true,
            switch_at: None,
        })
    }
    /// Call only for a new call, after membership is visible. Joins never notify.
    pub async fn ring(&mut self, video: bool) -> Result<(), CallError> {
        use super::ringing::*;
        let room = &self.signalling.room;
        if members(room)
            .await?
            .iter()
            .any(|m| m.user_id != self.signalling.own.user_id)
        {
            return Ok(());
        }
        let parent = self
            .signalling
            .membership_event_id
            .clone()
            .ok_or(CallError::TransportFailed)?;
        // Modern Element readers require an open slot, including Compatibility mode.
        room.send_state_event_raw(SLOT_EVENT, SLOT, serde_json::json!({"status":"open","application":{"type":"m.call"},"encryption":{"type":"org.matrix.msc4143.per_member"}})).await.ok(); // Groups often forbid custom state; still notify.
        let users = room
            .members_no_sync(matrix_sdk::RoomMemberships::JOIN)
            .await
            .map_err(|_| CallError::TransportFailed)?
            .iter()
            .filter(|m| m.user_id() != self.signalling.own.user_id)
            .map(|m| m.user_id().to_string())
            .take(256)
            .collect();
        let now = now_ms();
        let response = room
            .send_raw(
                NOTIFICATION_EVENT,
                notification_content(
                    &parent,
                    video,
                    self.state.kind == CallKind::Direct,
                    users,
                    now,
                ),
            )
            .await
            .map_err(|_| CallError::TransportFailed)?;
        self.ring = Some(OutgoingRing {
            event_id: response.response.event_id.to_string(),
            since: now,
            expires: now + RING_LIFETIME_MS,
        });
        self.state.phase = CallPhase::Ringing {
            since: now,
            expires: now + RING_LIFETIME_MS,
        };
        Ok(())
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
            NativeControl::Screen(true) => {
                if self.state.local.screen == ScreenShare::Off {
                    self.screen_events = Some(
                        self.media
                            .begin_screen_share()
                            .await
                            .map_err(|_| CallError::ScreenUnavailable)?,
                    );
                    self.state.local.screen = ScreenShare::Choosing;
                }
            }
            NativeControl::Screen(false) => self.stop_screen().await,
        }
        Ok(())
    }
    /// Poll on the backend runtime. Every accepted key and participant/device
    /// projection is room/call-scoped. No frame payloads enter CallState.
    async fn next_step(
        &mut self,
        commands: &mut tokio::sync::mpsc::Receiver<DriverCommand>,
    ) -> Result<(), CallError> {
        let switch = self.switch_at;
        let reconnect_deadline = self
            .reconnecting
            .map(|at| at + std::time::Duration::from_secs(45));
        let ring_remaining = self.ring.as_ref().map(|ring| {
            std::time::Duration::from_millis(ring.expires.saturating_sub(super::ringing::now_ms()))
        });
        let screen = self.screen_events.as_mut();
        tokio::select! {
            status = async move { match screen { Some(events) => events.recv().await, None => std::future::pending().await } } => {
                if status == Some(ScreenStatus::Live) && self.state.local.screen == ScreenShare::Choosing {
                    match self.media.publish_screen().await {
                        Ok(()) => {
                            self.state.local.screen = ScreenShare::Live;
                            self.state.screen_sharing = Some(true);
                        }
                        Err(error) => {
                            self.stop_screen().await;
                            self.state.error = Some(error.into());
                        }
                    }
                } else if status != Some(ScreenStatus::Live) {
                    // Picker cancelled, sharing stopped from the system, or capture failed.
                    self.stop_screen().await;
                }
                let current = members(&self.signalling.room).await?;
                self.project(&current).await;
            }
            changed = self.device_changes.changed() => {
                changed.map_err(|_|CallError::DeviceUnavailable)?;
                self.refresh_devices().await;
            }
            _ = self.permission_tick.tick() => { self.check_permissions().await; }
            _ = async move { match reconnect_deadline {Some(at)=>tokio::time::sleep_until(at).await, None=>std::future::pending().await} } => {return Err(CallError::ReconnectTimeout);}
            _ = async move { match ring_remaining {Some(duration)=>tokio::time::sleep(duration).await, None=>std::future::pending().await} } => {
                if self.state.kind == CallKind::Direct {
                    self.leave().await?;
                    self.state.phase = CallPhase::Ended(EndReason::RingExpired);
                } else {
                    self.ring = None;
                }
            }
            decline = self.declines.recv() => {
                if let (Some(decline),Some(ring)) = (decline,&self.ring) && self.state.kind == CallKind::Direct && let Some((parent,sender)) = decline.split_once('\n') && parent == ring.event_id && sender != self.signalling.own.user_id.as_str() && self.signalling.room.get_member_no_sync(&matrix_sdk::ruma::UserId::parse(sender).map_err(|_|CallError::InvalidRoom)?).await.ok().flatten().is_some_and(|m|m.membership() == &matrix_sdk::ruma::events::room::member::MembershipState::Join) {
                    let current = members(&self.signalling.room).await?;
                    let joined = current.iter().any(|m|m.user_id != self.signalling.own.user_id);
                    let phase = ring.phase(super::ringing::now_ms(),joined,true);
                    if matches!(phase,CallPhase::Ended(_)) {self.leave().await?;} else {self.ring = None;}
                    self.state.phase = phase;
                }
            }
            _ = async move {
                match switch {
                    Some((_, at)) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some((index, _)) = self.switch_at.take() {
                    self.media.set_sender_index(index);
                }
            }
            command = commands.recv() => {
                match command {
                    #[cfg(test)]
                    Some(DriverCommand::KeyIndex(response)) => { let _ = response.send(self.keys.index); }
                    Some(DriverCommand::Control(NativeControl::Leave)) | None => {
                        self.leave().await?;
                    }
                    Some(DriverCommand::Control(control)) => {
                        self.state.error = self.control(control).await.err();
                    }
                    #[cfg(test)]
                    Some(DriverCommand::Video(source, false, response)) => {
                        let result = self.media.publish_video(source).await.map_err(CallError::from);
                        let _ = response.send(result);
                    }
                    #[cfg(test)]
                    Some(DriverCommand::Video(source, true, response)) => {
                        let result = self.media.publish_screen_source(source).await.map_err(CallError::from);
                        if result.is_ok() {
                            self.state.local.screen = ScreenShare::Live;
                            self.state.screen_sharing = Some(true);
                        }
                        let _ = response.send(result);
                    }
                }
            }
            _ = self.tick.tick() => {
                let result = if self.signalling_pending {self.signalling.recover().await} else {self.signalling.maintain().await};
                match result {
                    Ok(()) => {
                        if self.signalling_pending {self.keys_pending = true;}
                        self.signalling_pending = false;
                        if !self.media_reconnecting {self.reconnecting = None;}
                    }
                    Err(error @ (CallError::InvalidRoom | CallError::MembershipForbidden | CallError::StaleRequest)) => return Err(error),
                    Err(_) => {
                        self.signalling_pending = true;
                        self.reconnecting.get_or_insert_with(tokio::time::Instant::now);
                    }
                }
                self.refresh_members().await?;
            }
            changed = self.signalling.membership_updates.changed() => {
                changed.map_err(|_| CallError::TransportFailed)?;
                self.refresh_members().await?;
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
                match event {
                    RoomEvent::Reconnecting => {
                        self.media_reconnecting = true;
                        self.reconnecting.get_or_insert_with(tokio::time::Instant::now);
                        self.state.phase = CallPhase::Reconnecting;
                    }
                    RoomEvent::Reconnected => {
                        self.media_reconnecting = false;
                        self.signalling_pending = self.signalling.recover().await.is_err();
                        if !self.signalling_pending {self.reconnecting = None;}
                        self.keys_pending = true;
                        self.refresh_members().await?;
                    }
                    _ => {}
                }
                // Media subscribes only to current members' encrypted tracks;
                // an unencrypted publication is left unreceived, not fatal.
                self.media.handle_event(&event);
                let current = members(&self.signalling.room).await?;
                self.project(&current).await;
            }
        }
        // Connected means encrypted transport is established, not that a person
        // has accepted microphone/camera quality. Evidence counts are separate.
        if !matches!(self.state.phase, CallPhase::Ended(_)) {
            self.state.phase = if self.reconnecting.is_some() {
                CallPhase::Reconnecting
            } else if let Some(ring) = &self.ring {
                ring.phase(super::ringing::now_ms(), false, false)
            } else {
                CallPhase::Connected
            };
        }
        Ok(())
    }
    fn event(&mut self, event: CallEvent) {
        self.event_serial = self.event_serial.wrapping_add(1);
        if self.state.events.len() == 16 {
            self.state.events.remove(0);
        }
        self.state.events.push((self.event_serial, event));
    }
    async fn check_permissions(&mut self) {
        use twayblade_platform::media_permissions::{CapturePermission, PermissionError, check};
        // Synthetic sources do not own OS capture grants.
        if self.media.uses_microphone()
            && !self.state.local.muted
            && let Err(error @ (PermissionError::Denied(_) | PermissionError::Restricted(_))) =
                check(CapturePermission::Microphone)
        {
            let _ = self.media.set_muted(true);
            self.state.local.muted = true;
            self.state.muted = Some(true);
            let error = CallError::from(error);
            self.state.error = Some(error);
            self.event(CallEvent::PermissionRevoked {
                kind: DeviceKind::Input,
                error,
            });
        }
        if self.state.local.video
            && let Err(error @ (PermissionError::Denied(_) | PermissionError::Restricted(_))) =
                check(CapturePermission::Camera)
        {
            let _ = self.media.configure_camera(None).await;
            self.state.local.video = false;
            self.state.video = Some(false);
            let error = CallError::from(error);
            self.state.error = Some(error);
            self.event(CallEvent::PermissionRevoked {
                kind: DeviceKind::Camera,
                error,
            });
        }
    }
    async fn refresh_devices(&mut self) {
        let devices = self.media.devices();
        for kind in [DeviceKind::Input, DeviceKind::Output, DeviceKind::Camera] {
            let (selected, list) = match kind {
                DeviceKind::Input => (&self.state.local.input_device, &devices.inputs),
                DeviceKind::Output => (&self.state.local.output_device, &devices.outputs),
                DeviceKind::Camera => (&self.state.local.camera, &devices.cameras),
            };
            let Some(from) = selected
                .clone()
                .filter(|id| !list.iter().any(|d| d.id == *id))
            else {
                continue;
            };
            let default = match kind {
                DeviceKind::Camera => twayblade_media::camera::default_device(),
                _ => twayblade_platform::media_devices::default_audio(kind == DeviceKind::Input)
                    .or_else(|| {
                        list.iter()
                            .find(|d| d.id == "default")
                            .map(|d| d.id.clone())
                    }),
            };
            let to = fallback_device(Some(&from), list, default.as_deref());
            let result = match (kind, to.as_deref()) {
                (DeviceKind::Input, Some(id)) => self.media.select_input(id),
                (DeviceKind::Output, Some(id)) => self.media.select_output(id),
                (DeviceKind::Camera, Some(id)) if self.state.local.video => {
                    self.media.configure_camera(Some(id)).await
                }
                (DeviceKind::Camera, Some(_)) => Ok(()),
                _ => Err(MediaError::DeviceUnavailable),
            };
            if result.is_err() {
                self.state.error = Some(CallError::DeviceUnavailable);
                if kind == DeviceKind::Input {
                    let _ = self.media.set_muted(true);
                    self.state.local.muted = true;
                    self.state.muted = Some(true);
                }
                if kind == DeviceKind::Camera {
                    let _ = self.media.configure_camera(None).await;
                    self.state.local.video = false;
                    self.state.video = Some(false);
                }
            }
            let confirmed = result.ok().and(to);
            match kind {
                DeviceKind::Input => self.state.local.input_device = confirmed.clone(),
                DeviceKind::Output => self.state.local.output_device = confirmed.clone(),
                DeviceKind::Camera => self.state.local.camera = confirmed.clone(),
            }
            self.event(CallEvent::DeviceFallback {
                kind,
                from,
                to: confirmed,
            });
        }
        self.state.devices = devices;
    }
    async fn refresh_members(&mut self) -> Result<(), CallError> {
        let current = members(&self.signalling.room).await?;
        let own = &self.signalling.own;
        if current.iter().any(|m| m.user_id != own.user_id) {
            self.ring = None;
        }
        // Only current call members' media is received.
        self.media.set_members(
            current
                .iter()
                .filter(|m| m.user_id != own.user_id || m.device_id != own.device_id)
                .map(Member::identity),
        );
        let change = self.keys.update_members(own, &current)?;
        if change.rotated {
            // Install the new key for our encryptor now, but switch to it only
            // after the remaining members had time to receive it.
            self.media.install_key(
                &self.signalling.own.identity(),
                self.keys.index,
                self.keys.key.bytes(),
            )?;
            self.switch_at = Some((self.keys.index, tokio::time::Instant::now() + USE_KEY_DELAY));
        }
        // Send on a membership change or to retry a failed delivery, never
        // merely because the keepalive ticked.
        if change.changed || self.keys_pending {
            self.keys_pending = !tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.keys.send(&self.signalling.room, &self.signalling.own),
            )
            .await
            .is_ok_and(|r| r.is_ok());
            if !self.keys_pending {
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
            let mut screen = false;
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
                                    source: VideoSource::Camera,
                                });
                            }
                        }
                        TrackSource::Screenshare if !publication.is_muted() => {
                            screen = true;
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
                                    source: VideoSource::Screen,
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
                screen = self.state.local.screen == ScreenShare::Live;
                let mut ids: Vec<String> = self
                    .media
                    .video
                    .ids()
                    .into_iter()
                    .filter(|id| id.starts_with("local:"))
                    .collect();
                ids.sort();
                for id in ids {
                    let source = if id.starts_with(LOCAL_SCREEN) {
                        VideoSource::Screen
                    } else {
                        VideoSource::Camera
                    };
                    self.state.video_tracks.push(CallVideoTrack {
                        id,
                        user_id: member.user_id.to_string(),
                        device_id: member.device_id.clone(),
                        local: true,
                        source,
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
                media_blocked: self.media.is_blocked(&member.identity()),
                screen,
                connection_quality,
            });
        }
        self.state.devices = self.media.devices();
    }
    async fn stop_screen(&mut self) {
        self.screen_events = None;
        self.media.stop_screen_share().await;
        self.state.local.screen = ScreenShare::Off;
        self.state.screen_sharing = Some(false);
    }
    pub async fn leave(&mut self) -> Result<(), CallError> {
        self.ring = None;
        self.screen_events = None;
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
    #[cfg(test)]
    KeyIndex(tokio::sync::oneshot::Sender<u8>),
    Control(NativeControl),
    /// Fixture media: a generated camera, or a generated screen share.
    #[cfg(test)]
    Video(
        twayblade_media::livekit::webrtc::video_source::RtcVideoSource,
        bool,
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
                    // Finish bounded cleanup before reporting the terminal snapshot.
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), self.leave())
                        .await;
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
    #[cfg(test)]
    pub(crate) async fn sender_key_index(&self) -> u8 {
        let (send, receive) = tokio::sync::oneshot::channel();
        self.commands
            .send(DriverCommand::KeyIndex(send))
            .await
            .unwrap();
        receive.await.unwrap()
    }
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
        screen: bool,
    ) -> Result<(), CallError> {
        let (send, receive) = tokio::sync::oneshot::channel();
        self.commands
            .send(DriverCommand::Video(source, screen, send))
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

/// Preserve a connected device; on removal select only the OS default, never
/// an arbitrary first device (which could unexpectedly capture private audio).
fn fallback_device(
    selected: Option<&str>,
    devices: &[CallDevice],
    default: Option<&str>,
) -> Option<String> {
    selected
        .filter(|id| devices.iter().any(|d| d.id == *id))
        .or_else(|| default.filter(|id| devices.iter().any(|d| d.id == *id)))
        .map(str::to_owned)
}
#[cfg(test)]
mod resilience_tests {
    use super::*;
    #[test]
    fn fallback_preserves_selection_or_uses_only_the_system_default() {
        let devices = vec![
            CallDevice {
                id: "a".into(),
                name: "A".into(),
            },
            CallDevice {
                id: "b".into(),
                name: "B".into(),
            },
        ];
        assert_eq!(
            fallback_device(Some("a"), &devices, Some("b")),
            Some("a".into())
        );
        assert_eq!(
            fallback_device(Some("removed"), &devices, Some("b")),
            Some("b".into())
        );
        assert_eq!(fallback_device(Some("removed"), &devices, None), None);
        assert_eq!(fallback_device(Some("removed"), &[], Some("b")), None);
    }
}
