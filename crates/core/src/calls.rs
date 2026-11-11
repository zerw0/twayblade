//! MatrixRTC contracts and room-scoped SDK widget driver. No frontend wording.
#[cfg(all(test, target_os = "macos"))]
mod integration;
mod keys;
pub mod native;
#[cfg(test)]
mod native_integration;
pub mod policy;
mod session;
use crate::auth::Account;
use matrix_sdk::{
    Room, RoomState,
    widget::{
        ClientProperties, EncryptionSystem, Intent, VirtualElementCallWidgetConfig,
        VirtualElementCallWidgetProperties, WidgetDriver, WidgetDriverHandle, WidgetSettings,
    },
};
pub use session::{NativeCall, NativeControl, CallHandle};
use std::{collections::HashMap, path::PathBuf};
use twayblade_call_host::{
    Frame, ORIGIN,
    supervisor::{Exit, Supervisor},
};
pub use twayblade_media::{Device as CallDevice, Devices as CallDevices, LatestFrame, VideoTracks};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    #[error("call_no_foci")]
    NoFoci,
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
    #[error("call_unsupported_server")]
    UnsupportedServer,
    #[error("call_widget_rejected")]
    WidgetRejected,
    #[error("call_runtime_exited")]
    RuntimeExited,
    #[error("call_runtime_hung")]
    RuntimeHung,
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
    #[error("call_unsupported_runtime")]
    UnsupportedRuntime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Direct,
    Group,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    Left,
    Logout,
    RoomLeft,
    RuntimeClosed,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallPhase {
    Starting,
    Joining,
    Connected,
    Ended(EndReason),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Participant {
    pub device_id: String,
    pub speaking: Option<bool>,
    pub connection_quality: Option<ConnectionQuality>,
    pub user_id: String,
    pub display_name: Option<String>,
    /// Unknown from MatrixRTC membership alone. Never infer device state.
    pub muted: Option<bool>,
    pub video: Option<bool>,
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
}
#[derive(Clone, Debug)]
pub struct CallVideoTrack {
    pub id: String,
    pub user_id: String,
    pub device_id: String,
    pub local: bool,
}
#[derive(Clone, Debug)]
pub struct CallState {
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
    /// Screen sharing is not implemented in this phase.
    pub screen_sharing: Option<bool>,
    pub error: Option<CallError>,
}
#[derive(Clone, Debug)]
pub struct ActiveCall {
    pub participants: Vec<Participant>,
    /// Earliest nonexpired membership creation time; None if unavailable.
    pub started_ms: Option<u64>,
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
pub struct CallWidget {
    pub widget_id: String,
    pub room: Room,
    pub url: String,
    pub jwt_service: String,
    pub handle: WidgetDriverHandle,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for CallWidget {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl CallWidget {
    pub async fn accepts_message(&self, value: &serde_json::Value) -> bool {
        if !policy::room_scoped_message(value, self.room.room_id().as_str(), &self.widget_id) {
            return false;
        }
        if value.get("response").is_none() && value["action"] == "send_to_device" {
            let Some(users) = value["data"]["messages"].as_object() else {
                return false;
            };
            for user in users.keys() {
                let Ok(user) = matrix_sdk::ruma::UserId::parse(user) else {
                    return false;
                };
                let Ok(Some(member)) = self.room.get_member_no_sync(&user).await else {
                    return false;
                };
                if member.membership()
                    != &matrix_sdk::ruma::events::room::member::MembershipState::Join
                {
                    return false;
                }
            }
        }
        true
    }
}
impl Account {
    pub async fn call_widget(
        &self,
        room_id: &str,
        join: bool,
        video: bool,
    ) -> Result<CallWidget, CallError> {
        let id = matrix_sdk::ruma::RoomId::parse(room_id).map_err(|_| CallError::InvalidRoom)?;
        let room = self
            .client
            .get_room(&id)
            .filter(|r| r.state() == RoomState::Joined)
            .ok_or(CallError::InvalidRoom)?;
        if !room
            .latest_encryption_state()
            .await
            .map_err(|_| CallError::UnsupportedServer)?
            .is_encrypted()
        {
            return Err(CallError::UnencryptedRoom);
        }
        let foci = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            self.client.well_known_rtc_transports(),
        )
        .await
        .map_err(|_| CallError::UnsupportedServer)?
        .map_err(|_| CallError::UnsupportedServer)?;
        let jwt_service = foci
            .iter()
            .filter(|f| f.transport_type() == "livekit")
            .filter_map(|f| {
                f.data()
                    .get("livekit_service_url")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .find(|s| twayblade_call_host::jwt_service(s).is_some())
            .ok_or(CallError::NoFoci)?;
        let settings = WidgetSettings::new_virtual_element_call_widget(
            VirtualElementCallWidgetProperties {
                element_call_url: format!("{ORIGIN}/index.html"),
                widget_id: uuid::Uuid::new_v4().to_string(),
                encryption: EncryptionSystem::PerParticipantKeys,
                ..Default::default()
            },
            VirtualElementCallWidgetConfig {
                intent: Some(if join {
                    Intent::JoinExisting
                } else if video {
                    Intent::StartCall
                } else {
                    Intent::StartCallDmVoice
                }),
                confine_to_room: Some(true),
                // User gesture and OS permission prompts remain in Element Call.
                skip_lobby: Some(false),
                ..Default::default()
            },
        )
        .map_err(|_| CallError::WidgetRejected)?;
        let widget_id = settings.widget_id().to_owned();
        let url = settings
            .generate_webview_url(
                &room,
                ClientProperties::new("com.twayblade.desktop", None, None),
            )
            .await
            .map_err(|_| CallError::WidgetRejected)?
            .to_string();
        let provider = policy::CallCapabilities::new(
            &self.user_id(),
            self.client
                .device_id()
                .ok_or(CallError::WidgetRejected)?
                .as_str(),
        );
        let (driver, handle) = WidgetDriver::new(settings);
        let target = room.clone();
        let task = tokio::spawn(async move {
            let _ = driver.run(target, provider).await;
        });
        Ok(CallWidget {
            widget_id,
            room,
            url,
            jwt_service,
            handle,
            task,
        })
    }
    /// SDK-derived nonexpired memberships, including rooms not selected in UI.
    pub async fn room_calls(&self) -> HashMap<String, ActiveCall> {
        let mut calls = HashMap::new();
        for room in self.client.joined_rooms() {
            if !room.has_active_room_call() {
                continue;
            }
            let mut participants = Vec::new();
            for member in native::members(&room).await.unwrap_or_default() {
                let display_name = room
                    .get_member_no_sync(&member.user_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|m| m.display_name().map(str::to_owned));
                participants.push(Participant {
                    user_id: member.user_id.to_string(),
                    device_id: member.device_id,
                    display_name,
                    speaking: None,
                    connection_quality: None,
                    muted: None,
                    video: None,
                });
            }
            calls.insert(
                room.room_id().to_string(),
                ActiveCall {
                    participants,
                    started_ms: membership_start(&room).await,
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
async fn membership_start(room: &Room) -> Option<u64> {
    use matrix_sdk::ruma::events::call::member::CallMemberEventContent;
    let mut oldest = None;
    for raw in room
        .get_state_events_static::<CallMemberEventContent>()
        .await
        .ok()?
    {
        let Ok(event) = raw.deserialize() else {
            continue;
        };
        let timestamp = event.as_sync().map(|e| e.origin_server_ts());
        let Some(content) = event.original_content() else {
            continue;
        };
        for member in content.active_memberships(timestamp) {
            if !member.is_room_call() {
                continue;
            }
            if let Some(time) = member.created_ts().or(timestamp) {
                let ms = u64::from(time.0);
                oldest = Some(oldest.map_or(ms, |old: u64| old.min(ms)));
            }
        }
    }
    oldest
}
pub fn runtime_executable() -> Result<PathBuf, CallError> {
    let exe = std::env::current_exe().map_err(|_| CallError::RuntimeExited)?;
    let parent = exe.parent().ok_or(CallError::RuntimeExited)?;
    let bundled = parent.join("../Helpers/Twayblade Call.app/Contents/MacOS/twayblade-call-host");
    if bundled.is_file() {
        return Ok(bundled);
    }
    let parent = if parent.file_name().is_some_and(|n| n == "deps") {
        parent.parent().ok_or(CallError::RuntimeExited)?
    } else {
        parent
    };
    let helper = parent.join("bundle/Twayblade Call.app/Contents/MacOS/twayblade-call-host");
    if helper.is_file() {
        return Ok(helper);
    }
    Ok(parent.join("twayblade-call-host"))
}
pub fn runtime_error(exit: Exit) -> CallError {
    if exit == Exit::Hung {
        CallError::RuntimeHung
    } else {
        CallError::RuntimeExited
    }
}

/// Spawn only after account/ticket admission in the application backend.
pub async fn spawn_runtime(widget: &CallWidget) -> Result<Supervisor, CallError> {
    let supervisor =
        Supervisor::spawn(&runtime_executable()?).map_err(|_| CallError::RuntimeExited)?;
    supervisor
        .send
        .try_send(Frame::Configure {
            url: widget.url.clone(),
            jwt_service: widget.jwt_service.clone(),
        })
        .map_err(|_| CallError::RuntimeExited)?;
    Ok(supervisor)
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
