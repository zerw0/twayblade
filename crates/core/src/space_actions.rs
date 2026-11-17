//! Space hierarchy and membership services. Presentation wording belongs in GPUI.
use crate::auth::Account;
use matrix_sdk::{
    RoomState,
    ruma::{
        RoomId, RoomOrAliasId, UserId,
        api::client::room::create_room,
        events::{
            InitialStateEvent, StateEventType,
            room::{
                encryption::RoomEncryptionEventContent,
                join_rules::{AllowRule, RoomJoinRulesEventContent},
            },
        },
        room::JoinRuleSummary,
    },
};
use matrix_sdk_ui::spaces::{
    SpaceService,
    leave::LeaveSpaceHandle,
    room_list::{SpaceRoomList, SpaceRoomListPaginationState},
};
use std::{collections::HashSet, sync::Arc};

#[derive(Debug, thiserror::Error)]
pub enum SpaceError {
    #[error("invalid_identifier")]
    InvalidId,
    #[error("channel_space_required")]
    SpaceRequired,
    #[error("not_a_space")]
    NotSpace,
    #[error("space_unavailable")]
    Unavailable,
    #[error("permission_denied")]
    Permission,
    #[error("hierarchy_failed")]
    Hierarchy,
    #[error("join_failed")]
    Join,
    #[error("leave_failed")]
    Leave,
    #[error("create_failed")]
    Create,
    #[error("invite_denied")]
    InviteDenied,
    #[error("invite_failed")]
    InviteFailed,
    #[error("parent_link_failed")]
    ParentLink { room: String },
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum JoinAccess {
    Public,
    Restricted,
    Invite,
    #[default]
    Unknown,
}
#[derive(Clone, Default)]
pub struct DirectoryRoom {
    pub id: String,
    pub name: String,
    pub topic: Option<String>,
    pub space: bool,
    pub joined: bool,
    pub invited: bool,
    pub suggested: bool,
    pub access: JoinAccess,
    pub via: Vec<String>,
    /// The room's picture (mxc URI).
    pub avatar: Option<String>,
    /// Joined members, as the server reports them.
    pub members: u64,
    /// For a space, how many rooms and spaces it lists.
    pub children: u64,
}
#[derive(Clone, Default)]
pub struct DirectorySnapshot {
    pub space: String,
    pub name: String,
    pub topic: Option<String>,
    pub avatar: Option<String>,
    pub members: u64,
    pub access: JoinAccess,
    pub rooms: Vec<DirectoryRoom>,
    pub end_reached: bool,
    pub inaccessible: u64,
}
#[derive(Clone)]
pub struct Directory {
    list: Arc<SpaceRoomList>,
    space: String,
}
impl Directory {
    pub async fn next_page(&self) -> Result<DirectorySnapshot, SpaceError> {
        self.list
            .paginate()
            .await
            .map_err(|_| SpaceError::Hierarchy)?;
        Ok(self.snapshot().await)
    }
    pub async fn snapshot(&self) -> DirectorySnapshot {
        let space = self.list.space();
        let end_reached = matches!(
            self.list.pagination_state(),
            SpaceRoomListPaginationState::Idle { end_reached: true }
        );
        let rooms: Vec<_> = self
            .list
            .rooms()
            .await
            .into_iter()
            .map(|r| DirectoryRoom {
                id: r.room_id.to_string(),
                name: r.display_name,
                topic: r.topic,
                space: r.room_type == Some(matrix_sdk::ruma::room::RoomType::Space),
                joined: r.state == Some(RoomState::Joined),
                invited: r.state == Some(RoomState::Invited),
                suggested: r.suggested,
                access: join_access(r.join_rule.as_ref()),
                via: r.via.iter().map(ToString::to_string).collect(),
                avatar: r.avatar_url.as_ref().map(ToString::to_string),
                members: r.num_joined_members,
                children: r.children_count,
            })
            .collect();
        DirectorySnapshot {
            space: self.space.clone(),
            name: space
                .as_ref()
                .map(|s| s.display_name.clone())
                .unwrap_or_else(|| self.space.clone()),
            topic: space.as_ref().and_then(|s| s.topic.clone()),
            avatar: space
                .as_ref()
                .and_then(|s| s.avatar_url.as_ref().map(ToString::to_string)),
            members: space.as_ref().map_or(0, |s| s.num_joined_members),
            access: join_access(space.as_ref().and_then(|s| s.join_rule.as_ref())),
            inaccessible: if end_reached {
                space
                    .as_ref()
                    .map(|s| s.children_count.saturating_sub(rooms.len() as u64))
                    .unwrap_or(0)
            } else {
                0
            },
            rooms,
            end_reached,
        }
    }
}
fn join_access(rule: Option<&JoinRuleSummary>) -> JoinAccess {
    match rule {
        Some(JoinRuleSummary::Public) => JoinAccess::Public,
        Some(JoinRuleSummary::Restricted(_) | JoinRuleSummary::KnockRestricted(_)) => {
            JoinAccess::Restricted
        }
        Some(JoinRuleSummary::Invite | JoinRuleSummary::Knock) => JoinAccess::Invite,
        _ => JoinAccess::Unknown,
    }
}
#[derive(Clone, Default)]
pub struct LeaveCandidate {
    pub id: String,
    pub name: String,
    pub space: bool,
    pub last_owner: bool,
    pub shared: bool,
}
pub struct LeavePlan {
    space: String,
    handle: LeaveSpaceHandle,
    pub rooms: Vec<LeaveCandidate>,
}
impl LeavePlan {
    pub async fn leave(&self, selected: HashSet<String>) -> Result<(), SpaceError> {
        if !selected.contains(&self.space)
            || selected
                .iter()
                .any(|id| !self.rooms.iter().any(|r| &r.id == id))
        {
            return Err(SpaceError::InvalidId);
        }
        self.handle
            .leave(|r| selected.contains(r.space_room.room_id.as_str()))
            .await
            .map_err(|_| SpaceError::Leave)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CreateKind {
    Text,
    Voice,
    Space,
}

#[derive(Clone)]
pub struct CreateRoom {
    pub name: String,
    pub topic: String,
    pub encrypted: bool,
    pub parent: Option<String>,
}
impl CreateRoom {
    fn validate(&self) -> Result<(), SpaceError> {
        let parent = self.parent.as_deref().ok_or(SpaceError::SpaceRequired)?;
        if self.name.trim().is_empty() || RoomId::parse(parent).is_err() {
            return Err(SpaceError::InvalidId);
        }
        Ok(())
    }
}
impl Default for CreateRoom {
    fn default() -> Self {
        Self {
            name: String::new(),
            topic: String::new(),
            encrypted: true,
            parent: None,
        }
    }
}

/// A new space: its name, optional topic, and whether anyone may join.
#[derive(Clone, Default)]
pub struct CreateSpace {
    pub name: String,
    pub topic: String,
    pub public: bool,
}
impl CreateSpace {
    fn validate(&self) -> Result<(), SpaceError> {
        if self.name.trim().is_empty() || self.name.chars().count() > 255 {
            return Err(SpaceError::InvalidId);
        }
        Ok(())
    }
}

#[cfg(test)]
mod create_space_tests {
    use super::*;
    #[test]
    fn spaces_need_a_name() {
        let mut draft = CreateSpace::default();
        assert!(matches!(draft.validate(), Err(SpaceError::InvalidId)));
        draft.name = "  ".into();
        assert!(matches!(draft.validate(), Err(SpaceError::InvalidId)));
        draft.name = "Design team".into();
        assert!(draft.validate().is_ok());
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;
    #[test]
    fn channels_require_a_valid_parent_before_any_network_work() {
        let mut draft = CreateRoom {
            name: "General".into(),
            ..Default::default()
        };
        assert!(matches!(draft.validate(), Err(SpaceError::SpaceRequired)));
        draft.parent = Some("invalid".into());
        assert!(matches!(draft.validate(), Err(SpaceError::InvalidId)));
        draft.parent = Some("!space:example.org".into());
        assert!(draft.validate().is_ok());
        draft.name = "  ".into();
        assert!(matches!(draft.validate(), Err(SpaceError::InvalidId)));
    }
}
#[derive(Clone, Default)]
pub struct Invite {
    pub id: String,
    pub name: String,
    pub space: bool,
    pub direct: bool,
}
impl Account {
    pub async fn directory(&self, id: &str) -> Result<Directory, SpaceError> {
        let id = RoomId::parse(id).map_err(|_| SpaceError::InvalidId)?;
        let _state = if self
            .client
            .get_room(&id)
            .is_some_and(|r| r.state() == RoomState::Joined)
        {
            Some(
                self.sync_rooms(vec![id.clone()])
                    .await
                    .map_err(|_| SpaceError::Unavailable)?,
            )
        } else {
            None
        };
        let service = SpaceService::new(self.client.clone()).await;
        let list = service.space_room_list(id.clone()).await;
        Ok(Directory {
            list: Arc::new(list),
            space: id.to_string(),
        })
    }
    pub async fn join_room_or_space(
        &self,
        id: &str,
        via: Vec<String>,
    ) -> Result<String, SpaceError> {
        let id = RoomOrAliasId::parse(id).map_err(|_| SpaceError::InvalidId)?;
        let via = via
            .into_iter()
            .map(matrix_sdk::ruma::ServerName::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| SpaceError::InvalidId)?;
        self.client
            .join_room_by_id_or_alias(&id, &via)
            .await
            .map(|r| r.room_id().to_string())
            .map_err(|_| SpaceError::Join)
    }
    pub async fn join_space(&self, id: &str) -> Result<String, SpaceError> {
        let parsed = RoomOrAliasId::parse(id).map_err(|_| SpaceError::InvalidId)?;
        let preview = self
            .client
            .get_room_preview(&parsed, Vec::new())
            .await
            .map_err(|_| SpaceError::Unavailable)?;
        if preview.room_type != Some(matrix_sdk::ruma::room::RoomType::Space) {
            return Err(SpaceError::NotSpace);
        }
        self.join_room_or_space(id, Vec::new()).await
    }
    pub async fn invites(&self) -> Vec<Invite> {
        let mut invites = Vec::new();
        for room in self.client.invited_rooms() {
            invites.push(Invite {
                id: room.room_id().to_string(),
                name: room
                    .cached_display_name()
                    .map(|n| n.to_string())
                    .or_else(|| room.name())
                    .unwrap_or_else(|| room.room_id().to_string()),
                space: room.is_space(),
                direct: room.is_direct().await.unwrap_or(false),
            });
        }
        invites
    }
    pub async fn respond_to_invite(&self, id: &str, accept: bool) -> Result<(), SpaceError> {
        let id = RoomId::parse(id).map_err(|_| SpaceError::InvalidId)?;
        let room = self
            .client
            .get_room(&id)
            .filter(|r| r.state() == RoomState::Invited)
            .ok_or(SpaceError::Unavailable)?;
        if accept {
            room.join().await.map_err(|_| SpaceError::Join)
        } else {
            room.leave().await.map_err(|_| SpaceError::Leave)
        }
    }
    pub async fn plan_leave_space(&self, id: &str) -> Result<LeavePlan, SpaceError> {
        let id = RoomId::parse(id).map_err(|_| SpaceError::InvalidId)?;
        let service = SpaceService::new(self.client.clone()).await;
        let handle = service
            .leave_space(&id)
            .await
            .map_err(|_| SpaceError::Unavailable)?;
        let mut rooms = Vec::new();
        for r in handle.rooms() {
            rooms.push(LeaveCandidate {
                id: r.space_room.room_id.to_string(),
                name: r.space_room.display_name.clone(),
                space: r.space_room.room_type == Some(matrix_sdk::ruma::room::RoomType::Space),
                last_owner: r.is_last_owner,
                shared: service
                    .joined_parent_ids_of_child(&r.space_room.room_id)
                    .await
                    .len()
                    > 1,
            });
        }
        Ok(LeavePlan {
            space: id.to_string(),
            handle,
            rooms,
        })
    }
    pub async fn create_room(&self, draft: CreateRoom) -> Result<String, SpaceError> {
        self.create_channel(draft, CreateKind::Text).await
    }

    pub async fn create_voice_channel(&self, draft: CreateRoom) -> Result<String, SpaceError> {
        self.create_channel(draft, CreateKind::Voice).await
    }

    /// Creates a joined subspace with the same parent permission and linking checks as channels.
    pub async fn create_subspace(&self, draft: CreateRoom) -> Result<String, SpaceError> {
        self.create_channel(draft, CreateKind::Space).await
    }

    async fn create_channel(
        &self,
        draft: CreateRoom,
        kind: CreateKind,
    ) -> Result<String, SpaceError> {
        draft.validate()?;
        let _parent_sync = if let Some(id) = &draft.parent {
            Some(
                self.sync_rooms(vec![RoomId::parse(id).map_err(|_| SpaceError::InvalidId)?])
                    .await
                    .map_err(|_| SpaceError::Unavailable)?,
            )
        } else {
            None
        };
        let parent = match draft.parent {
            Some(id) => {
                let id = RoomId::parse(id).map_err(|_| SpaceError::InvalidId)?;
                let room = self
                    .client
                    .get_room(&id)
                    .filter(|r| r.is_space() && r.state() == RoomState::Joined)
                    .ok_or(SpaceError::Unavailable)?;
                let powers = room
                    .power_levels()
                    .await
                    .map_err(|_| SpaceError::Permission)?;
                if !powers.user_can_send_state(room.own_user_id(), StateEventType::SpaceChild) {
                    return Err(SpaceError::Permission);
                }
                Some(id)
            }
            None => None,
        };
        let mut request = create_room::v3::Request::new();
        request.name = Some(draft.name.trim().into());
        request.topic = (!draft.topic.trim().is_empty()).then(|| draft.topic.trim().into());
        request.preset = Some(create_room::v3::RoomPreset::PrivateChat);
        if kind == CreateKind::Space {
            let mut creation = create_room::v3::CreationContent::new();
            creation.room_type = Some(matrix_sdk::ruma::room::RoomType::Space);
            request.creation_content =
                Some(matrix_sdk::ruma::serde::Raw::new(&creation).map_err(|_| SpaceError::Create)?);
        }
        if kind == CreateKind::Voice {
            let mut creation = create_room::v3::CreationContent::new();
            creation.room_type = Some(crate::channels::VOICE_ROOM_TYPE.into());
            request.creation_content =
                Some(matrix_sdk::ruma::serde::Raw::new(&creation).map_err(|_| SpaceError::Create)?);
            let mut powers = create_room::RoomPowerLevelsContentOverride::new();
            powers.users.insert(
                self.client
                    .user_id()
                    .ok_or(SpaceError::Unavailable)?
                    .to_owned(),
                100.into(),
            );
            powers
                .events
                .insert(crate::calls::native::MEMBER_EVENT.into(), 0.into());
            powers.events.insert("m.room.message".into(), 100.into());
            powers.events.insert("m.room.encrypted".into(), 100.into());
            request.power_level_content_override =
                Some(matrix_sdk::ruma::serde::Raw::new(&powers).map_err(|_| SpaceError::Create)?);
        }
        if let Some(parent) = &parent {
            // Like Element's "visible to space members": anyone in the parent
            // space can join, instead of an invite-only room nobody can enter.
            request.initial_state.push(
                InitialStateEvent::with_empty_state_key(RoomJoinRulesEventContent::restricted(
                    vec![AllowRule::room_membership(parent.clone())],
                ))
                .to_raw_any(),
            );
        }
        if kind != CreateKind::Space && (draft.encrypted || kind == CreateKind::Voice) {
            request.initial_state.push(
                InitialStateEvent::with_empty_state_key(
                    RoomEncryptionEventContent::with_recommended_defaults(),
                )
                .to_raw_any(),
            );
        }
        let room = self
            .client
            .create_room(request)
            .await
            .map_err(|_| SpaceError::Create)?;
        let id = room.room_id().to_string();
        if let Some(parent) = parent {
            let _child_sync = self
                .sync_rooms(vec![room.room_id().to_owned()])
                .await
                .map_err(|_| SpaceError::ParentLink { room: id.clone() })?;
            SpaceService::new(self.client.clone())
                .await
                .add_child_to_space(room.room_id().to_owned(), parent)
                .await
                .map_err(|_| SpaceError::ParentLink { room: id.clone() })?;
        }
        Ok(id)
    }
    /// Creates a space and returns its ID. Public spaces anyone can join (and
    /// preview); private ones need an invitation. As in Element, only admins
    /// post in the space room itself, and members may invite.
    pub async fn create_space(&self, draft: CreateSpace) -> Result<String, SpaceError> {
        draft.validate()?;
        let mut request = create_room::v3::Request::new();
        request.name = Some(draft.name.trim().into());
        request.topic = (!draft.topic.trim().is_empty()).then(|| draft.topic.trim().into());
        request.preset = Some(if draft.public {
            create_room::v3::RoomPreset::PublicChat
        } else {
            create_room::v3::RoomPreset::PrivateChat
        });
        let mut creation = create_room::v3::CreationContent::new();
        creation.room_type = Some(matrix_sdk::ruma::room::RoomType::Space);
        request.creation_content =
            Some(matrix_sdk::ruma::serde::Raw::new(&creation).map_err(|_| SpaceError::Create)?);
        let mut powers = create_room::RoomPowerLevelsContentOverride::new();
        powers.events_default = Some(100.into());
        powers.invite = Some(0.into());
        request.power_level_content_override =
            Some(matrix_sdk::ruma::serde::Raw::new(&powers).map_err(|_| SpaceError::Create)?);
        let room = self
            .client
            .create_room(request)
            .await
            .map_err(|_| SpaceError::Create)?;
        Ok(room.room_id().to_string())
    }
    /// Leaves one joined room or direct message. The room list updates through sync.
    pub async fn leave_room(&self, room_id: &str) -> Result<(), SpaceError> {
        let room_id =
            matrix_sdk::ruma::RoomId::parse(room_id).map_err(|_| SpaceError::InvalidId)?;
        let room = self
            .client
            .get_room(&room_id)
            .ok_or(SpaceError::Unavailable)?;
        room.leave().await.map_err(|_| SpaceError::Leave)
    }
    /// Invites one user to a room this account is in. The room's power levels
    /// decide who may invite; `InviteDenied` reports a missing permission.
    pub async fn invite_user(&self, room_id: &str, user_id: &str) -> Result<(), SpaceError> {
        let user = UserId::parse(user_id.trim()).map_err(|_| SpaceError::InvalidId)?;
        let room_id =
            matrix_sdk::ruma::RoomId::parse(room_id).map_err(|_| SpaceError::InvalidId)?;
        let room = self
            .client
            .get_room(&room_id)
            .ok_or(SpaceError::Unavailable)?;
        // A known member who lacks the invite power level gets a clear answer
        // without a round trip; otherwise the server decides.
        if let Some(me) = self.client.user_id()
            && let Ok(Some(member)) = room.get_member_no_sync(me).await
            && !member.can_invite()
        {
            return Err(SpaceError::InviteDenied);
        }
        room.invite_user_by_id(&user).await.map_err(|error| {
            if error
                .as_client_api_error()
                .is_some_and(|e| e.status_code == matrix_sdk::reqwest::StatusCode::FORBIDDEN)
            {
                SpaceError::InviteDenied
            } else {
                SpaceError::InviteFailed
            }
        })
    }
    pub async fn start_dm(&self, id: &str) -> Result<String, SpaceError> {
        let user = UserId::parse(id).map_err(|_| SpaceError::InvalidId)?;
        if let Some(room) = self.client.get_dm_room(&user) {
            return Ok(room.room_id().to_string());
        }
        self.client
            .create_dm(&user)
            .await
            .map(|r| r.room_id().to_string())
            .map_err(|_| SpaceError::Create)
    }
}
