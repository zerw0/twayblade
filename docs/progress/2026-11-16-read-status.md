# Read status under the composer (#122)

## Behaviour

When nobody is typing, the line under the composer shows who has read up to
the latest message. It appears in groups and direct messages alike: a
double-check icon, then "Seen by Ana", "Seen by Ana and Bo", "Seen by Ana, Bo
and Cy", or "Seen by Ana, Bo, Cy and 2 others". Names come from the room's
member list. Before the list loads, the user's local part is used. Your own
receipt is never counted. Someone typing takes the line instead.

The source is the SDK's read receipts on the latest event (`read_by`). The SDK
places each member's receipt on the last event they have read, so the readers
of the latest message are exactly the people who are caught up.

## Verification

- `seen_by_line` unit test: one to three names, then "and N other(s)".
- `the_composer_shows_who_has_seen_the_latest_message`: a reader of the latest
  message is shown, and typing replaces the line.
- `cargo test -p twayblade --all-features --locked`: 187 passed. Clippy is
  clean.

## Not verified

- Live receipts on the physical Mac (groups and DMs), including how quickly
  the line updates.
- Linux and Windows.
