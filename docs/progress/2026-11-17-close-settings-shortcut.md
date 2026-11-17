# Close Settings with Cmd-W (#171)

## Built

- New `CloseWindow` action, bound to **Cmd-W** on macOS and **Ctrl-W**
  elsewhere, listed in Settings › Keyboard as "Close Settings".
- The Settings window handles it like its close button: it closes unless a
  new recovery key is on screen, which must be saved first. The button and
  the shortcut share one rule (`release_settings`).
- The Settings window now takes keyboard focus when it opens, so the
  shortcut works before any control has been clicked.
- In the main window Cmd-W does nothing; it does not close the app.

## Verified

- Compile: strict workspace Clippy. Offline tests: a UI test dispatches the
  action with a recovery key shown (Settings stays) and without (it closes).

## Not verified

- Physical key press on a Mac, Linux or Windows build.
