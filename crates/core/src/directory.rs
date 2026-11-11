//! Public discovery. No membership changes occur while browsing or previewing.
use crate::auth::Account;
use matrix_sdk::ruma::api::error::ErrorKind;
use matrix_sdk::{
    RoomState,
    ruma::{
        self, MatrixToUri, MatrixUri, OwnedRoomOrAliasId, OwnedServerName, RoomOrAliasId,
        api::client::directory::get_public_rooms_filtered::v3 as api,
        directory::{PublicRoomsChunk, RoomNetwork},
        matrix_uri::MatrixId,
        room::{JoinRuleKind, RoomType},
    },
};
pub type ServerName = OwnedServerName;
pub type ThirdPartyNetwork = String;
#[derive(Clone)]
pub struct Token {
    account: u128,
    server: Option<ServerName>,
    filter: Option<String>,
    network: Option<ThirdPartyNetwork>,
    position: String,
    get: bool,
}
#[derive(Clone)]
pub struct DirectoryQuery {
    pub server: Option<ServerName>,
    pub filter: Option<String>,
    pub network: Option<ThirdPartyNetwork>,
    pub since: Option<Token>,
    pub limit: usize,
}
#[derive(Clone)]
pub struct DirectoryRoom {
    pub room_id: String,
    pub alias: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar: Option<String>,
    pub members: u64,
    pub world_readable: bool,
    pub guest_can_join: bool,
    pub join_rule: JoinRuleKind,
    pub is_space: bool,
    pub joined: bool,
}
pub struct DirectoryPage {
    pub rooms: Vec<DirectoryRoom>,
    pub next: Option<Token>,
    pub total_estimate: Option<u64>,
}
#[derive(Clone)]
pub struct Network {
    pub id: String,
    pub name: String,
}
#[derive(Clone)]
pub struct RoomTarget {
    pub id: OwnedRoomOrAliasId,
    pub via: Vec<ServerName>,
}
impl RoomTarget {
    pub fn parse(input: &str, via: Vec<ServerName>) -> Result<Self, DirectoryError> {
        if input.len() > 8192 || via.len() > 16 {
            return Err(DirectoryError::InvalidTarget);
        }
        let (id, mut servers) = if input.starts_with("matrix:") {
            let uri: MatrixUri = input.parse().map_err(|_| DirectoryError::InvalidTarget)?;
            (uri.id().clone(), uri.via().to_vec())
        } else if input.starts_with("https://matrix.to/") {
            let uri: MatrixToUri = input.parse().map_err(|_| DirectoryError::InvalidTarget)?;
            (uri.id().clone(), uri.via().to_vec())
        } else {
            return Ok(Self {
                id: RoomOrAliasId::parse(input).map_err(|_| DirectoryError::InvalidTarget)?,
                via,
            });
        };
        let id = match id {
            MatrixId::Room(id) => id.into(),
            MatrixId::RoomAlias(id) => id.into(),
            MatrixId::Event(id, _) => id,
            _ => return Err(DirectoryError::InvalidTarget),
        };
        servers.extend(via);
        servers.sort();
        servers.dedup();
        if servers.len() > 16 {
            return Err(DirectoryError::InvalidTarget);
        }
        Ok(Self { id, via: servers })
    }
}
#[derive(Clone)]
pub struct PreviewMessage {
    pub event_id: String,
    pub sender: String,
    pub body: String,
    pub timestamp_ms: i64,
}
#[derive(Clone)]
pub struct RoomPreview {
    pub room: DirectoryRoom,
    pub messages: Vec<PreviewMessage>,
    pub history_error: Option<DirectoryError>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DirectoryError {
    #[error("directory_invalid_target")]
    InvalidTarget,
    #[error("directory_invalid_query")]
    InvalidQuery,
    #[error("directory_not_found")]
    NotFound,
    #[error("directory_forbidden")]
    Forbidden,
    #[error("directory_unreachable")]
    ServerUnreachable,
    #[error("directory_invite_required")]
    InviteRequired,
    #[error("directory_knock_required")]
    KnockRequired,
    #[error("directory_unsupported")]
    Unsupported,
}
fn http_error(error: &matrix_sdk::HttpError) -> DirectoryError {
    if error
        .as_client_api_error()
        .is_some_and(|e| e.status_code.as_u16() == 405)
    {
        return DirectoryError::Unsupported;
    }
    match error.client_api_error_kind() {
        Some(ErrorKind::NotFound) => DirectoryError::NotFound,
        Some(ErrorKind::Forbidden) => DirectoryError::Forbidden,
        Some(ErrorKind::Unrecognized) => DirectoryError::Unsupported,
        _ => DirectoryError::ServerUnreachable,
    }
}
fn sdk_error(error: matrix_sdk::Error) -> DirectoryError {
    match error.client_api_error_kind() {
        Some(ErrorKind::NotFound) => DirectoryError::NotFound,
        Some(ErrorKind::Forbidden) => DirectoryError::Forbidden,
        _ => DirectoryError::ServerUnreachable,
    }
}
impl Account {
    fn directory_room(&self, r: PublicRoomsChunk) -> DirectoryRoom {
        DirectoryRoom {
            joined: self
                .client
                .get_room(&r.room_id)
                .is_some_and(|r| r.state() == RoomState::Joined),
            room_id: r.room_id.to_string(),
            alias: r.canonical_alias.map(|v| v.to_string()),
            name: r.name,
            topic: r.topic,
            avatar: r.avatar_url.map(|v| v.to_string()),
            members: r.num_joined_members.into(),
            world_readable: r.world_readable,
            guest_can_join: r.guest_can_join,
            join_rule: r.join_rule,
            is_space: r.room_type == Some(RoomType::Space),
        }
    }
    pub async fn public_rooms(
        &self,
        query: DirectoryQuery,
    ) -> Result<DirectoryPage, DirectoryError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(20),
            self.public_rooms_inner(query),
        )
        .await
        .unwrap_or(Err(DirectoryError::ServerUnreachable))
    }
    async fn public_rooms_inner(
        &self,
        query: DirectoryQuery,
    ) -> Result<DirectoryPage, DirectoryError> {
        if !(1..=100).contains(&query.limit)
            || query.filter.as_ref().is_some_and(|s| s.len() > 1024)
            || query.network.as_ref().is_some_and(|s| s.len() > 256)
        {
            return Err(DirectoryError::InvalidQuery);
        }
        if query.since.as_ref().is_some_and(|t| {
            t.account != self.instance_id()
                || t.server != query.server
                || t.filter != query.filter
                || t.network != query.network
        }) {
            return Err(DirectoryError::InvalidQuery);
        }
        let previous = query.since.as_ref().map(|t| t.position.clone());
        let mut get = query.since.as_ref().is_some_and(|t| t.get);
        let mut request = api::Request::new();
        request.server = query.server.clone();
        request.limit = Some((query.limit as u32).into());
        request.since = previous.clone();
        request.filter.generic_search_term = query.filter.clone();
        if let Some(network) = &query.network {
            request.room_network = RoomNetwork::ThirdParty(network.clone());
        }
        let response = if get {
            None
        } else {
            match self.client.public_rooms_filtered(request).await {
                Ok(r) => Some(r),
                Err(e)
                    if query.server.is_some()
                        && matches!(
                            http_error(&e),
                            DirectoryError::Unsupported
                                | DirectoryError::NotFound
                                | DirectoryError::Forbidden
                        )
                        && query.network.is_none() =>
                {
                    get = true;
                    None
                }
                Err(e) => return Err(http_error(&e)),
            }
        };
        let (chunk, next, total) = match response {
            Some(r) => (r.chunk, r.next_batch, r.total_room_count_estimate),
            None => {
                let r = self
                    .client
                    .public_rooms(
                        Some(query.limit as u32),
                        previous.as_deref(),
                        query.server.as_deref(),
                    )
                    .await
                    .map_err(|e| http_error(&e))?;
                (r.chunk, r.next_batch, r.total_room_count_estimate)
            }
        };
        if chunk.len() > query.limit {
            return Err(DirectoryError::InvalidQuery);
        }
        let rooms = chunk
            .into_iter()
            .filter(|r| {
                !get || query.filter.as_ref().is_none_or(|f| {
                    let f = f.to_lowercase();
                    [
                        r.name.as_deref(),
                        r.topic.as_deref(),
                        r.canonical_alias.as_ref().map(|a| a.as_str()),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|s| s.to_lowercase().contains(&f))
                })
            })
            .map(|r| self.directory_room(r))
            .collect();
        let next = next
            .filter(|n| Some(n) != previous.as_ref())
            .map(|position| Token {
                account: self.instance_id(),
                server: query.server,
                filter: query.filter,
                network: query.network,
                position,
                get,
            });
        Ok(DirectoryPage {
            rooms,
            next,
            total_estimate: total.map(u64::from),
        })
    }
    pub async fn directory_networks(
        &self,
        server: Option<ServerName>,
    ) -> Result<Vec<Network>, DirectoryError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.directory_networks_inner(server),
        )
        .await
        .unwrap_or(Err(DirectoryError::ServerUnreachable))
    }
    async fn directory_networks_inner(
        &self,
        server: Option<ServerName>,
    ) -> Result<Vec<Network>, DirectoryError> {
        // The client API exposes protocols only for the authenticated homeserver.
        if server.as_ref().is_some_and(|s| {
            self.client
                .user_id()
                .is_none_or(|u| u.server_name() != &**s)
        }) {
            return Ok(Vec::new());
        }
        use ruma::api::client::thirdparty::get_protocols::v3::Request;
        match self.client.send(Request::new()).await {
            Ok(r) => Ok(r
                .protocols
                .into_values()
                .flat_map(|p| {
                    p.instances
                        .into_iter()
                        .filter_map(|i| i.instance_id.map(|id| Network { id, name: i.desc }))
                })
                .take(64)
                .collect()),
            Err(e)
                if matches!(
                    http_error(&e),
                    DirectoryError::Unsupported | DirectoryError::NotFound
                ) =>
            {
                Ok(Vec::new())
            }
            Err(e) => Err(http_error(&e)),
        }
    }
    pub async fn preview_room(&self, target: RoomTarget) -> Result<RoomPreview, DirectoryError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.preview_room_inner(target),
        )
        .await
        .unwrap_or(Err(DirectoryError::ServerUnreachable))
    }
    async fn preview_room_inner(&self, target: RoomTarget) -> Result<RoomPreview, DirectoryError> {
        let summary = self
            .client
            .get_room_preview(&target.id, target.via.clone())
            .await;
        let room = match summary {
            Ok(r) => DirectoryRoom {
                room_id: r.room_id.to_string(),
                alias: r.canonical_alias.map(|a| a.to_string()),
                name: r.name,
                topic: r.topic,
                avatar: r.avatar_url.map(|a| a.to_string()),
                members: r.num_joined_members,
                world_readable: r.is_world_readable.unwrap_or(false),
                guest_can_join: false,
                join_rule: r
                    .join_rule
                    .map(|j| JoinRuleKind::from(j.as_str()))
                    .unwrap_or(JoinRuleKind::Invite),
                is_space: r.room_type == Some(RoomType::Space),
                joined: r.state == Some(RoomState::Joined),
            },
            Err(e) => {
                let id = self.resolve_directory_target(&target).await?;
                let mut since = None;
                let mut found = None;
                for _ in 0..4 {
                    let page = self
                        .public_rooms(DirectoryQuery {
                            server: target.via.first().cloned(),
                            filter: None,
                            network: None,
                            since,
                            limit: 100,
                        })
                        .await?;
                    found = page.rooms.into_iter().find(|r| r.room_id == id.as_str());
                    since = page.next;
                    if found.is_some() || since.is_none() {
                        break;
                    }
                }
                found.ok_or_else(|| sdk_error(e))?
            }
        };
        let mut preview = RoomPreview {
            room,
            messages: Vec::new(),
            history_error: None,
        };
        if preview.room.world_readable {
            use ruma::api::client::message::get_message_events::v3::Request;
            let mut request = Request::backward(
                ruma::RoomId::parse(&preview.room.room_id)
                    .map_err(|_| DirectoryError::InvalidTarget)?,
            );
            request.limit = 20u32.into();
            match self.client.send(request).await {
                Ok(r) => {
                    for raw in r.chunk.into_iter().take(20) {
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.json().get())
                        else {
                            continue;
                        };
                        if v["type"] != "m.room.message" {
                            continue;
                        }
                        if let (Some(id), Some(sender), Some(body), Some(ts)) = (
                            v["event_id"].as_str(),
                            v["sender"].as_str(),
                            v["content"]["body"].as_str(),
                            v["origin_server_ts"].as_i64(),
                        ) {
                            preview.messages.push(PreviewMessage {
                                event_id: id.into(),
                                sender: sender.into(),
                                body: body.chars().take(4096).collect(),
                                timestamp_ms: ts,
                            });
                        }
                    }
                }
                Err(e) => preview.history_error = Some(http_error(&e)),
            }
        }
        Ok(preview)
    }
    async fn resolve_directory_target(
        &self,
        target: &RoomTarget,
    ) -> Result<ruma::OwnedRoomId, DirectoryError> {
        match <&ruma::RoomId>::try_from(&*target.id) {
            Ok(id) => Ok(id.to_owned()),
            Err(alias) => self
                .client
                .resolve_room_alias(alias)
                .await
                .map(|r| r.room_id)
                .map_err(|e| http_error(&e)),
        }
    }
    pub async fn join_room(&self, target: RoomTarget) -> Result<String, DirectoryError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.join_room_inner(target),
        )
        .await
        .unwrap_or(Err(DirectoryError::ServerUnreachable))
    }
    async fn join_room_inner(&self, target: RoomTarget) -> Result<String, DirectoryError> {
        match self
            .client
            .join_room_by_id_or_alias(&target.id, &target.via)
            .await
        {
            Ok(room) => Ok(room.room_id().to_string()),
            Err(e) => {
                let error = sdk_error(e);
                if error == DirectoryError::Forbidden
                    && let Ok(preview) = self.preview_room(target).await
                {
                    return Err(match preview.room.join_rule {
                        JoinRuleKind::Knock | JoinRuleKind::KnockRestricted => {
                            DirectoryError::KnockRequired
                        }
                        JoinRuleKind::Invite => DirectoryError::InviteRequired,
                        _ => error,
                    });
                }
                Err(error)
            }
        }
    }
    pub async fn knock_room(
        &self,
        target: RoomTarget,
        reason: Option<String>,
    ) -> Result<String, DirectoryError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.knock_room_inner(target, reason),
        )
        .await
        .unwrap_or(Err(DirectoryError::ServerUnreachable))
    }
    async fn knock_room_inner(
        &self,
        target: RoomTarget,
        reason: Option<String>,
    ) -> Result<String, DirectoryError> {
        if reason
            .as_ref()
            .is_some_and(|r| r.chars().count() > 256 || r.chars().any(char::is_control))
        {
            return Err(DirectoryError::InvalidQuery);
        }
        self.client
            .knock(target.id, reason, target.via)
            .await
            .map(|r| r.room_id().to_string())
            .map_err(sdk_error)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn room_targets_reject_users_and_external_links() {
        for target in [
            "@u:example.org",
            "https://evil.invalid/#/!r:example.org",
            "matrix:u/u:example.org",
            "https://matrix.to/#/@u:example.org",
            "garbage",
        ] {
            assert!(RoomTarget::parse(target, vec![]).is_err());
        }
        for target in [
            "#a:example.org",
            "!r:example.org",
            "https://matrix.to/#/%23a:example.org?via=example.org",
            "matrix:roomid/r:example.org",
            "https://matrix.to/#/!r:example.org/$event",
        ] {
            assert!(RoomTarget::parse(target, vec![]).is_ok(), "{target}");
        }
    }
}

#[cfg(test)]
mod integration;
