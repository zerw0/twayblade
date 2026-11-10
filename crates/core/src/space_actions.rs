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
}
#[derive(Clone, Default)]
pub struct DirectorySnapshot {
    pub space: String,
    pub name: String,
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
                access: match r.join_rule {
                    Some(JoinRuleSummary::Public) => JoinAccess::Public,
                    Some(JoinRuleSummary::Restricted(_) | JoinRuleSummary::KnockRestricted(_)) => {
                        JoinAccess::Restricted
                    }
                    Some(JoinRuleSummary::Invite | JoinRuleSummary::Knock) => JoinAccess::Invite,
                    _ => JoinAccess::Unknown,
                },
                via: r.via.iter().map(ToString::to_string).collect(),
            })
            .collect();
        DirectorySnapshot {
            space: self.space.clone(),
            name: space
                .as_ref()
                .map(|s| s.display_name.clone())
                .unwrap_or_else(|| self.space.clone()),
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
#[derive(Clone)]
pub struct CreateRoom {
    pub name: String,
    pub topic: String,
    pub encrypted: bool,
    pub parent: Option<String>,
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
        if draft.name.trim().is_empty() {
            return Err(SpaceError::InvalidId);
        }
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
        if draft.encrypted {
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
