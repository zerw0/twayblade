# Reply to a message (#126)

## Behaviour

Messages in the main timeline have a Reply action: a curved-arrow button in
the hover bar, and Reply in the right-click menu. A bar above the composer
then shows "Replying to <name>" and the first line of the message (or its file
name), with a × button. Escape also cancels the reply. The next text message
is sent as a reply. Attachments sent alone are not replies. Replying and
editing share the composer, so starting one ends the other. The reply belongs
to its room, so another room's composer does not show it.

Core sends with `Timeline::send_reply`. The SDK adds the reply relation and
the quoted sender's mention, and keeps a reply in a thread inside that thread.

## Verification

- Local Synapse (`threads` profile): Bob sends a quoted reply to the thread
  root from the main timeline, and his timeline shows it with the root as its
  ready reply target.
- UI test `reply_names_the_message_and_can_be_cancelled` and the
  `reply_excerpt` unit test. `cargo test -p twayblade --all-features --locked`:
  187 passed. Clippy is clean.

## Not verified

- Live account, and how Element shows the reply.
- Linux and Windows.
