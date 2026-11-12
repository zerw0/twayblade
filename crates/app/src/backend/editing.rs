#![allow(
    dead_code,
    reason = "Message editing contracts for Claude's composer implementation"
)]
use super::*;
use twayblade_core::conversation::EditError;

/// Sources contain private message text: never log or Debug this state.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Editing {
    pub event_id: String,
    pub thread_root: Option<String>,
    /// Latest plain body, preserving this client's Markdown. Other clients'
    /// HTML-only formatting is lost when the changed source is saved.
    pub original: String,
}
impl State {
    pub(crate) fn cancel_edit(&mut self) {
        self.edit_request = self.edit_request.wrapping_add(1);
        if let Some(task) = self.edit_task.take() {
            task.abort();
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Ticket {
    account: Option<u128>,
    epoch: u64,
    selection: u64,
    request: u64,
    thread: u64,
}
impl Ticket {
    fn current(state: &State) -> Self {
        Self {
            account: state.account.as_ref().map(Account::instance_id),
            epoch: state.epoch,
            selection: state.selection,
            request: state.edit_request,
            thread: state.thread_request,
        }
    }
}
fn complete(
    view: &mut Snapshot,
    event_id: String,
    root: Option<String>,
    result: Result<Option<String>, EditError>,
) {
    view.edit_saving = false;
    match result {
        Ok(Some(original)) => {
            view.editing = Some(Editing {
                event_id,
                thread_root: root,
                original,
            })
        }
        Ok(None) => view.editing = None,
        Err(error) => view.edit_error = Some(wording(error).into()),
    }
}
fn wording(error: EditError) -> &'static str {
    match error {
        EditError::NotEditable => "This message can no longer be edited.",
        EditError::Unchanged => "The message has not changed.",
        EditError::TooLong => "This message is too long. Shorten it and try again.",
        EditError::Empty => "A message can't be empty.",
        EditError::Failed => "The edit could not be saved. Try again.",
    }
}
impl Backend {
    pub(crate) fn cancel_edit(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.cancel_edit();
            publish(&self.updates, |view| {
                view.editing = None;
                view.edit_error = None;
                view.edit_saving = false;
            });
        }
    }
    pub(crate) fn begin_edit(&self, event_id: String) -> Result<(), ServiceError> {
        self.edit_operation(Some(event_id), None)
    }
    pub(crate) fn save_edit(&self, text: String) -> Result<(), ServiceError> {
        self.edit_operation(None, Some(text))
    }
    fn edit_operation(
        &self,
        event: Option<String>,
        text: Option<String>,
    ) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let view = self.updates.borrow().clone();
        if view.edit_saving && event.is_none() {
            return Err(failure("The edit is still being saved."));
        }
        let saving = text.is_some();
        let event_id = event
            .or_else(|| view.editing.as_ref().map(|e| e.event_id.clone()))
            .ok_or_else(|| failure("Choose a message to edit."))?;
        let thread = view.thread.as_ref().filter(|t| {
            t.root != event_id
                && t.messages
                    .iter()
                    .any(|m| m.event_id.as_ref() == Some(&event_id))
        });
        let root = thread.map(|t| t.root.clone());
        let conversation = if thread.is_some() {
            state.thread_conversation.clone()
        } else {
            state.conversation.clone()
        }
        .ok_or_else(|| failure("Conversation is still loading."))?;
        let _account = state
            .account
            .as_ref()
            .map(Account::instance_id)
            .ok_or_else(|| failure("Sign in first."))?;
        state.cancel_edit();
        let ticket = Ticket::current(&state);
        publish(&self.updates, |view| {
            view.edit_error = None;
            view.edit_saving = saving;
            if !saving {
                view.editing = None;
            }
        });
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.edit_task = Some(self.runtime.spawn(async move {
            let result = match text {
                Some(text) => conversation.edit(&event_id, text).await.map(|_| None),
                None => conversation.edit_source(&event_id).await.map(Some),
            };
            let Ok(mut state) = shared.lock() else {
                return;
            };
            if Ticket::current(&state) != ticket {
                return;
            }
            state.edit_task = None;
            publish(&updates, |view| complete(view, event_id, root, result));
        }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unauthenticated_edit_commands_cannot_publish_sources() {
        let backend = Backend::with_lock(None).unwrap();
        assert!(backend.begin_edit("$missing:example.org".into()).is_err());
        assert!(backend.save_edit("private draft".into()).is_err());
        assert!(backend.snapshot().editing.is_none());
        assert_eq!(backend.snapshot().revision, 0);
    }
    #[test]
    fn cancel_invalidates_every_outstanding_request() {
        let mut state = State::default();
        let ticket = state.edit_request;
        state.cancel_edit();
        assert_ne!(ticket, state.edit_request);
        assert!(state.edit_task.is_none());
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    #[test]
    fn failures_and_unchanged_keep_the_source_for_retry() {
        let edit = Editing {
            event_id: "$event".into(),
            thread_root: Some("$root".into()),
            original: "private source".into(),
        };
        let mut view = Snapshot {
            editing: Some(edit.clone()),
            edit_saving: true,
            ..Default::default()
        };
        for error in [
            EditError::Failed,
            EditError::Unchanged,
            EditError::NotEditable,
            EditError::TooLong,
        ] {
            complete(&mut view, "$event".into(), None, Err(error));
            assert!(view.editing.as_ref() == Some(&edit));
            assert!(!view.edit_saving && view.edit_error.is_some());
        }
        complete(&mut view, "$event".into(), None, Ok(None));
        assert!(view.editing.is_none());
    }
    #[test]
    fn tickets_reject_account_selection_thread_and_request_changes() {
        let state = State::default();
        let ticket = Ticket::current(&state);
        assert!(Ticket::current(&state) == ticket);
        for changed in [
            Ticket {
                account: Some(1),
                ..ticket
            },
            Ticket { epoch: 1, ..ticket },
            Ticket {
                selection: 1,
                ..ticket
            },
            Ticket {
                request: 1,
                ..ticket
            },
            Ticket {
                thread: 1,
                ..ticket
            },
        ] {
            assert!(changed != ticket);
        }
    }
}
