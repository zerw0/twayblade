#![allow(
    dead_code,
    reason = "Visible presence-set contracts for the separate UI implementation"
)]
use super::*;
use matrix_client_core::presence::State as PresenceState;
impl State {
    pub(super) fn cancel_presence(&mut self) {
        if let Some(task) = self.presence_task.take() {
            task.abort();
        }
        self.presence_request = self.presence_request.wrapping_add(1);
        self.presence_visible.clear();
    }
}
fn project(view: &mut Snapshot) {
    for member in &mut view.members {
        member.presence = view.presence.get(&member.user_id).cloned();
    }
    for room in &mut view.rooms {
        room.presence = room
            .partner
            .as_ref()
            .and_then(|user| view.presence.get(user))
            .cloned();
    }
}
impl Backend {
    /// The UI supplies only DM partners/member rows it currently displays.
    pub(crate) fn observe_presence(&self, users: Vec<String>) -> Result<(), ServiceError> {
        if users.len() > 64 {
            return Err(failure("Too many visible members."));
        }
        let users: Vec<_> = users
            .into_iter()
            .map(|id| matrix_client_core::presence::parse_user(&id))
            .collect::<Option<_>>()
            .ok_or_else(|| failure("Invalid member."))?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = state
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        state.cancel_presence();
        state.presence_visible = users.iter().map(ToString::to_string).collect();
        let request = state.presence_request;
        let instance = account.instance_id();
        publish(&self.updates, |view| {
            view.presence
                .retain(|id, _| state.presence_visible.contains(id));
            project(view);
        });
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.presence_task = Some(self.runtime.spawn(async move {
            let stream = account.presence_updates(users);
            tokio::pin!(stream);
            while let Some(result) = stream.next().await {
                let Ok(state) = shared.lock() else {
                    return;
                };
                if state.account.as_ref().map(Account::instance_id) != Some(instance)
                    || state.presence_request != request
                {
                    return;
                }
                publish(&updates, |view| {
                    match result {
                        Ok(snapshot) => {
                            view.presence_enabled = snapshot.enabled;
                            view.own_presence = snapshot.own;
                            view.presence = snapshot.users;
                        }
                        Err(_) => {
                            view.presence_enabled = false;
                            view.own_presence = None;
                            view.presence.clear();
                        }
                    }
                    project(view);
                });
            }
        }));
        Ok(())
    }
    pub(crate) fn set_presence(
        &self,
        presence: PresenceState,
        status: Option<String>,
    ) -> Result<(), ServiceError> {
        let (account, visible, request) = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session is unavailable."))?;
            (
                state
                    .account
                    .clone()
                    .ok_or_else(|| failure("Sign in first."))?,
                state.presence_visible.clone(),
                state.presence_request,
            )
        };
        self.runtime
            .block_on(account.set_presence(presence, status))
            .map_err(|_| failure("Status could not be changed."))?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(account.instance_id()) {
            return Err(failure("Account changed."));
        }
        if state.presence_request != request {
            return Ok(());
        }
        drop(state);
        self.observe_presence(visible)
    }
}
