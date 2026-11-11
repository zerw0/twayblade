//! Presence is positive server evidence, never inferred from connection state.
use crate::auth::Account;
use futures_util::{Stream, StreamExt};
use matrix_sdk::ruma::{
    OwnedUserId, api::client::presence::get_presence::v3 as api, presence::PresenceState,
};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Online,
    Unavailable,
    Offline,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Presence {
    pub state: State,
    /// Absolute epoch milliseconds, only when the server supplies last_active_ago.
    pub last_active_ms: Option<u64>,
    pub status_msg: Option<String>,
}
impl std::fmt::Debug for Presence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Presence")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Default)]
pub struct PresenceSnapshot {
    pub enabled: bool,
    pub own: Option<Presence>,
    pub users: HashMap<String, Presence>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PresenceError {
    #[error("presence_invalid_status")]
    InvalidStatus,
    #[error("presence_invalid_request")]
    InvalidRequest,
    #[error("presence_unavailable")]
    Unavailable,
}
/// Whitespace-only input clears the status, like `None`.
fn validate_status(status: Option<String>) -> Result<Option<String>, PresenceError> {
    let Some(status) = status else {
        return Ok(None);
    };
    if status.chars().any(char::is_control) {
        return Err(PresenceError::InvalidStatus);
    }
    let status = status.trim();
    if status.chars().count() > 256 {
        return Err(PresenceError::InvalidStatus);
    }
    Ok((!status.is_empty()).then(|| status.to_owned()))
}
fn project(response: api::Response) -> Option<Presence> {
    let state = match response.presence {
        PresenceState::Online => State::Online,
        PresenceState::Unavailable => State::Unavailable,
        PresenceState::Offline => State::Offline,
        _ => return None,
    };
    // Synapse deliberately returns a bare offline response when presence is off.
    // Bare offline is also ambiguous on other servers: do not synthesize a status.
    if state == State::Offline
        && response.last_active_ago.is_none()
        && response.currently_active.is_none()
        && response.status_msg.is_none()
    {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64);
    Some(Presence {
        state,
        last_active_ms: response.last_active_ago.and_then(|ago| {
            now.map(|now| now.saturating_sub(ago.as_millis().min(u64::MAX as u128) as u64))
        }),
        status_msg: response.status_msg,
    })
}
pub fn parse_user(id: &str) -> Option<OwnedUserId> {
    matrix_sdk::ruma::UserId::parse(id).ok()
}
impl Account {
    pub async fn set_presence(
        &self,
        state: State,
        status_msg: Option<String>,
    ) -> Result<(), PresenceError> {
        let status_msg = validate_status(status_msg)?;
        let state = match state {
            State::Online => PresenceState::Online,
            State::Unavailable => PresenceState::Unavailable,
            State::Offline => PresenceState::Offline,
        };
        self.client
            .set_presence(state, status_msg, true)
            .await
            .map_err(|_| PresenceError::Unavailable)
    }
    pub async fn presence(
        &self,
        visible: Vec<OwnedUserId>,
    ) -> Result<PresenceSnapshot, PresenceError> {
        if visible.len() > 64 {
            return Err(PresenceError::InvalidRequest);
        }
        let own = self
            .client
            .user_id()
            .ok_or(PresenceError::Unavailable)?
            .to_owned();
        let response = self
            .client
            .send(api::Request::new(own.clone()))
            .await
            .map_err(|_| PresenceError::Unavailable)?;
        let Some(own_presence) = project(response) else {
            return Ok(PresenceSnapshot::default());
        };
        let mut users = HashMap::new();
        let visible: std::collections::BTreeSet<_> =
            visible.into_iter().filter(|id| id != &own).collect();
        let mut work = futures_util::stream::iter(visible)
            .map(|user| async move {
                let result = self
                    .client
                    .send(api::Request::new(user.clone()))
                    .await
                    .ok()
                    .and_then(project);
                (user, result)
            })
            .buffer_unordered(4);
        while let Some((user, presence)) = work.next().await {
            if let Some(presence) = presence {
                users.insert(user.to_string(), presence);
            }
        }
        Ok(PresenceSnapshot {
            enabled: true,
            own: Some(own_presence),
            users,
        })
    }
    /// Drop the stream to cancel the account/visible-set observer. Requests are
    /// bounded to four concurrent users and polled every 30 seconds, since native
    /// sliding sync does not provide a presence extension.
    pub fn presence_updates(
        &self,
        visible: Vec<OwnedUserId>,
    ) -> impl Stream<Item = Result<PresenceSnapshot, PresenceError>> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            loop {
                yield tokio::time::timeout(Duration::from_secs(20),account.presence(visible.clone())).await.unwrap_or(Err(PresenceError::Unavailable));
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_validation_trims_counts_unicode_and_rejects_controls() {
        assert_eq!(validate_status(None).unwrap(), None);
        assert_eq!(
            validate_status(Some("  status  ".into()))
                .unwrap()
                .as_deref(),
            Some("status")
        );
        assert_eq!(validate_status(Some("   ".into())).unwrap(), None);
        assert_eq!(
            validate_status(Some("🙂".repeat(256)))
                .unwrap()
                .unwrap()
                .chars()
                .count(),
            256
        );
        for input in [
            "a".repeat(257),
            "🙂".repeat(257),
            "line\nbreak".into(),
            "tab\t".into(),
            "nul\0".into(),
            "delete\u{7f}".into(),
        ] {
            assert_eq!(
                validate_status(Some(input)),
                Err(PresenceError::InvalidStatus)
            );
        }
    }
    #[test]
    fn bare_offline_is_unknown_and_real_evidence_is_preserved() {
        assert!(project(api::Response::new(PresenceState::Offline)).is_none());
        let mut response = api::Response::new(PresenceState::Offline);
        response.last_active_ago = Some(Duration::from_millis(10));
        assert_eq!(project(response).unwrap().state, State::Offline);
        let mut response = api::Response::new(PresenceState::Unavailable);
        response.status_msg = Some("private status".into());
        let value = project(response).unwrap();
        assert_eq!(value.status_msg.as_deref(), Some("private status"));
        assert!(!format!("{value:?}").contains("private status"));
    }
}

#[cfg(test)]
mod integration;
