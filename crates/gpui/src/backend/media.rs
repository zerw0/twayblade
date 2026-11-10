#![allow(
    dead_code,
    reason = "Gallery contracts consumed by the separately implemented UI"
)]
//! Gallery lifecycle, independent of view code.
use super::*;
use matrix_client_core::media::{MediaError, MediaItem, MediaKind, Token};
const MAX_ITEMS: usize = 2000;
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
impl State {
    pub(super) fn cancel_media(&mut self) {
        if let Some(task) = self.media_task.take() {
            task.abort();
        }
        self.media_request = self.media_request.wrapping_add(1);
    }
    fn accepts_media(&self, account: u128, selection: u64, request: u64) -> bool {
        self.account.as_ref().map(Account::instance_id) == Some(account)
            && self.selection == selection
            && self.media_request == request
    }
}
impl Backend {
    pub(crate) fn load_media(&self, kind: MediaKind) -> Result<(), ServiceError> {
        self.request_media(Some(kind))
    }
    pub(crate) fn load_more_media(&self) -> Result<(), ServiceError> {
        self.request_media(None)
    }
    fn request_media(&self, kind: Option<MediaKind>) -> Result<(), ServiceError> {
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
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.media_task = Some(self.runtime.spawn(async move {
            let result = account.room_media(&room, kind, from, 50).await;
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
                            if !view.media.items.iter().any(|i| i.event_id == item.event_id)
                                && view.media.items.len() < MAX_ITEMS
                            {
                                view.media.items.push(item);
                            }
                        }
                        view.media.next = page.next;
                        view.media.exhausted =
                            view.media.next.is_none() || view.media.items.len() == MAX_ITEMS;
                    }
                    Err(error) => view.media.error = Some(error),
                }
            });
        }));
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn room_switch_aborts_media_and_invalidates_requests() {
        let runtime = Runtime::new().unwrap();
        let mut state = State {
            selection: 7,
            media_request: 3,
            ..Default::default()
        };
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
        assert!(abort.is_finished());
        assert_eq!(state.media_request, 4);
        assert!(!state.accepts_media(1, 7, 3));
        assert!(!state.accepts_media(1, 8, 4));
    }
}
