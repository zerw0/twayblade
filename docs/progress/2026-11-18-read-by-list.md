# Who has read a message

Clicking "Seen by …" above the composer, or "Read" under your latest message
(all chat styles), opens a popover listing the readers: "Seen by N", then an
avatar and name per reader, scrolling past 280 pt. Clicking a reader opens
their profile. Names come from the member list, or the user ID's local part
before it loads.

## Verification

- `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings`.
- `cargo test -p twayblade --all-features` (246 passed); the seen-by test now
  clicks the line and finds the reader row.

## Not verified

- No capture of the open popover; not checked against a live homeserver.
- Physical Linux/Windows.
