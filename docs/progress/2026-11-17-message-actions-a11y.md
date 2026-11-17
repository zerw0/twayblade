# Message actions out of the screen-reader path (#156)

## Built

- A message's action buttons (Add reaction, Reply, Edit, Reply in Thread)
  are rendered only for the message under the pointer or the message with
  keyboard focus (the row or one of its buttons). Every other message keeps
  a same-size empty space, so nothing shifts when the buttons appear.
- VoiceOver therefore no longer steps through four buttons per message;
  reading a message gives its text, and the actions are there once it is
  focused. The message's context menu keeps all actions too.
- Interim: true screen-reader custom actions (VO-Cmd-Space) need custom
  action support in AccessKit's macOS, Windows and AT-SPI adapters, which
  0.26/0.34/0.19 lack (comment on #156).

## Verified

- Compile: strict workspace Clippy. Offline tests: a UI test (no action
  buttons at rest; only the hovered message has them; keyboard focus on
  another message moves them there); existing reaction, reply, edit and
  style tests now hover the message first, as a person would.
- Offline capture at rest: layout unchanged.

## Not verified

- VoiceOver on a physical Mac (depends on #166's fix for focus).
