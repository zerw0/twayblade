# Formatting in the composer's right-click menu

Requested 2026-11-15: the composer's formatting actions as a right-click option
instead of a row of buttons (this replaces the toolbar from #69, issue #39).

- Right-clicking the message field (and the thread reply field) shows Cut, Copy
  (disabled with nothing selected), Paste, Select All, then a **Format** submenu:
  Bold, Italic, Code, Link, Quote. The text field's own menu is replaced, so the
  edit items are rebuilt in `composer_context_menu` (`ui/keyboard.rs`).
- Items dispatch the same actions as the shortcuts, so the shortcuts show beside them
  on macOS and there is one code path. The toolbar buttons are gone; the composer
  row keeps attach, emoji and send.

Verified: compilation, strict Clippy, 148 offline tests (the shortcut actions are
covered by `formatting_shortcuts_reach_main_and_thread_composers`). Not verified:
the menu itself. It is a native OS menu, so offline tests and captures cannot open
it; it needs a look on a real Mac (and Linux/Windows, which draw their own
fallbacks). Still missing from #39: a list option.
