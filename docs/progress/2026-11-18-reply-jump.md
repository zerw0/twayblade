# Jump to a reply's original message

Clicking the quote above a reply goes to the message it answers.

- `core`: `MessageRow::reply_to` carries the replied-to event ID (from the SDK's
  `in_reply_to`), also when the original's details are still loading.
- If the original is in the loaded timeline (the thread's, then the main one),
  it scrolls into view, takes the message cursor and keyboard focus, and is
  tinted like an opened search result.
- Otherwise the room reopens around the original (the search-result path);
  Back returns.
- The quote shows a pointer, a hover state and the tooltip "Go to Original
  Message". A quote whose original could not be loaded is not clickable.

## Verification

- `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings`.
- Offline UI test `clicking_a_reply_quote_reveals_the_original` (preview fixture).

## Not verified

- Live homeserver: jumping to an original outside the loaded history.
- Physical Linux/Windows.
