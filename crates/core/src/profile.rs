//! Read-only Matrix profiles. Display names are not user identities.
use std::time::Duration;

use matrix_sdk::ruma::{RoomId, UserId, api::client::profile::DisplayName};

use crate::auth::Account;

#[derive(Clone)]
pub struct UserProfile {
    pub user_id: String,
    pub display_name: String,
    pub room_name: Option<String>,
    pub room_display_name: Option<String>,
    pub server_profile_available: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("profile_invalid_user")]
    InvalidUser,
    #[error("profile_unavailable")]
    Unavailable,
}

impl Account {
    pub async fn profile(
        &self,
        user_id: String,
        room_id: Option<String>,
    ) -> Result<UserProfile, ProfileError> {
        let user = UserId::parse(&user_id).map_err(|_| ProfileError::InvalidUser)?;
        let room = room_id
            .and_then(|id| RoomId::parse(id).ok())
            .and_then(|id| self.client.get_room(&id));
        let member = if let Some(room) = &room {
            room.get_member_no_sync(&user).await.ok().flatten()
        } else {
            None
        };
        let profile = tokio::time::timeout(
            Duration::from_secs(15),
            self.client.account().fetch_user_profile_of(&user),
        )
        .await
        .ok()
        .and_then(Result::ok);
        if profile.is_none() && member.is_none() {
            return Err(ProfileError::Unavailable);
        }
        let display_name = profile
            .as_ref()
            .and_then(|profile| profile.get_static::<DisplayName>().ok().flatten())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| user_id.clone());
        Ok(UserProfile {
            user_id,
            display_name,
            room_name: room.and_then(|room| room.name()),
            room_display_name: member.and_then(|member| member.display_name().map(str::to_owned)),
            server_profile_available: profile.is_some(),
        })
    }
}
