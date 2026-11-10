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
    pub avatar: Option<String>,
    pub presence: Option<crate::presence::Presence>,
    /// What you may do to this member here, from the room's power levels.
    pub can_kick: bool,
    pub can_ban: bool,
    /// You may change their power level (used for Mute/Unmute).
    pub can_mute: bool,
    /// Their power level is below the level needed to send messages.
    pub muted: bool,
}

/// Actions on another member. Moderation ones are checked again by the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberAction {
    /// Account-wide: hide everything they send, in every room (Matrix "ignore").
    Ignore,
    Unignore,
    Kick,
    Ban,
    /// Drop their power level below the room's send level, or restore it.
    Mute(bool),
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
    #[error("moderation_failed")]
    Moderation,
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
    let own = room.own_user_id().to_owned();
    let levels = room.power_levels().await.ok();
    let mut members: Vec<_> = room
        .members(RoomMemberships::JOIN)
        .await
        .map_err(|_| RoomStateError::Members)?
        .into_iter()
        .map(|member| Member {
            presence: None,
            user_id: member.user_id().to_string(),
            display_name: member
                .display_name()
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| member.user_id().to_string()),
            role: role(member.power_level()),
            avatar: member.avatar_url().map(|url| url.to_string()),
            can_kick: levels.as_ref().is_some_and(|levels| {
                member.user_id() != own && levels.user_can_kick_user(&own, member.user_id())
            }),
            can_ban: levels.as_ref().is_some_and(|levels| {
                member.user_id() != own && levels.user_can_ban_user(&own, member.user_id())
            }),
            can_mute: levels.as_ref().is_some_and(|levels| {
                member.user_id() != own
                    && levels.user_can_change_user_power_level(&own, member.user_id())
            }),
            muted: levels.as_ref().is_some_and(|levels| {
                matches!(levels.for_user(member.user_id()), UserPowerLevel::Int(level) if level < levels.events_default)
            }),
        })
        .collect();
    members
        .sort_by_cached_key(|member| (member.display_name.to_lowercase(), member.user_id.clone()));
    Ok(members)
}

impl Account {
    pub async fn member_action(
        &self,
        room_id: &str,
        user_id: &str,
        action: MemberAction,
    ) -> Result<(), RoomStateError> {
        let user =
            matrix_sdk::ruma::UserId::parse(user_id).map_err(|_| RoomStateError::Unavailable)?;
        if matches!(action, MemberAction::Ignore | MemberAction::Unignore) {
            let account = self.client.account();
            return if action == MemberAction::Ignore {
                account.ignore_user(&user).await
            } else {
                account.unignore_user(&user).await
            }
            .map_err(|_| RoomStateError::Moderation);
        }
        let room = RoomId::parse(room_id)
            .ok()
            .and_then(|id| self.client.get_room(&id))
            .ok_or(RoomStateError::Unavailable)?;
        match action {
            MemberAction::Kick => room.kick_user(&user, None).await.map(|_| ()),
            MemberAction::Ban => room.ban_user(&user, None).await.map(|_| ()),
            MemberAction::Mute(mute) => {
                let levels = room
                    .power_levels()
                    .await
                    .map_err(|_| RoomStateError::Moderation)?;
                let level = if mute {
                    levels.events_default - int!(1)
                } else {
                    levels.users_default
                };
                room.update_power_levels(vec![(&user, level)])
                    .await
                    .map(|_| ())
            }
            MemberAction::Ignore | MemberAction::Unignore => unreachable!("handled above"),
        }
        .map_err(|_| RoomStateError::Moderation)
    }

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
