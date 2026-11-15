# Keyboard areas, one tab stop per timeline, Archive focus (#51)

## Built

- **F6 / ⇧F6 move between areas** on every platform: sidebar → messages →
  message field → thread messages → thread message field, wrapping. Only
  areas that are shown count. F6 does nothing while a dialog, the image viewer
  or a full-window call is open. In the sidebar, F6 lands on the open
  conversation's row, or on conversation search when that row is hidden or a
  search is typed. In a timeline it lands on the message last moved to, or on
  the latest one. Both shortcuts appear in Settings › Keyboard.
- **Each timeline is one Tab stop.** Only the current message's row is in the
  Tab order. ↑/↓ move between messages, Home/End jump to the first/last loaded
  one, and each move scrolls the message into view. Tab then reaches that
  message's own controls (react, edit, reply in thread, reactions, thread
  link, image, save/open, retry/cancel) and leaves the timeline. Before, every
  message added 2–8 stops. Its hover actions stay visible while it has
  keyboard focus. Clicking a message makes it the current one. The thread
  root's controls stay reachable in the thread panel.
- A message row shows the selection tint only for keyboard focus
  (focus-visible), not on click.
- **Archive from the avatar menu** scrolls the sidebar to the Archive section
  and focuses its first room, so keyboard and VoiceOver users land there. The
  menu does not take focus back.
- Kit's Button sets the tab-stop flag on the focus handle it is given (default
  on). Controls that share a handle must set `.tab_stop` on the Button too.
- `--preview-message-focus` opens the preview with a focused message, for
  captures.

## Verified

- Compile: `cargo fmt --check`, `cargo clippy --workspace --all-targets
  --all-features --locked -D warnings` (macOS host).
- Offline UI tests (158 pass):
  - `f6_moves_between_areas_and_each_timeline_is_one_tab_stop`: F6 order and
    wrap, ↑ and Home, ⇧F6 back to the remembered message, and Tab through one
    message's controls and out with no other message's control on the way.
  - `opening_the_archive_scrolls_a_long_sidebar_to_it`: 60 extra rooms. It
    fails with the scroll removed.
  - `archive_moves_a_room_out_of_its_section_and_lists_it_from_home` now also
    checks the focus.
  - Unit tests for stepping, group lookup and area cycling.
- Offline captures (macOS): a focused message shows its action bar, and the
  Archive row is focused with its ring.

## Not verified

- VoiceOver, Full Keyboard Access, Orca and Narrator runs. Needs the user.
- The focus-visible tint on a row was not captured: the capture moves focus
  programmatically, so it is not keyboard focus.
- On Mac keyboards whose top row defaults to media keys, F6 needs Fn. Whether
  that is acceptable or needs a second binding is open.
- Live account, local Synapse, Element: not relevant to this change, not run.
- Linux and Windows: only the shared lint/test gates. Not built or run there.
