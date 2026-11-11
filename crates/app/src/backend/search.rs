#![allow(
    dead_code,
    reason = "Search contracts consumed by the separately implemented UI"
)]
use super::*;
use twayblade_core::search::{SearchError, SearchQuery, SearchResult, SearchToken};
#[derive(Clone, Default)]
pub(crate) struct SearchState {
    pub query: Option<SearchQuery>,
    pub results: Vec<SearchResult>,
    pub loading: bool,
    pub exhausted: bool,
    pub limited: bool,
    pub error: Option<SearchError>,
    next: Option<SearchToken>,
}
impl State {
    pub(super) fn cancel_search(&mut self) {
        if let Some(task) = self.search_task.take() {
            task.abort();
        }
        self.search_request = self.search_request.wrapping_add(1);
    }
}
const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);
async fn debounced<T>(work: impl std::future::Future<Output = T>) -> T {
    tokio::time::sleep(DEBOUNCE).await;
    work.await
}
impl Backend {
    pub(crate) fn search(&self, query: SearchQuery) -> Result<(), ServiceError> {
        self.request_search(Some(query))
    }
    pub(crate) fn search_more(&self) -> Result<(), ServiceError> {
        self.request_search(None)
    }
    fn request_search(&self, query: Option<SearchQuery>) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = state
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        let old = self.updates.borrow().search.clone();
        let reset = query.is_some();
        if !reset && (old.loading || old.exhausted) {
            return Ok(());
        }
        let Some(query) = query.or(old.query) else {
            return Ok(());
        };
        state.cancel_search();
        let request = state.search_request;
        let instance = account.instance_id();
        let from = if reset { None } else { old.next };
        publish(&self.updates, |view| {
            if reset {
                view.search = SearchState {
                    query: Some(query.clone()),
                    ..Default::default()
                };
            }
            view.search.loading = true;
            view.search.error = None;
        });
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.search_task = Some(self.runtime.spawn(async move {
            let work = account.search(query, from);
            let result = if reset {
                debounced(work).await
            } else {
                work.await
            };
            let Ok(state) = shared.lock() else {
                return;
            };
            if state.account.as_ref().map(Account::instance_id) != Some(instance)
                || state.search_request != request
            {
                return;
            }
            publish(&updates, |view| {
                view.search.loading = false;
                match result {
                    Ok(page) => {
                        for item in page.results {
                            if view.search.results.len() < 1000
                                && !view.search.results.iter().any(|i| {
                                    i.room_id == item.room_id && i.event_id == item.event_id
                                })
                            {
                                view.search.results.push(item);
                            }
                        }
                        view.search.limited = page.limited;
                        view.search.next = page.next;
                        view.search.exhausted =
                            view.search.next.is_none() || view.search.results.len() >= 1000;
                    }
                    Err(error) => view.search.error = Some(error),
                }
            });
        }));
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn replacement_during_debounce_never_starts_superseded_work() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let old = tokio::spawn(debounced({
            let tx = tx.clone();
            async move {
                tx.send(1).await.unwrap();
            }
        }));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(rx.try_recv().is_err());
        old.abort();
        assert!(old.await.unwrap_err().is_cancelled());
        debounced(async move {
            tx.send(2).await.unwrap();
        })
        .await;
        assert_eq!(rx.recv().await, Some(2));
        assert!(rx.try_recv().is_err());
    }
}
