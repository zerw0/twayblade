//! Editing a sent message in the composer: Edit in the message actions, ↑ in an
//! empty composer, ↑/↓ to step between your messages, Return saves, Escape
//! cancels. The draft you were writing is kept aside and comes back afterwards.
use crate::app::MatrixApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

/// The composer side of an edit. The backend owns the edit itself
/// (`Snapshot.editing`, `edit_saving`, `edit_error`).
pub(crate) struct EditSession {
    pub event_id: String,
    /// Edited in the open thread's composer rather than the main one.
    pub in_thread: bool,
    /// The composer text before editing started, restored afterwards.
    pub draft: String,
    /// The message text has been loaded into the composer.
    pub loaded: bool,
    /// Save was requested; success is the backend clearing `editing`.
    pub saving: bool,
}

impl MatrixApp {
    pub(crate) fn edit_composer(&self, in_thread: bool) -> Option<Entity<TextareaState>> {
        if in_thread {
            self.thread.as_ref().map(|t| t.composer.clone())
        } else {
            Some(self.composer.clone())
        }
    }

    /// Our editable messages on one surface, oldest first.
    fn editable_ids(&self, in_thread: bool) -> Vec<String> {
        let messages = if in_thread {
            match &self.thread {
                Some(thread) => &thread.messages,
                None => return Vec::new(),
            }
        } else {
            &self.snapshot.messages
        };
        messages
            .iter()
            .filter(|m| m.editable)
            .filter_map(|m| m.event_id.clone())
            .collect()
    }

    pub(crate) fn start_edit(
        &mut self,
        event_id: String,
        in_thread: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(composer) = self.edit_composer(in_thread) else {
            return;
        };
        // Stepping from one edit to the next keeps the original draft.
        let draft = match self.edit_session.take() {
            Some(session) => session.draft,
            None => composer.read(cx).value().to_string(),
        };
        if self.preview {
            // Sample data has no backend: load the shown text directly.
            let body = self
                .snapshot
                .messages
                .iter()
                .find(|m| m.event_id.as_deref() == Some(event_id.as_str()))
                .map(|m| m.body.clone())
                .unwrap_or_default();
            composer.update(cx, |state, cx| {
                state.set_value(body, window, cx);
                state.focus(window, cx);
            });
            self.edit_session = Some(EditSession {
                event_id,
                in_thread,
                draft,
                loaded: true,
                saving: false,
            });
            cx.notify();
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };
        self.edit_session = Some(EditSession {
            event_id: event_id.clone(),
            in_thread,
            draft,
            loaded: false,
            saving: false,
        });
        if let Err(error) = backend.begin_edit(event_id) {
            self.status = error.to_string();
            self.edit_session = None;
        }
        cx.notify();
    }

