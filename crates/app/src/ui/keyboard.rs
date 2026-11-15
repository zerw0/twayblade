//! Composer keyboard actions: Markdown formatting and sent-message history.
use super::*;
use crate::composer_format::{self, Format};
use crate::shortcuts::{
    FormatBold, FormatCode, FormatItalic, FormatLink, FormatQuote, HistoryNext, HistoryPrevious,
};
use gpui_kit::component::native_menu::NativeMenu;

/// The composer's right-click menu: the usual edit items, then a Format submenu.
/// Items dispatch the same actions as the shortcuts, so the shortcuts show beside
/// them and everything goes through one path. The text field's own menu is
/// replaced by this one, so Cut, Copy, Paste and Select All are rebuilt here.
pub(crate) fn composer_context_menu(
    composer: Entity<gpui_kit::component::input::TextareaState>,
) -> impl Fn(NativeMenu, &mut Window, &mut App) -> NativeMenu + 'static {
    use gpui_kit::component::input::{Copy, Cut, Paste, SelectAll};
    move |menu, _, cx| {
        let selected = !composer.read(cx).selected_range().is_empty();
        let format = NativeMenu::new()
            .menu("Bold", Box::new(FormatBold))
            .menu("Italic", Box::new(FormatItalic))
            .menu("Code", Box::new(FormatCode))
            .menu("Link", Box::new(FormatLink))
            .menu("Quote", Box::new(FormatQuote));
        menu.menu_with_disabled("Cut", !selected, Box::new(Cut))
            .menu_with_disabled("Copy", !selected, Box::new(Copy))
            .menu("Paste", Box::new(Paste))
            .separator()
            .menu("Select All", Box::new(SelectAll))
            .separator()
            .submenu("Format", format)
    }
}

impl MatrixApp {
    /// Gives a composer panel the `Composer` key context and its actions.
    pub(crate) fn composer_keys(
        &self,
        panel: Stateful<Div>,
        in_thread: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        panel
            .key_context("Composer")
            .on_action(cx.listener(move |this, _: &FormatBold, window, cx| {
                this.format_composer(Format::Bold, in_thread, window, cx)
            }))
            .on_action(cx.listener(move |this, _: &FormatItalic, window, cx| {
                this.format_composer(Format::Italic, in_thread, window, cx)
            }))
            .on_action(cx.listener(move |this, _: &FormatCode, window, cx| {
                this.format_composer(Format::Code, in_thread, window, cx)
            }))
            .on_action(cx.listener(move |this, _: &FormatQuote, window, cx| {
                this.format_composer(Format::Quote, in_thread, window, cx)
            }))
            .on_action(cx.listener(move |this, _: &FormatLink, window, cx| {
                this.format_composer(Format::Link, in_thread, window, cx)
            }))
            .on_action(cx.listener(move |this, _: &HistoryPrevious, window, cx| {
                this.composer_history(true, in_thread, window, cx)
            }))
            .on_action(cx.listener(move |this, _: &HistoryNext, window, cx| {
                this.composer_history(false, in_thread, window, cx)
            }))
    }

    fn format_composer(
        &mut self,
        format: Format,
        in_thread: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(composer) = self.edit_composer(in_thread) {
            composer.update(cx, |state, cx| {
                composer_format::apply(format, state, window, cx);
                state.focus(window, cx);
            });
        }
    }

    /// Steps through what you sent in this room. Not while editing a message:
    /// the composer then holds that message, not a draft.
    fn composer_history(
        &mut self,
        previous: bool,
        in_thread: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit_session.is_some() {
            return;
        }
        let (Some(backend), Some(room), Some(composer)) = (
            self.backend.clone(),
            self.navigation.selected_room.clone(),
            self.edit_composer(in_thread),
        ) else {
            return;
        };
        let account = self.snapshot.account_instance;
        let result = if previous {
            let draft = composer.read(cx).value().to_string();
            backend.history_previous(account, &room, &draft)
        } else {
            backend.history_next(account, &room)
        };
        match result {
            Ok(Some(text)) => composer.update(cx, |state, cx| {
                let end = text.len();
                // Undoable, unlike set_value.
                state.replace_all(text, window, cx);
                state.set_selected_range(end..end, cx);
            }),
            Ok(None) => {}
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
            }
        }
    }
}
