//! Joined-space navigation projected from the SDK's space hierarchy filters.

use std::collections::{HashMap, HashSet};

use futures_util::{Stream, StreamExt};
use matrix_sdk_ui::spaces::{SpaceFilter, SpaceService};

use crate::auth::Account;

#[derive(Clone, Default)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub level: u8,
    pub avatar: Option<String>,
    pub can_add_rooms: bool,
    pub room_ids: HashSet<String>,
}

#[derive(Clone)]
pub struct SpaceSnapshot {
    pub account_instance: u128,
    pub spaces: Vec<SpaceSummary>,
}

// Expand the SDK-provided filter relationships for inclusive parent navigation.
// Identity is the room ID; cycles and shared descendants must not duplicate rooms.
fn descendants(root: &str, edges: &HashMap<String, Vec<String>>) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(id) = pending.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        if let Some(children) = edges.get(&id) {
            pending.extend(children.iter().cloned());
        }
    }
    visited.remove(root);
    visited
}

fn summaries(
    filters: impl Iterator<Item = SpaceFilter>,
    editable: &HashSet<String>,
) -> Vec<SpaceSummary> {
    let filters: Vec<_> = filters.collect();
    let edges: HashMap<_, _> = filters
        .iter()
        .map(|filter| {
            (
                filter.space_room.room_id.to_string(),
                filter.descendants.iter().map(ToString::to_string).collect(),
            )
        })
        .collect();
    let mut seen = HashSet::new();
    filters
        .into_iter()
        .filter(|filter| seen.insert(filter.space_room.room_id.clone()))
        .map(|filter| {
            let id = filter.space_room.room_id.to_string();
            SpaceSummary {
                room_ids: descendants(&id, &edges),
                can_add_rooms: editable.contains(&id),
                id,
                avatar: filter.space_room.avatar_url.map(|url| url.to_string()),
                name: filter.space_room.display_name,
                level: filter.level,
            }
        })
        .collect()
}

impl Account {
    pub fn space_updates(&self) -> impl Stream<Item = SpaceSnapshot> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            let service = SpaceService::new(account.client.clone()).await;
            let (mut filters, updates) = service.subscribe_to_space_filters().await;
            let mut updates = Box::pin(updates);
            let mut room_updates = account.client.subscribe_to_all_room_updates();
            loop {
                yield SpaceSnapshot {
                    account_instance: account.instance_id(),
                    spaces: summaries(filters.iter().cloned(), &service.editable_spaces().await.into_iter().map(|r| r.room_id.to_string()).collect()),
                };
                tokio::select! {
                    next = updates.next() => { let Some(diffs) = next else { break; }; for diff in diffs { diff.apply(&mut filters); } }
                    update = room_updates.recv() => { if matches!(update, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_spaces_expand_shared_rooms_without_cycles_or_duplicates() {
        let edges = HashMap::from([
            ("parent".into(), vec!["nested".into(), "shared".into()]),
            (
                "nested".into(),
                vec!["parent".into(), "child".into(), "shared".into()],
            ),
        ]);
        assert_eq!(
            descendants("parent", &edges),
            HashSet::from(["nested".into(), "shared".into(), "child".into(),])
        );
        assert!(descendants("unknown", &edges).is_empty());
    }
}
