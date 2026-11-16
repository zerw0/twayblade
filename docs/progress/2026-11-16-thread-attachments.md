# Attachments in thread replies (#118)

## Behaviour

The thread reply field has an Attach File button, like the main composer.
Files can also be pasted, or dropped on the thread panel. They stack in the
reply field and go out with the next reply: attachments first, in order, then
the text. If sending stops, the unsent attachments go back into the field.

Each composer has its own stack (`AttachmentStack`): one for the main composer
and one in `ThreadPanel`. A file added in a thread never goes to the main room.
The stack is dropped when the thread closes.

Attachments are sent through the thread's timeline (`ThreadAction::Attach`).
For a thread-focused timeline, the SDK infers the thread relation, so they
arrive as thread replies.

## Verification

- Compile: `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- Offline UI tests: `cargo test -p twayblade --all-features --locked`.
  `a_thread_reply_has_its_own_attachments` shows that a file being added in
  the thread appears in the thread's tray and not in the main composer.

## Not verified

- Sending to a live homeserver, and checking that Element shows the file inside
  the thread.
- Pasting and dropping files on the physical Mac.
- Linux and Windows.
