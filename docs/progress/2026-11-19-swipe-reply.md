# Two-finger swipe to reply

On a trackpad, swiping right with two fingers over a message in the main
timeline replies to it, as in Messages and WhatsApp. The message follows the
fingers and a reply arrow appears in the gap; past 64 pt the arrow takes the
accent and letting go starts the reply. The message then slides back (180 ms
ease-out; none under Reduce Motion). Only precise scrolls count, the
direction is decided once per gesture (so vertical scrolling is untouched),
and momentum after the fingers lift is ignored. It is offered where Reply is:
main timeline messages with an event ID, not thread rows or membership lines.

The timeline's elastic scrolling claims every wheel event in the capture
phase, so the gesture is read by an invisible layer registered before the
timeline paints (`swipe_reply::swipe_capture`), which stops propagation only
once a gesture is horizontal.

## Verification

- `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings`,
  `cargo test -p twayblade --all-features` (247 passed), including
  `two_finger_swipe_right_replies_and_scrolls_do_not` (short swipe, vertical
  scroll, and a reply swipe with synthetic trackpad events).

## Not verified

- A physical trackpad (only synthetic events); Linux and Windows touchpads,
  whose scroll phases differ.
