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
    #[error("profile_invalid_image")]
    InvalidImage,
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

/// Editable account fields are sent through the SDK and never persisted by the frontend.
#[derive(Clone)]
pub enum ProfileUpdate {
    DisplayName(String),
    Avatar(std::path::PathBuf),
    RemoveAvatar,
}
impl Account {
    pub async fn update_profile(&self, update: ProfileUpdate) -> Result<(), ProfileError> {
        let account = self.client.account();
        match update {
            ProfileUpdate::DisplayName(name) => {
                if name.chars().count() > 255 {
                    return Err(ProfileError::InvalidUser);
                }
                account
                    .set_display_name((!name.is_empty()).then_some(name.as_str()))
                    .await
                    .map_err(|_| ProfileError::Unavailable)?;
            }
            ProfileUpdate::Avatar(path) => {
                // Bound encoded and decoded dimensions before uploading untrusted input.
                let bytes = tokio::task::spawn_blocking(move || {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    std::fs::File::open(path)
                        .map_err(|_| ProfileError::InvalidImage)?
                        .take(8 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| ProfileError::InvalidImage)?;
                    if bytes.len() > 8 * 1024 * 1024 {
                        return Err(ProfileError::InvalidImage);
                    }
                    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
                        .with_guessed_format()
                        .map_err(|_| ProfileError::InvalidImage)?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(4096);
                    limits.max_image_height = Some(4096);
                    limits.max_alloc = Some(64 * 1024 * 1024);
                    reader.limits(limits);
                    let decoded = reader.decode().map_err(|_| ProfileError::InvalidImage)?;
                    let mut output = std::io::Cursor::new(Vec::new());
                    decoded
                        .write_to(&mut output, image::ImageFormat::Png)
                        .map_err(|_| ProfileError::InvalidImage)?;
                    Ok::<_, ProfileError>(output.into_inner())
                })
                .await
                .map_err(|_| ProfileError::InvalidImage)??;
                account
                    .upload_avatar(&mime::IMAGE_PNG, bytes)
                    .await
                    .map_err(|_| ProfileError::Unavailable)?;
            }
            ProfileUpdate::RemoveAvatar => account
                .set_avatar_url(None)
                .await
                .map_err(|_| ProfileError::Unavailable)?,
        }
        Ok(())
    }
}
