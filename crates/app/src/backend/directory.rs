#![allow(
    dead_code,
    reason = "Public discovery contracts for the UI contributor"
)]
use super::{Account, Backend, ServiceError, State, failure, publish};
use twayblade_core::directory::{
    DirectoryError, DirectoryQuery, DirectoryRoom, RoomPreview, RoomTarget, ServerName, Token,
};
type DirectorySelection = (Option<ServerName>, Option<String>, Option<String>);
#[derive(Clone, Default)]
pub(crate) struct DirectoryState {
    pub server: Option<ServerName>,
    pub filter: Option<String>,
    pub network: Option<String>,
    pub networks: Vec<twayblade_core::directory::Network>,
    pub networks_error: Option<DirectoryError>,
    pub rooms: Vec<DirectoryRoom>,
    pub loading: bool,
    pub exhausted: bool,
    pub error: Option<DirectoryError>,
    next: Option<Token>,
}
#[derive(Clone, Default)]
pub(crate) struct PreviewState {
    pub preview: Option<RoomPreview>,
    pub loading: bool,
    pub error: Option<DirectoryError>,
}
#[derive(Clone, Copy)]
pub(crate) struct MembershipTicket {
    account: u128,
    selection: u64,
    request: u64,
}
fn membership_matches(ticket: MembershipTicket, actual: (Option<u128>, u64, u64)) -> bool {
    actual == (Some(ticket.account), ticket.selection, ticket.request)
}
impl State {
    fn accepts_membership(&self, ticket: MembershipTicket) -> bool {
        membership_matches(
            ticket,
            (
                self.account.as_ref().map(Account::instance_id),
                self.selection,
                self.membership_request,
            ),
        )
    }
}
impl State {
    pub(super) fn cancel_discovery(&mut self) {
        self.membership_request = self.membership_request.wrapping_add(1);
        if let Some(task) = self.membership_task.take() {
            task.abort();
        }
        if let Some(task) = self.discovery_task.take() {
            task.abort();
        }
        self.discovery_request = self.discovery_request.wrapping_add(1);
        if let Some(task) = self.preview_task.take() {
            task.abort();
        }
        self.preview_request = self.preview_request.wrapping_add(1);
    }
}
impl Backend {
    pub(crate) fn browse_directory(
        &self,
        server: Option<ServerName>,
        filter: Option<String>,
    ) -> Result<(), ServiceError> {
        self.request_directory(Some((server, filter, None)))
    }
    pub(crate) fn browse_directory_network(
        &self,
        server: Option<ServerName>,
        filter: Option<String>,
        network: Option<String>,
    ) -> Result<(), ServiceError> {
        self.request_directory(Some((server, filter, network)))
    }
    pub(crate) fn directory_more(&self) -> Result<(), ServiceError> {
        self.request_directory(None)
    }
    fn request_directory(&self, query: Option<DirectorySelection>) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = state
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        let old = self.updates.borrow().directory.clone();
        let reset = query.is_some();
        if !reset && (old.loading || old.exhausted) {
            return Ok(());
        }
        let (server, filter, network) =
            query.unwrap_or((old.server.clone(), old.filter, old.network));
        state.cancel_discovery();
        let request = state.discovery_request;
        let instance = account.instance_id();
        let since = if reset { None } else { old.next };
        // Networks depend only on the server; keep them across filter changes.
        let fetch_networks = reset
            && (old.server != server || (old.networks.is_empty() && old.networks_error.is_some()));
        publish(&self.updates, |v| {
            if reset {
                let (networks, networks_error) = if fetch_networks {
                    (Vec::new(), None)
                } else {
                    (old.networks.clone(), old.networks_error)
                };
                v.directory = DirectoryState {
                    server: server.clone(),
                    filter: filter.clone(),
                    network: network.clone(),
                    networks,
                    networks_error,
                    ..Default::default()
                };
                v.room_preview = PreviewState::default();
            }
            v.directory.loading = true;
            v.directory.error = None;
        });
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.discovery_task = Some(self.runtime.spawn(async move {
            if reset {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            let networks = if fetch_networks {
                Some(account.directory_networks(server.clone()).await)
            } else {
                None
            };
            let result = account
                .public_rooms(DirectoryQuery {
                    server,
                    filter,
                    network,
                    since,
                    limit: 50,
                })
                .await;
            let Ok(state) = shared.lock() else {
                return;
            };
            if state.account.as_ref().map(Account::instance_id) != Some(instance)
                || state.discovery_request != request
            {
                return;
            }
            publish(&updates, |v| {
                v.directory.loading = false;
                if let Some(networks) = networks {
                    match networks {
                        Ok(n) => {
                            v.directory.networks = n;
                            v.directory.networks_error = None;
                        }
                        Err(e) => v.directory.networks_error = Some(e),
                    }
                }
                match result {
                    Ok(page) => {
                        for room in page.rooms {
                            if v.directory.rooms.len() < 1000
                                && !v.directory.rooms.iter().any(|r| r.room_id == room.room_id)
                            {
                                v.directory.rooms.push(room);
                            }
                        }
                        v.directory.next = page.next;
                        v.directory.exhausted =
                            v.directory.next.is_none() || v.directory.rooms.len() >= 1000;
                    }
                    Err(e) => v.directory.error = Some(e),
                }
            });
        }));
        Ok(())
    }
    pub(crate) fn preview_room(&self, target: RoomTarget) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = state
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        if let Some(task) = state.preview_task.take() {
            task.abort();
        }
        state.preview_request = state.preview_request.wrapping_add(1);
        let request = state.preview_request;
        let instance = account.instance_id();
        let shared = self.state.clone();
        let updates = self.updates.clone();
        publish(&updates, |v| {
            v.room_preview = PreviewState {
                loading: true,
                ..Default::default()
            }
        });
        state.preview_task = Some(self.runtime.spawn(async move {
            let result = account.preview_room(target).await;
            let Ok(state) = shared.lock() else {
                return;
            };
            if state.account.as_ref().map(Account::instance_id) != Some(instance)
                || state.preview_request != request
            {
                return;
            }
            publish(&updates, |v| {
                v.room_preview.loading = false;
                match result {
                    Ok(preview) => v.room_preview.preview = Some(preview),
                    Err(e) => v.room_preview.error = Some(e),
                }
            });
        }));
        Ok(())
    }
    pub(crate) fn reserve_directory_membership(
        &self,
        expected_account: u128,
    ) -> Result<MembershipTicket, ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(expected_account) {
            return Err(failure("Account changed."));
        }
        if let Some(task) = state.membership_task.take() {
            task.abort();
        }
        state.membership_request = state.membership_request.wrapping_add(1);
        Ok(MembershipTicket {
            account: expected_account,
            selection: state.selection,
            request: state.membership_request,
        })
    }
    pub(crate) fn membership_is_current(&self, ticket: MembershipTicket) -> bool {
        self.state
            .lock()
            .is_ok_and(|s| s.accepts_membership(ticket))
    }
    pub(crate) fn directory_membership(
        &self,
        ticket: MembershipTicket,
        target: RoomTarget,
        knock: bool,
        reason: Option<String>,
    ) -> Result<Option<(String, bool)>, ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = state
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        if !state.accepts_membership(ticket) {
            return Ok(None);
        }
        if let Some(task) = state.membership_task.take() {
            task.abort();
        }
        // Prefer room types the UI already loaded over another preview request.
        let known_space = {
            let view = self.updates.borrow();
            let id = target.id.as_str();
            view.room_preview
                .preview
                .iter()
                .map(|p| &p.room)
                .chain(view.directory.rooms.iter())
                .find(|r| r.room_id == id || r.alias.as_deref() == Some(id))
                .map(|r| r.is_space)
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let worker = account.clone();
        state.membership_task = Some(self.runtime.spawn(async move {
            let result = if knock {
                worker
                    .knock_room(target, reason)
                    .await
                    .map(|id| (id, false))
            } else {
                let space = match known_space {
                    Some(space) => space,
                    None => worker
                        .preview_room(target.clone())
                        .await
                        .is_ok_and(|p| p.room.is_space),
                };
                worker.join_room(target).await.map(|id| (id, space))
            };
            let _ = tx.send(result);
        }));
        drop(state);
        let result = self.runtime.block_on(rx);
        let state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if !state.accepts_membership(ticket) {
            return Ok(None);
        }
        let result = result.map_err(|_| failure("Room request cancelled."))?;
        result
            .map(|joined| (!knock).then_some(joined))
            .map_err(|e| {
                failure(match e {
                    DirectoryError::NotFound => "Room not found.",
                    DirectoryError::Forbidden => "Access denied.",
                    DirectoryError::InviteRequired => "An invitation is required.",
                    DirectoryError::KnockRequired => "Request an invitation first.",
                    _ => "Room request failed.",
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;
    #[test]
    fn membership_tickets_reject_superseded_admission_and_completion() {
        let ticket = MembershipTicket {
            account: 1,
            selection: 2,
            request: 3,
        };
        assert!(membership_matches(ticket, (Some(1), 2, 3)));
        for actual in [
            (None, 2, 3),
            (Some(9), 2, 3),
            (Some(1), 9, 3),
            (Some(1), 2, 9),
        ] {
            assert!(!membership_matches(ticket, actual));
        }
    }
    #[test]
    fn replacement_cancels_directory_and_preview_work() {
        let runtime = Runtime::new().unwrap();
        let mut state = State::default();
        let directory = runtime.spawn(std::future::pending());
        let preview = runtime.spawn(std::future::pending());
        let directory_abort = directory.abort_handle();
        let preview_abort = preview.abort_handle();
        state.discovery_task = Some(directory);
        state.preview_task = Some(preview);
        state.cancel_discovery();
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while !directory_abort.is_finished() || !preview_abort.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
        assert!(state.discovery_task.is_none());
        assert!(state.preview_task.is_none());
        assert_eq!(state.discovery_request, 1);
        assert_eq!(state.preview_request, 1);
    }
}
