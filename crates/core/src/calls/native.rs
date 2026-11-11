//! Native MatrixRTC signalling, targeting Element Call 0.28.2 Compatibility mode.
use super::CallError;
use crate::auth::Account;
use matrix_sdk::{
    Room, RoomState,
    ruma::{
        MilliSecondsSinceUnixEpoch, OwnedUserId,
        api::client::{
            account::request_openid_token,
            delayed_events::{self, DelayParameters, update_delayed_event::UpdateAction},
        },
        events::call::member::{CallMemberEventContent, CallMemberStateKey, Focus},
        serde::Raw,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

pub const MEMBER_EVENT: &str = "org.matrix.msc3401.call.member";
pub const KEY_EVENT: &str = "io.element.call.encryption_keys";
pub const EXPIRY: Duration = Duration::from_secs(120);
pub const KEEPALIVE: Duration = Duration::from_secs(5);
const LEAVE_DELAY: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Member {
    pub user_id: OwnedUserId,
    pub device_id: String,
    /// Session membership ID (not the state key).
    pub member_id: String,
    pub created_ms: u64,
    pub service_url: Option<String>,
    pub alias: Option<String>,
}
impl Member {
    pub fn identity(&self) -> String {
        format!("{}:{}", self.user_id, self.device_id)
    }
}

pub fn state_key(user: OwnedUserId, device: &str, room_version: &str) -> CallMemberStateKey {
    CallMemberStateKey::new(
        user,
        Some(format!("{device}_m.call")),
        !["org.matrix.msc3757", "org.matrix.msc3779"]
            .iter()
            .any(|prefix| {
                room_version == *prefix
                    || room_version
                        .strip_prefix(prefix)
                        .is_some_and(|suffix| suffix.starts_with('.'))
            }),
    )
}

/// SDK parsing and expiry rules; reject mismatched state-key owners and other calls.
pub async fn members(room: &Room) -> Result<Vec<Member>, CallError> {
    let mut result = BTreeMap::new();
    for raw in room
        .get_state_events_static::<CallMemberEventContent>()
        .await
        .map_err(|_| CallError::TransportFailed)?
    {
        let Ok(event) = raw.deserialize() else {
            continue;
        };
        let Some(sync) = event.as_sync() else {
            continue;
        };
        let Some(content) = event.original_content() else {
            continue;
        };
        if sync.state_key().user_id() != sync.sender() {
            continue;
        }
        if !room
            .get_member_no_sync(sync.sender())
            .await
            .ok()
            .flatten()
            .is_some_and(|m| {
                m.membership() == &matrix_sdk::ruma::events::room::member::MembershipState::Join
            })
        {
            continue;
        }
        for membership in content.active_memberships(Some(sync.origin_server_ts())) {
            // SDK is_room_call checks application/scope; explicitly bind empty call_id too.
            if !membership.is_room_call() {
                continue;
            }
            let matrix_sdk::ruma::events::call::member::Application::Call(call) =
                membership.application()
            else {
                continue;
            };
            if !call.call_id.is_empty() {
                continue;
            }
            let device_id = membership.device_id().to_string();
            let mut member = Member {
                user_id: sync.sender().to_owned(),
                device_id: device_id.clone(),
                member_id: format!("{}:{device_id}", sync.sender()),
                created_ms: u64::from(membership.created_ts().unwrap_or(sync.origin_server_ts()).0),
                service_url: None,
                alias: None,
            };
            if let Some(Focus::Livekit(focus)) = membership.foci_preferred().first()
                && valid_service(&focus.service_url)
            {
                member.service_url = Some(focus.service_url.clone());
                member.alias = Some(focus.alias.clone());
            }
            result.insert((member.user_id.clone(), device_id), member);
        }
    }
    let mut result: Vec<_> = result.into_values().collect();
    result.sort_by(|a, b| {
        (a.created_ms, &a.user_id, &a.device_id).cmp(&(b.created_ms, &b.user_id, &b.device_id))
    });
    Ok(result)
}
pub fn valid_service(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}
/// An existing call's oldest membership controls the focus. Do not silently fork it.
pub fn select_focus(
    members: &[Member],
    discovered: &[String],
    room_id: &str,
) -> Result<(String, String), CallError> {
    if let Some(first) = members.iter().min_by(|a, b| {
        (a.created_ms, &a.user_id, &a.device_id).cmp(&(b.created_ms, &b.user_id, &b.device_id))
    }) {
        return first
            .service_url
            .clone()
            .zip(first.alias.clone())
            .filter(|(s, _)| valid_service(s))
            .ok_or(CallError::NoFoci);
    }
    discovered
        .iter()
        .find(|s| valid_service(s))
        .map(|s| (s.clone(), room_id.to_owned()))
        .ok_or(CallError::NoFoci)
}

/// Sensitive credentials: deliberately no Debug/Serialize implementation.
pub struct SfuCredentials {
    pub url: String,
    pub jwt: zeroize::Zeroizing<String>,
    pub http: matrix_sdk::reqwest::Client,
}

pub struct Signalling {
    pub room: Room,
    pub own: Member,
    state_key: CallMemberStateKey,
    content: Value,
    delayed: Option<String>,
    closed: bool,
    pub(crate) membership_updates: tokio::sync::watch::Receiver<u64>,
    _membership_handler: matrix_sdk::event_handler::EventHandlerDropGuard,
    pub(crate) lease: Option<tokio::sync::OwnedSemaphorePermit>,
}
impl Account {
    pub async fn prepare_native_call(
        &self,
        room_id: &str,
        media_e2ee: bool,
    ) -> Result<(Signalling, SfuCredentials), CallError> {
        if !media_e2ee {
            return Err(CallError::MediaE2eeRequired);
        }
        let id = matrix_sdk::ruma::RoomId::parse(room_id).map_err(|_| CallError::InvalidRoom)?;
        let room = self
            .client
            .get_room(&id)
            .filter(|r| r.state() == RoomState::Joined)
            .ok_or(CallError::InvalidRoom)?;
        if !room
            .latest_encryption_state()
            .await
            .map_err(|_| CallError::TransportFailed)?
            .is_encrypted()
        {
            return Err(CallError::UnencryptedRoom);
        }
        let user = self
            .client
            .user_id()
            .ok_or(CallError::StaleRequest)?
            .to_owned();
        let device = self
            .client
            .device_id()
            .ok_or(CallError::StaleRequest)?
            .to_string();
        let current = members(&room).await?;
        let foci = self
            .client
            .well_known_rtc_transports()
            .await
            .map_err(|_| CallError::NoFoci)?;
        let discovered: Vec<_> = foci
            .iter()
            .filter(|f| f.transport_type() == "livekit")
            .filter_map(|f| {
                f.data()
                    .get("livekit_service_url")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        #[cfg(test)]
        eprintln!("Native fixture: discovered_foci={}", discovered.len());
        let (service, alias) = select_focus(&current, &discovered, room_id)?;
        let token = self
            .client
            .send(request_openid_token::v3::Request::new(user.clone()))
            .with_request_config(
                matrix_sdk::config::RequestConfig::new()
                    .timeout(Duration::from_secs(10))
                    .retry_limit(0),
            )
            .await
            .map_err(|_| CallError::TransportFailed)?;
        #[cfg(test)]
        eprintln!("Native fixture: OpenID obtained");
        // Build a separate HTTP client with the account's explicit fixture CA,
        // no Matrix bearer token on this request and no redirects to other origins.
        let body = json!({"room":alias,"openid_token":{"access_token":token.access_token,"token_type":token.token_type,"matrix_server_name":token.matrix_server_name,"expires_in":token.expires_in.as_secs()},"device_id":device});
        let mut http = matrix_sdk::reqwest::Client::builder()
            .http1_only()
            .redirect(matrix_sdk::reqwest::redirect::Policy::none());
        if let Some(cert) = &self.root_certificate {
            http = http.add_root_certificate(cert.clone());
        }
        let http = http.build().map_err(|_| CallError::TransportFailed)?;
        let response = http
            .post(format!("{}/sfu/get", service.trim_end_matches('/')))
            .timeout(Duration::from_secs(15))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|_| CallError::TransportFailed)?;
        #[cfg(test)]
        eprintln!(
            "Native fixture: JWT HTTP status={}",
            response.status().as_u16()
        );
        if response.status().is_redirection() || !response.status().is_success() {
            return Err(CallError::TransportFailed);
        }
        let value: Value = serde_json::from_slice(
            &response
                .bytes()
                .await
                .map_err(|_| CallError::TransportFailed)?,
        )
        .map_err(|_| CallError::TransportFailed)?;
        let url = value["url"]
            .as_str()
            .ok_or(CallError::TransportFailed)?
            .to_owned();
        let parsed = url::Url::parse(&url).map_err(|_| CallError::TransportFailed)?;
        if parsed.scheme() != "wss"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(CallError::TransportFailed);
        }
        let jwt = zeroize::Zeroizing::new(
            value["jwt"]
                .as_str()
                .ok_or(CallError::TransportFailed)?
                .to_owned(),
        );
        let state_key = state_key(
            user.clone(),
            &device,
            room.version().as_ref().map(|v| v.as_str()).unwrap_or("10"),
        );
        let now = u64::from(MilliSecondsSinceUnixEpoch::now().0);
        let content = membership_content(&device, &service, &alias, now);
        let (changed, membership_updates) = tokio::sync::watch::channel(0u64);
        let handler = room.client().add_room_event_handler(
            room.room_id(),
            move |_event: matrix_sdk::ruma::events::call::member::SyncCallMemberEvent| {
                let changed = changed.clone();
                async move {
                    changed.send_modify(|n| *n = n.wrapping_add(1));
                }
            },
        );
        let membership_handler = room.client().event_handler_drop_guard(handler);
        Ok((
            Signalling {
                room,
                own: Member {
                    user_id: user.clone(),
                    device_id: device.clone(),
                    member_id: format!("{user}:{device}"),
                    created_ms: now,
                    service_url: Some(service),
                    alias: Some(alias),
                },
                state_key,
                content,
                delayed: None,
                closed: false,
                lease: None,
                membership_updates,
                _membership_handler: membership_handler,
            },
            SfuCredentials { url, jwt, http },
        ))
    }
}
fn membership_content(device: &str, service: &str, alias: &str, now: u64) -> Value {
    json!({"application":"m.call","call_id":"","scope":"m.room","device_id":device,"expires":EXPIRY.as_millis() as u64,"created_ts":now,
        "focus_active":{"type":"livekit","focus_selection":"oldest_membership"},"foci_preferred":[{"type":"livekit","livekit_service_url":service,"livekit_alias":alias}]})
}
impl Signalling {
    async fn write(&self, content: Value) -> Result<(), CallError> {
        if self.room.state() != RoomState::Joined {
            return Err(CallError::InvalidRoom);
        }
        tokio::time::timeout(
            Duration::from_secs(5),
            self.room
                .send_state_event_raw(MEMBER_EVENT, self.state_key.as_ref(), content),
        )
        .await
        .map_err(|_| CallError::TransportFailed)?
        .map_err(|error| {
            if matches!(
                error.client_api_error_kind(),
                Some(matrix_sdk::ruma::api::error::ErrorKind::Forbidden)
            ) {
                CallError::MembershipForbidden
            } else {
                CallError::TransportFailed
            }
        })?;
        Ok(())
    }

    pub async fn join(&mut self) -> Result<(), CallError> {
        self.write(self.content.clone()).await?;
        self.schedule_leave().await;
        Ok(())
    }
    async fn schedule_leave(&mut self) {
        self.delayed = self
            .room
            .client()
            .send(
                delayed_events::delayed_state_event::unstable::Request::new_raw(
                    self.room.room_id().to_owned(),
                    self.state_key.as_ref().to_owned(),
                    MEMBER_EVENT.into(),
                    DelayParameters::Timeout {
                        timeout: LEAVE_DELAY,
                    },
                    Raw::new(&json!({}))
                        .expect("empty object serializes")
                        .cast_unchecked(),
                ),
            )
            .with_request_config(
                matrix_sdk::config::RequestConfig::new()
                    .timeout(Duration::from_secs(5))
                    .retry_limit(0),
            )
            .await
            .ok()
            .map(|r| r.delay_id);
    }
    async fn restart_leave(&self, id: &str) -> Result<bool, CallError> {
        match self
            .room
            .client()
            .send(
                delayed_events::update_delayed_event::unstable_v1::Request::new(
                    id.to_owned(),
                    UpdateAction::Restart,
                ),
            )
            .with_request_config(
                matrix_sdk::config::RequestConfig::new()
                    .timeout(Duration::from_secs(5))
                    .retry_limit(0),
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(error)
                if matches!(
                    error.client_api_error_kind(),
                    Some(matrix_sdk::ruma::api::error::ErrorKind::NotFound)
                ) =>
            {
                Ok(false)
            }
            Err(_) => Err(CallError::TransportFailed),
        }
    }
    pub async fn maintain(&mut self) -> Result<(), CallError> {
        if self.closed {
            return Err(CallError::StaleRequest);
        }
        if let Some(id) = &self.delayed
            && !self.restart_leave(id).await?
        {
            // A lost/fired delayed event may already have removed membership.
            // End rather than silently resurrecting an unprotected session.
            return Err(CallError::TransportFailed);
        }
        let now = u64::from(MilliSecondsSinceUnixEpoch::now().0);
        let expires = self.content["expires"]
            .as_u64()
            .ok_or(CallError::TransportFailed)?;
        if now.saturating_sub(self.own.created_ms) + 60_000 >= expires {
            self.content["expires"] =
                json!(now.saturating_sub(self.own.created_ms) + EXPIRY.as_millis() as u64);
            self.write(self.content.clone()).await?;
            // Some servers retain the delayed leave across a state write,
            // others cancel it. Keep restarting the existing ID when it still
            // exists: replacing it unconditionally leaves an orphan that later
            // removes the refreshed membership.
            if let Some(id) = &self.delayed
                && !self.restart_leave(id).await?
            {
                self.delayed = None;
                self.schedule_leave().await;
            }
        }
        Ok(())
    }
    pub async fn leave(&mut self) -> Result<(), CallError> {
        self.write(json!({})).await?;
        self.closed = true;
        if let Some(id) = self.delayed.take() {
            let _ = self
                .room
                .client()
                .send(
                    delayed_events::update_delayed_event::unstable_v1::Request::new(
                        id,
                        UpdateAction::Cancel,
                    ),
                )
                .with_request_config(
                    matrix_sdk::config::RequestConfig::new()
                        .timeout(Duration::from_secs(5))
                        .retry_limit(0),
                )
                .await;
        }
        Ok(())
    }
}
impl Drop for Signalling {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        // Task cancellation/account replacement must also leave. A bounded
        // best-effort cleanup complements the server delayed event/expiry.
        let lease = self.lease.take();
        let room = self.room.clone();
        let key = self.state_key.as_ref().to_owned();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(
                    Duration::from_secs(5),
                    room.send_state_event_raw(MEMBER_EVENT, &key, json!({})),
                )
                .await;
                drop(lease);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_keys_bind_user_device_and_call() {
        let user = matrix_sdk::ruma::user_id!("@a:example.org").to_owned();
        assert_eq!(
            state_key(user.clone(), "D", "10").as_ref(),
            "_@a:example.org_D_m.call"
        );
        assert_eq!(
            state_key(user.clone(), "D", "org.matrix.msc37570").as_ref(),
            "_@a:example.org_D_m.call"
        );
        assert_eq!(
            state_key(user, "D", "org.matrix.msc3757.10").as_ref(),
            "@a:example.org_D_m.call"
        );
    }
    #[test]
    fn focus_must_be_https_without_credentials_or_tokens() {
        for url in [
            "http://sfu.test",
            "https://u:p@sfu.test",
            "https://sfu.test?token=x",
            "https://sfu.test/#x",
        ] {
            assert!(!valid_service(url));
        }
        assert!(valid_service("https://sfu.test/jwt"));
        assert_eq!(
            select_focus(
                &[],
                &["http://bad".into(), "https://good".into()],
                "!r:e.org"
            )
            .unwrap(),
            ("https://good".into(), "!r:e.org".into())
        );
    }
    #[test]
    fn existing_focus_comes_from_oldest_membership_or_fails_without_forking() {
        let older = Member {
            user_id: matrix_sdk::ruma::user_id!("@a:example.org").to_owned(),
            device_id: "A".into(),
            member_id: "a".into(),
            created_ms: 1,
            service_url: Some("https://oldest.test".into()),
            alias: Some("shared".into()),
        };
        let mut newer = older.clone();
        newer.created_ms = 2;
        newer.service_url = Some("https://newer.test".into());
        let fallback = vec!["https://discovered.test".into()];
        assert_eq!(
            select_focus(&[newer.clone(), older.clone()], &fallback, "!r:example.org").unwrap(),
            ("https://oldest.test".into(), "shared".into())
        );
        let mut unusable = older;
        unusable.service_url = None;
        assert_eq!(
            select_focus(&[newer, unusable], &fallback, "!r:example.org"),
            Err(CallError::NoFoci)
        );
    }
    #[test]
    fn membership_has_bounded_sdk_expiry_and_empty_room_call_id() {
        let value = membership_content("D", "https://sfu.test", "!r:e.org", 0);
        let content: CallMemberEventContent = serde_json::from_value(value).unwrap();
        assert_eq!(content.active_memberships(None).len(), 0);
        let now = u64::from(MilliSecondsSinceUnixEpoch::now().0);
        let content: CallMemberEventContent =
            serde_json::from_value(membership_content("D", "https://sfu.test", "!r:e.org", now))
                .unwrap();
        assert_eq!(content.active_memberships(None).len(), 1);
        assert_eq!(
            u64::from(
                content.active_memberships(None)[0]
                    .expires_ts(None)
                    .unwrap()
                    .0
            ),
            now + 120_000
        );
    }
}
