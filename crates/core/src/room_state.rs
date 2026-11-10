//! Room member data and typing intents. Protocol state remains in the SDK.
use crate::auth::Account;
use matrix_sdk::{
    Room, RoomMemberships,
    ruma::{RoomId, events::room::power_levels::UserPowerLevel, int},
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum MemberRole {
    Admin,
    Moderator,
    #[default]
    Member,
}

#[derive(Clone, Default)]
pub struct Member {
    pub user_id: String,
    pub display_name: String,
    pub role: MemberRole,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum MembersState {
    #[default]
    Loading,
    Ready,
    Failed,
}

#[derive(Debug, thiserror::Error)]
pub enum RoomStateError {
    #[error("room_unavailable")]
    Unavailable,
    #[error("members_failed")]
    Members,
    #[error("typing_failed")]
    Typing,
}

fn role(power: UserPowerLevel) -> MemberRole {
    match power {
        UserPowerLevel::Infinite => MemberRole::Admin,
        UserPowerLevel::Int(power) if power >= int!(100) => MemberRole::Admin,
        UserPowerLevel::Int(power) if power >= int!(50) => MemberRole::Moderator,
        _ => MemberRole::Member,
    }
}

pub(crate) async fn room_members(room: Room) -> Result<Vec<Member>, RoomStateError> {
    let mut members: Vec<_> = room
        .members(RoomMemberships::JOIN)
        .await
        .map_err(|_| RoomStateError::Members)?
        .into_iter()
        .map(|member| Member {
            user_id: member.user_id().to_string(),
            display_name: member
                .display_name()
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| member.user_id().to_string()),
            role: role(member.power_level()),
        })
        .collect();
    members
        .sort_by_cached_key(|member| (member.display_name.to_lowercase(), member.user_id.clone()));
    Ok(members)
}

impl Account {
    pub async fn typing_notice(&self, room_id: &str, typing: bool) -> Result<(), RoomStateError> {
        let room = RoomId::parse(room_id)
            .ok()
            .and_then(|id| self.client.get_room(&id))
            .ok_or(RoomStateError::Unavailable)?;
        // The SDK supplies throttling and the server-side timeout.
        room.typing_notice(typing)
            .await
            .map_err(|_| RoomStateError::Typing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn power_roles_use_raw_sdk_levels_including_version_12_creators() {
        for (power, expected) in [
            (int!(-1), MemberRole::Member),
            (int!(0), MemberRole::Member),
            (int!(49), MemberRole::Member),
            (int!(50), MemberRole::Moderator),
            (int!(99), MemberRole::Moderator),
            (int!(100), MemberRole::Admin),
            (int!(200), MemberRole::Admin),
        ] {
            assert!(role(UserPowerLevel::Int(power)) == expected);
        }
        assert!(role(UserPowerLevel::Infinite) == MemberRole::Admin);
    }
}
