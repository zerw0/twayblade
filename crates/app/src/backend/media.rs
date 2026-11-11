#![allow(
    dead_code,
    reason = "Gallery contracts consumed by the separately implemented UI"
)]
//! Gallery lifecycle, independent of view code.
use super::*;
use twayblade_core::media::{MediaError, MediaItem, MediaKind, Token};
#[derive(Clone, Default)]
pub(crate) struct MediaState {
    pub room: String,
    pub kind: MediaKind,
    /// Newest first, retaining SDK pagination order.
    pub items: Vec<MediaItem>,
    pub loading: bool,
    pub exhausted: bool,
    pub error: Option<MediaError>,
    next: Option<Token>,
}
fn current_request(expected: (u128, u64, u64), actual: (Option<u128>, u64, u64)) -> bool {
    actual == (Some(expected.0), expected.1, expected.2)
}
const THUMBNAIL_CAPACITY: usize = 256;
fn insert_thumbnail(
    map: &mut std::collections::HashMap<String, Arc<Vec<u8>>>,
    order: &mut std::collections::VecDeque<String>,
    id: String,
    png: Arc<Vec<u8>>,
) {
    order.retain(|old| old != &id);
    order.push_back(id.clone());
    map.insert(id, png);
    while order.len() > THUMBNAIL_CAPACITY {
        if let Some(old) = order.pop_front() {
            map.remove(&old);
        }
    }
}
impl State {
    pub(super) fn cancel_media(&mut self) {
        if let Some(cancel) = self.media_cancel.take() {
            cancel.send_replace(true);
        }
        if let Some(task) = self.media_task.take() {
            task.abort();
        }
        self.media_request = self.media_request.wrapping_add(1);
    }
    fn accepts_media(&self, account: u128, selection: u64, request: u64) -> bool {
        current_request(
            (account, selection, request),
            (
                self.account.as_ref().map(Account::instance_id),
                self.selection,
                self.media_request,
            ),
        )
    }
}
impl Backend {
    pub(crate) fn load_media(
        &self,
        kind: MediaKind,
        account: u128,
        room: &str,
    ) -> Result<(), ServiceError> {
        self.request_media(Some(kind), account, room)
    }
    pub(crate) fn load_more_media(&self, account: u128, room: &str) -> Result<(), ServiceError> {
        self.request_media(None, account, room)
    }
    fn request_media(
        &self,
        kind: Option<MediaKind>,
        expected_account: u128,
        expected_room: &str,
    ) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = state
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        let view = self.updates.borrow().clone();
        let room = view.selected_room;
        if account.instance_id() != expected_account || room != expected_room {
            return Err(failure("Conversation changed."));
        }
        if room.is_empty() {
            return Err(failure("Select a conversation first."));
        }
        if kind.is_none() && (view.media.room != room || view.media.loading || view.media.exhausted)
        {
            return Ok(());
        }
        state.cancel_media();
        let request = state.media_request;
        let selection = state.selection;
        let instance = account.instance_id();
        let reset = kind.is_some();
        let kind = kind.unwrap_or(view.media.kind);
        let from = if reset { None } else { view.media.next };
        publish(&self.updates, |view| {
            if reset {
                view.media = MediaState {
                    room: room.clone(),
                    kind,
                    ..Default::default()
                };
            }
            view.media.loading = true;
            view.media.error = None;
        });
        let (cancel_tx, cancel) = watch::channel(false);
        state.media_cancel = Some(cancel_tx);
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.media_task = Some(self.runtime.spawn(async move {
            let result = account.room_media(&room, kind, from, 50).await;
            let fresh = result.as_ref().map(|p| p.items.clone()).unwrap_or_default();
            let pending = {
                let Ok(state) = shared.lock() else {
                    return;
                };
                if !state.accepts_media(instance, selection, request) {
                    return;
                }
                publish(&updates, |view| {
                    view.media.loading = false;
                    match result {
                        Ok(page) => {
                            for item in page.items {
                                if let Some(png) =
                                    view.media_thumbnails.get(&item.event_id).cloned()
                                {
                                    insert_thumbnail(
                                        &mut view.media_thumbnails,
                                        &mut view.media_thumbnail_order,
                                        item.event_id.clone(),
                                        png,
                                    );
                                }
                                if !view.media.items.iter().any(|i| i.event_id == item.event_id) {
                                    view.media.items.push(item);
                                }
                            }
                            view.media.next = page.next;
                            view.media.exhausted = view.media.next.is_none();
                        }
                        Err(error) => view.media.error = Some(error),
                    }
                });
                let view = updates.borrow();
                let mut seen = std::collections::HashSet::new();
                fresh
                    .into_iter()
                    .chain(view.media.items.iter().cloned())
                    .filter(|i| {
                        !view.media_thumbnails.contains_key(&i.event_id)
                            && seen.insert(i.event_id.clone())
                    })
                    .take(256)
                    .collect::<Vec<_>>()
            };
            for item in pending {
                let Ok(png) = account.gallery_thumbnail(&item, cancel.clone()).await else {
                    continue;
                };
                let Ok(state) = shared.lock() else {
                    return;
                };
                if !state.accepts_media(instance, selection, request) {
                    return;
                }
                publish(&updates, |view| {
                    insert_thumbnail(
                        &mut view.media_thumbnails,
                        &mut view.media_thumbnail_order,
                        item.event_id,
                        png,
                    )
                });
            }
        }));
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thumbnail_cache_evicts_least_recently_used_and_caps_memory() {
        let mut map = std::collections::HashMap::new();
        let mut order = std::collections::VecDeque::new();
        for n in 0..256 {
            insert_thumbnail(&mut map, &mut order, n.to_string(), Arc::new(vec![]));
        }
        insert_thumbnail(&mut map, &mut order, "0".into(), Arc::new(vec![1]));
        insert_thumbnail(&mut map, &mut order, "256".into(), Arc::new(vec![2]));
        assert_eq!(map.len(), 256);
        assert!(map.contains_key("0"));
        assert!(!map.contains_key("1"));
    }
    #[test]
    fn media_pages_require_the_same_account_selection_and_request() {
        assert!(current_request((1, 2, 3), (Some(1), 2, 3)));
        for actual in [
            (None, 2, 3),
            (Some(9), 2, 3),
            (Some(1), 9, 3),
            (Some(1), 2, 9),
        ] {
            assert!(!current_request((1, 2, 3), actual));
        }
    }
    #[test]
    fn room_switch_aborts_media_and_invalidates_requests() {
        let runtime = Runtime::new().unwrap();
        let mut state = State {
            selection: 7,
            media_request: 3,
            ..Default::default()
        };
        let (cancel_tx, cancel_rx) = watch::channel(false);
        state.media_cancel = Some(cancel_tx);
        let handle = runtime.spawn(std::future::pending());
        let abort = handle.abort_handle();
        state.media_task = Some(handle);
        state.cancel_media();
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while !abort.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
        assert!(*cancel_rx.borrow());
        assert!(abort.is_finished());
        assert_eq!(state.media_request, 4);
        assert!(!state.accepts_media(1, 7, 3));
        assert!(!state.accepts_media(1, 8, 4));
    }
}
