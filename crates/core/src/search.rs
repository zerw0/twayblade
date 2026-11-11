//! Search without an application index. Queries, cursors and excerpts deliberately
//! have no Debug/Serialize implementation and are held only in memory.
use crate::auth::Account;
use matrix_sdk::ruma::{OwnedRoomId, OwnedUserId, api::client::search::search_events::v3 as api};
use std::{ops::Range, sync::Arc};
#[derive(Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub room: Option<OwnedRoomId>,
    pub sender: Option<OwnedUserId>,
    pub has_file: bool,
}
#[derive(Clone)]
pub struct SearchResult {
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub sender_name: String,
    pub timestamp_ms: i64,
    pub snippet: String,
    /// UTF-8 byte ranges in `snippet`, always on character boundaries.
    pub match_ranges: Vec<Range<usize>>,
}
#[derive(Clone)]
pub struct SearchToken {
    account: u128,
    query: SearchQuery,
    cached: Arc<Vec<SearchResult>>,
    offset: usize,
    rooms: Vec<OwnedRoomId>,
    server: Option<String>,
    server_done: bool,
    limited: bool,
}
pub struct SearchPage {
    pub results: Vec<SearchResult>,
    pub next: Option<SearchToken>,
    pub limited: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    #[error("search_invalid_query")]
    InvalidQuery,
    #[error("search_unavailable")]
    Unavailable,
    #[error("search_request_failed")]
    Request,
    #[error("search_scope_limit")]
    ScopeLimit,
}
fn ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let mut folded = String::new();
    let mut positions = Vec::new();
    for (start, ch) in text.char_indices() {
        for lower in ch.to_lowercase() {
            for _ in 0..lower.len_utf8() {
                positions.push((start, start + ch.len_utf8()));
            }
            folded.push(lower);
        }
    }
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    folded
        .match_indices(&needle)
        .take(32)
        .map(|(i, m)| positions[i].0..positions[i + m.len() - 1].1)
        .collect()
}
fn project(
    value: &serde_json::Value,
    room: &str,
    query: &SearchQuery,
    local: bool,
) -> Option<SearchResult> {
    if value.get("type")?.as_str()? != "m.room.message" {
        return None;
    }
    let content = value.get("content")?;
    let msgtype = content.get("msgtype")?.as_str()?;
    let sender = value.get("sender")?.as_str()?;
    if query.sender.as_ref().is_some_and(|s| s.as_str() != sender) {
        return None;
    }
    if query.has_file && !matches!(msgtype, "m.image" | "m.file" | "m.video" | "m.audio") {
        return None;
    }
    let body = content.get("body")?.as_str()?;
    let hits = ranges(body, &query.text);
    if local && hits.is_empty() {
        return None;
    }
    let start = hits
        .first()
        .map(|r| {
            body[..r.start]
                .char_indices()
                .rev()
                .nth(60)
                .map(|(i, _)| i)
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let snippet: String = body[start..].chars().take(240).collect();
    let match_ranges = ranges(&snippet, &query.text);
    Some(SearchResult {
        room_id: room.into(),
        event_id: value.get("event_id")?.as_str()?.into(),
        sender: sender.into(),
        sender_name: sender.into(),
        timestamp_ms: value.get("origin_server_ts")?.as_i64()?,
        snippet,
        match_ranges,
    })
}
impl Account {
    pub async fn search(
        &self,
        query: SearchQuery,
        from: Option<SearchToken>,
    ) -> Result<SearchPage, SearchError> {
        if query.text.trim().is_empty() || query.text.len() > 4096 {
            return Err(SearchError::InvalidQuery);
        }
        let mut cursor = if let Some(cursor) = from {
            if cursor.account != self.instance_id() || cursor.query != query {
                return Err(SearchError::InvalidQuery);
            }
            cursor
        } else {
            let rooms = if let Some(room) = &query.room {
                vec![
                    self.client
                        .get_room(room)
                        .filter(|r| r.state() == matrix_sdk::RoomState::Joined)
                        .ok_or(SearchError::Unavailable)?,
                ]
            } else {
                self.client.joined_rooms()
            };
            if rooms.len() > 500 {
                return Err(SearchError::ScopeLimit);
            }
            self.client
                .event_cache()
                .subscribe()
                .map_err(|_| SearchError::Unavailable)?;
            let mut plain = Vec::new();
            let mut cached = Vec::new();
            let mut limited = false;
            for room in rooms {
                let encrypted = room
                    .latest_encryption_state()
                    .await
                    .map_err(|_| SearchError::Unavailable)?
                    .is_encrypted();
                if !encrypted {
                    plain.push(room.room_id().to_owned());
                    continue;
                }
                limited = true;
                let (cache, _guard) = self
                    .client
                    .event_cache()
                    .room(room.room_id())
                    .await
                    .map_err(|_| SearchError::Unavailable)?;
                // Per-room cache is bounded by the 1,000-result cap. Member reads
                // use the SDK store only; never fetch members for search results.
                let mut names = std::collections::HashMap::<String, Option<String>>::new();
                // No pagination or network fetching: only already-decrypted cache events.
                for event in cache
                    .events()
                    .await
                    .map_err(|_| SearchError::Unavailable)?
                    .into_iter()
                    .rev()
                    .take(2000)
                {
                    if cached.len() >= 1000 {
                        break;
                    }
                    if let Ok(value) = serde_json::from_str(event.raw().json().get())
                        && let Some(mut result) =
                            project(&value, room.room_id().as_str(), &query, true)
                    {
                        if !names.contains_key(&result.sender) {
                            let name =
                                if let Ok(user) = matrix_sdk::ruma::UserId::parse(&result.sender) {
                                    room.get_member_no_sync(&user)
                                        .await
                                        .ok()
                                        .flatten()
                                        .and_then(|m| {
                                            m.display_name()
                                                .filter(|n| !n.is_empty())
                                                .map(str::to_owned)
                                        })
                                } else {
                                    None
                                };
                            names.insert(result.sender.clone(), name);
                        }
                        if let Some(Some(name)) = names.get(&result.sender) {
                            result.sender_name = name.clone();
                        }
                        cached.push(result);
                    }
                }
            }
            cached.sort_by(|a, b| {
                b.timestamp_ms
                    .cmp(&a.timestamp_ms)
                    .then_with(|| a.event_id.cmp(&b.event_id))
            });
            SearchToken {
                account: self.instance_id(),
                query: query.clone(),
                cached: Arc::new(cached),
                offset: 0,
                server_done: plain.is_empty(),
                rooms: plain,
                server: None,
                limited,
            }
        };
        let mut results = Vec::new();
        if cursor.offset < cursor.cached.len() {
            let end = (cursor.offset + 50).min(cursor.cached.len());
            results.extend_from_slice(&cursor.cached[cursor.offset..end]);
            cursor.offset = end;
        } else if !cursor.server_done {
            let mut criteria = api::Criteria::new(query.text.clone());
            criteria.keys = Some(vec![api::SearchKeys::ContentBody]);
            criteria.order_by = Some(api::OrderBy::Recent);
            criteria.filter.rooms = Some(cursor.rooms.clone());
            criteria.filter.types = Some(vec!["m.room.message".into()]);
            criteria.filter.limit = Some(50u32.into());
            criteria.filter.senders = query.sender.clone().map(|s| vec![s]);
            criteria.event_context.include_profile = true;
            let mut categories = api::Categories::new();
            categories.room_events = Some(criteria);
            let mut request = api::Request::new(categories);
            request.next_batch = cursor.server.clone();
            let response = self
                .client
                .send(request)
                .await
                .map_err(|_| SearchError::Request)?
                .search_categories
                .room_events;
            for result in response.results.into_iter().take(50) {
                let Some(raw) = result.result else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.json().get()) else {
                    continue;
                };
                let Some(room) = value
                    .get("room_id")
                    .and_then(|r| r.as_str())
                    .filter(|r| cursor.rooms.iter().any(|id| id.as_str() == *r))
                else {
                    continue;
                };
                if let Some(mut item) = project(&value, room, &query, false) {
                    if let Ok(sender) = matrix_sdk::ruma::UserId::parse(&item.sender)
                        && let Some(name) = result
                            .context
                            .profile_info
                            .get(&sender)
                            .and_then(|p| p.displayname.clone())
                    {
                        item.sender_name = name;
                    }
                    results.push(item);
                }
            }
            let next = response
                .next_batch
                .filter(|next| Some(next) != cursor.server.as_ref());
            cursor.server_done = next.is_none();
            cursor.server = next;
        }
        let limited = cursor.limited;
        let next = (cursor.offset < cursor.cached.len() || !cursor.server_done).then_some(cursor);
        Ok(SearchPage {
            results,
            next,
            limited,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_ranges_and_bounded_snippets_preserve_plain_fallback() {
        let query = SearchQuery {
            text: "КОД".into(),
            room: None,
            sender: None,
            has_file: false,
        };
        let event = serde_json::json!({"type":"m.room.message", "event_id":"$event", "sender":"@a:example.org", "origin_server_ts":123, "content":{"msgtype":"m.text","body":format!("{}код & plain", "x".repeat(1000)),"formatted_body":"<script>bad</script>"}});
        let result = project(&event, "!r:example.org", &query, true).unwrap();
        assert!(result.snippet.chars().count() <= 240);
        assert_eq!(&result.snippet[result.match_ranges[0].clone()], "код");
        assert!(!result.snippet.contains("script"));
        assert!(
            project(
                &event,
                "!r:example.org",
                &SearchQuery {
                    has_file: true,
                    ..query
                },
                true
            )
            .is_none()
        );
    }
    #[test]
    fn encrypted_envelopes_and_unmatched_cached_events_are_excluded() {
        let query = SearchQuery {
            text: "secret".into(),
            room: None,
            sender: None,
            has_file: false,
        };
        assert!(
            project(
                &serde_json::json!({"type":"m.room.encrypted", "content":{"body":"secret"}}),
                "!r:example.org",
                &query,
                true
            )
            .is_none()
        );
        assert_eq!(SearchError::Request.to_string(), "search_request_failed");
    }
}

#[cfg(test)]
mod integration;
