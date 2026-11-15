# Dialog and panel focus

Dialog and panel focus (2026-11-15, `feature/camera-linux-windows`, issue #51).

Implemented:
- The create-room, direct-message, join-space and invite dialogs move keyboard
  focus into their text field when they open. The leave-room confirmation
  focuses the sheet itself, so Return alone never confirms leaving.
- Tab and Shift-Tab stay inside an open dialog (a focus trap on the sheet);
  before, Tab walked out into the window behind the backdrop.
- Closing a dialog (Close button, Escape, backdrop click, completed action) or
  a side panel (Close button, Escape, the Room info toggle) returns focus to
  the thread composer when a thread is open, else to the conversation
  composer, else to the window, instead of leaving it on a handle that is no
  longer drawn. The image viewer already returned focus to the image it
  opened from.
- `--preview-leave` opens the leave confirmation for captures.

Window tab order, checked by stepping Tab through the sample window in an
offline test: title bar, space rail, sidebar, room header, timeline message
actions, composer and its formatting buttons, then the side panel, wrapping
back to the title bar. Hover-only message actions become visible when they
take keyboard focus. Each message contributes up to three stops (reaction,
edit, reply in thread), so long histories are slow to cross by Tab; there is
no landmark jump yet.

Verified: compilation, strict clippy, offline UI tests (field focus on open,
Tab confinement, focus return after Close and Escape for a dialog and after
Escape for the details panel), and a capture of the leave dialog on macOS.
Not verified: VoiceOver or any other screen reader, keyboard-only use on a
physical Mac with Full Keyboard Access, Linux and Windows. #51 stays open.