    /// Leave editing and put the earlier draft back.
    pub(crate) fn end_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.edit_session.take() else {
            return;
        };
        if let Some(backend) = &self.backend {
            backend.cancel_edit();
        }
        if let Some(composer) = self.edit_composer(session.in_thread) {
            composer.update(cx, |state, cx| {
                state.set_value(session.draft, window, cx);
                state.focus(window, cx);
            });
        }
        cx.notify();
    }

    /// Return in a composer that is editing: save instead of sending.
    pub(crate) fn save_edit_from_composer(
        &mut self,
        in_thread: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((loaded, saving)) = self
            .edit_session
            .as_ref()
            .filter(|session| session.in_thread == in_thread)
            .map(|session| (session.loaded, session.saving))
        else {
            return false;
        };
        if !loaded || saving {
            return true;
        }
        if self.preview {
            self.end_edit(window, cx);
            return true;
        }
        let Some(composer) = self.edit_composer(in_thread) else {
            return true;
        };
        let text = composer.read(cx).value().to_string();
        let result = match &self.backend {
            Some(backend) => backend.save_edit(text),
            None => return true,
        };
        match result {
            Ok(()) => {
                if let Some(session) = &mut self.edit_session {
                    session.saving = true;
                }
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
        true
    }

    /// Follow the backend: load the text once it arrives, finish after a save,
    /// and drop the session if the backend abandoned it.
    pub(crate) fn sync_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((composer, draft)) = self.edit_restore.take() {
            composer.update(cx, |state, cx| state.set_value(draft, window, cx));
        }
        if self.preview {
            return;
        }
        let Some((event_id, in_thread, loaded, saving)) = self
            .edit_session
            .as_ref()
            .map(|s| (s.event_id.clone(), s.in_thread, s.loaded, s.saving))
        else {
            return;
        };
        let original = self
            .snapshot
            .editing
            .as_ref()
            .filter(|e| e.event_id == event_id)
            .map(|e| e.original.clone());
        let failed = self.snapshot.edit_error.is_some();
        if !loaded {
            if let Some(text) = original {
                if let Some(session) = &mut self.edit_session {
                    session.loaded = true;
                }
                if let Some(composer) = self.edit_composer(in_thread) {
                    composer.update(cx, |state, cx| {
                        state.set_value(text, window, cx);
                        state.focus(window, cx);
                    });
                }
            } else if failed {
                // The message could not be opened for editing.
                self.edit_session = None;
            }
            return;
        }
        if saving && !self.snapshot.edit_saving {
            if failed {
                // Keep the edited text so the user can retry.
                if let Some(session) = &mut self.edit_session {
                    session.saving = false;
                }
            } else if original.is_none()
                && let Some(session) = self.edit_session.take()
                && let Some(composer) = self.edit_composer(in_thread)
            {
                composer.update(cx, |state, cx| state.set_value(session.draft, window, cx));
            }
        }
    }

    /// ↑ in an empty composer edits your last message; while editing, ↑ at the
    /// start and ↓ at the end step between your messages. Returns true when
    /// the key was used.
    pub(crate) fn edit_navigation_key(
        &mut self,
        key: &str,
        in_thread: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(key, "up" | "down") {
            return false;
        }
        let Some(composer) = self.edit_composer(in_thread) else {
            return false;
        };
        let (value, cursor) = {
            let state = composer.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let ids = self.editable_ids(in_thread);
        match &self.edit_session {
            None if key == "up" && value.is_empty() => {
                let Some(last) = ids.last().cloned() else {
                    return false;
                };
                self.start_edit(last, in_thread, window, cx);
                true
            }
            Some(session) if session.in_thread == in_thread && !session.saving => {
                let position = ids.iter().position(|id| *id == session.event_id);
                if key == "up" && cursor == 0 {
                    let Some(previous) = position
                        .and_then(|i| i.checked_sub(1))
                        .and_then(|i| ids.get(i))
                        .cloned()
                    else {
                        return false;
                    };
                    self.start_edit(previous, in_thread, window, cx);
                    true
                } else if key == "down" && cursor == value.len() {
                    match position.and_then(|i| ids.get(i + 1)).cloned() {
                        Some(next) => self.start_edit(next, in_thread, window, cx),
                        // Past your newest message: back to the draft.
                        None => self.end_edit(window, cx),
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// The bar above a composer that is editing.
    pub(crate) fn edit_bar(&self, in_thread: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        use gpui_kit::component::button::{Button, ButtonVariants};
        use gpui_kit::component::*;
        let session = self
            .edit_session
            .as_ref()
            .filter(|s| s.in_thread == in_thread)?;
        let muted = cx.theme().muted_foreground;
        let status = if !session.loaded {
            "Loading message…".to_owned()
        } else if session.saving {
            "Saving…".to_owned()
        } else if let Some(error) = &self.snapshot.edit_error {
            error.clone()
        } else {
            "Editing message · Return to save, Escape to cancel".to_owned()
        };
        let error = self.snapshot.edit_error.is_some() && !session.saving && session.loaded;
        Some(
            h_flex()
                .id(if in_thread {
                    "thread-edit-bar"
                } else {
                    "edit-bar"
                })
                .px(px(12.))
                .pt(px(8.))
                .gap(px(8.))
                .items_center()
                .text_size(px(12.))
                .child(
                    Icon::new(gpui_kit::assets::IconName::Pencil)
                        .size(px(13.))
                        .text_color(cx.theme().primary),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if error { cx.theme().danger } else { muted })
                        .child(status),
                )
                .child(
                    Button::new(if in_thread {
                        "thread-edit-cancel"
                    } else {
                        "edit-cancel"
                    })
                    .ghost()
                    .xsmall()
                    .label("Cancel")
                    .accessibility_label("Cancel editing")
                    .on_click(cx.listener(|this, _, window, cx| this.end_edit(window, cx))),
                )
                .test_support()
                .into_any_element(),
        )
    }
}
