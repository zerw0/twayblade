//! Native MatrixRTC contracts. Matrix state stays in core; no frontend wording.
mod keys;
pub mod native;
#[cfg(test)]
mod native_integration;
pub mod ringing;
mod session;
use crate::auth::Account;
use matrix_sdk::RoomState;
pub use ringing::{CallActivity, IncomingAction, IncomingCall};
pub use session::{CallHandle, NativeCall, NativeControl};
use std::collections::HashMap;
pub use twayblade_media::{
    Device as CallDevice, Devices as CallDevices, LatestFrame, VideoFrame, VideoTracks,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    #[error("call_no_foci")]
    NoFoci,
    #[error("call_invalid_volume")]
    InvalidVolume,
    #[error("call_media_e2ee_required")]
    MediaE2eeRequired,
    #[error("call_key_rejected")]
    KeyRejected,
    #[error("call_transport_failed")]
    TransportFailed,
    #[error("call_membership_forbidden")]
    MembershipForbidden,
    #[error("call_device_unavailable")]
    DeviceUnavailable,
    /// Screen capture couldn't start on this system.
    #[error("call_screen_unavailable")]
    ScreenUnavailable,
    #[error("call_unsupported_server")]
    UnsupportedServer,
    #[error("call_session_closed")]
    SessionClosed,
    #[error("call_permission_denied")]
    PermissionDenied,
    #[error("call_permission_restricted")]
    PermissionRestricted,
    #[error("call_already_active")]
    AlreadyActive,
    #[error("call_invalid_room")]
    InvalidRoom,
    #[error("call_unencrypted_room")]
    UnencryptedRoom,
    #[error("call_stale_request")]
    StaleRequest,
    #[error("call_control_unavailable")]
    ControlUnavailable,
    #[error("call_reconnect_timeout")]
    ReconnectTimeout,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Direct,
    Group,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    Left,
    Declined,
    RingExpired,
    Logout,
    RoomLeft,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallPhase {
    Starting,
    Joining,
    Connected,
    /// Unix milliseconds; the transport may already be connected while awaiting pickup.
    Ringing {
        since: u64,
        expires: u64,
    },
    Reconnecting,
    Ended(EndReason),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Participant {
    pub device_id: String,
    /// Raw LiveKit voice activity; UI must debounce (~1 s onset, several seconds hold).
    pub speaking: Option<bool>,
    /// Stable MatrixRTC member identity, scoped to this call (user + device).
    pub id: String,
    /// Local receiver gain/mute; independent of the sender's microphone mute.
    pub playback: twayblade_media::Playback,
    pub connection_quality: Option<ConnectionQuality>,
    pub user_id: String,
    pub display_name: Option<String>,
    /// Unknown from MatrixRTC membership alone. Never infer device state.
    pub muted: Option<bool>,
    pub video: Option<bool>,
    /// This member published media without end-to-end encryption; it is not
    /// received. The call continues for everyone else.
    pub media_blocked: bool,
    /// Publishing a screen share.
    pub screen: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}
#[derive(Clone, Debug, Default)]
pub struct LocalCallState {
    pub muted: bool,
    pub video: bool,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub camera: Option<String>,
    pub screen: ScreenShare,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenShare {
    #[default]
    Off,
    /// Capture started; the system picker may be open. Nothing is sent yet.
    Choosing,
    /// The chosen screen or window is published.
    Live,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoSource {
    Camera,
    Screen,
}
#[derive(Clone, Debug)]
pub struct CallVideoTrack {
    pub id: String,
    pub user_id: String,
    pub device_id: String,
    pub local: bool,
    pub source: VideoSource,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Input,
    Output,
    Camera,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallEvent {
    DeviceFallback {
        kind: DeviceKind,
        from: String,
        to: Option<String>,
    },
    PermissionRevoked {
        kind: DeviceKind,
        error: CallError,
    },
}
#[derive(Clone, Debug)]
pub struct CallState {
    /// Backend generation for UI actions; None on core-only snapshots.
    pub ticket: Option<CallTicket>,
    /// Bounded recent events with per-call serials for toast deduplication.
    pub events: Vec<(u64, CallEvent)>,
    pub local: LocalCallState,
    pub devices: CallDevices,
    pub video_tracks: Vec<CallVideoTrack>,
    pub room_id: String,
    pub kind: CallKind,
    pub phase: CallPhase,
    pub participants: Vec<Participant>,
    /// Confirmed native media state; local.* is the expanded contract.
    pub muted: Option<bool>,
    pub video: Option<bool>,
    /// Confirmed local screen share; `local.screen` also reports choosing.
    pub screen_sharing: Option<bool>,
    pub error: Option<CallError>,
}
#[derive(Clone, Debug)]
pub struct ActiveCall {
    pub participants: Vec<Participant>,
    /// Earliest nonexpired membership creation time; None if unavailable.
    pub started_ms: Option<u64>,
    /// Next SDK membership expiry, used to schedule a local projection update.
    pub expires_ms: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallTicket {
    pub account: u128,
    pub request: u64,
}
impl CallTicket {
    pub fn matches(self, account: Option<u128>, request: u64) -> bool {
        account == Some(self.account) && request == self.request
    }
}
impl Account {
    /// SDK-derived nonexpired memberships, including rooms not selected in UI.
    pub async fn room_calls(&self) -> HashMap<String, ActiveCall> {
        let mut calls = HashMap::new();
        for room in self.client.joined_rooms() {
            if !room.has_active_room_call() {
                continue;
            }
            let members = native::members(&room).await.unwrap_or_default();
            if members.is_empty() {
                continue;
            }
            let expires_ms = members.iter().filter_map(|m| m.expires_ms).min();
            let started_ms = members.iter().map(|m| m.created_ms).min();
            let mut participants = Vec::new();
            for member in members {
                let display_name = room
                    .get_member_no_sync(&member.user_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|m| m.display_name().map(str::to_owned));
                participants.push(Participant {
                    user_id: member.user_id.to_string(),
                    device_id: member.device_id.clone(),
                    display_name,
                    id: member.identity(),
                    playback: Default::default(),
                    speaking: None,
                    connection_quality: None,
                    muted: None,
                    video: None,
                    media_blocked: false,
                    screen: false,
                });
            }
            calls.insert(
                room.room_id().to_string(),
                ActiveCall {
                    participants,
                    started_ms,
                    expires_ms,
                },
            );
        }
        calls
    }
    pub fn call_room_joined(&self, id: &str) -> bool {
        matrix_sdk::ruma::RoomId::parse(id)
            .ok()
            .and_then(|id| self.client.get_room(&id))
            .is_some_and(|r| r.state() == RoomState::Joined)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_call_tickets_reject_account_changes_and_newer_requests() {
        let ticket = CallTicket {
            account: 5,
            request: 8,
        };
        assert!(ticket.matches(Some(5), 8));
        for (account, request) in [(None, 8), (Some(6), 8), (Some(5), 9)] {
            assert!(!ticket.matches(account, request));
        }
    }
}
