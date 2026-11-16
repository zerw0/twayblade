# Attachments in thread replies (#118)

## Behaviour

The thread reply field has an Attach File button, like the main composer.
Files can also be pasted, or dropped on the thread panel. They stack in the
reply field and go out with the next reply: attachments first, in order, then
the text. If sending stops, the unsent attachments go back into the field.

Each composer has its own stack (`AttachmentStack`): one for the main composer
and one in `ThreadPanel`. A file added in a thread never goes to the main room.
The stack is dropped when the thread closes.

Attachments go through the thread's timeline (`ThreadAction::Attach`). Core
used to call the room's send queue directly, so the thread relation was never
added, and live use showed a thread image landing in the main room. Core now
sends every attachment with `Timeline::send_attachment(...).use_send_queue()`.
A thread-focused timeline adds the thread relation, and the main timeline
sends exactly as before.

## Verification

- Compile: `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- Offline UI tests: `cargo test -p twayblade --all-features --locked`.
  `a_thread_reply_has_its_own_attachments` shows that a file being added in
  the thread appears in the thread's tray and not in the main composer.

- Local Synapse: the `threads` profile now sends a file from the thread. Bob
  sees it in the thread, the root's reply count includes it, and the main room
  does not show it. The `media` profile (plain and encrypted attachments in the
  main room, cancel and restart) still passes.

## Not verified

- The live account after the fix, and how Element shows the file in the thread.
- Pasting and dropping files on the physical Mac.
- Linux and Windows.
