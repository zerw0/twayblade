# Mentions, autocomplete Return, hover actions and link previews

## Built

- **Return in autocomplete.** The composer turns Return, ↑, ↓ and Escape into
  its own actions before any key listener runs, so Return on a highlighted
  `@` mention or `:shortcode` sent the message as well. The room and thread
  composers now capture those actions while a list is open and give them to
  the list. If the `:shortcode` list has no matches, Return still sends.
- **Mentions.** A mention link (`matrix.to/#/@user` or `matrix:u/…`) now shows
  as a bold `@Name`. Hovering it shows "Show Profile · @user:server" instead
  of the matrix.to URL. Clicking it opens the profile, as before.
- **Hover actions.** A click also focuses a message row, so its action bar
  stayed up after the pointer left. The bar now shows for the message under
  the pointer, or for the focused message only when the last input came from
  the keyboard.
- **Link previews** (Settings › Chat): "Show link previews" is on by default;
  "In encrypted rooms" is a separate switch and is off by default. The
  homeserver's `/preview_url` fetches the page, so the client never contacts
  the site, but the homeserver does see the link. Each message previews its
  first http(s) link: matrix.to links, code, quotes, notices and media are
  skipped. The card shows the site, title (2 lines), description (2 lines) and
  the `og:image` as a 56 pt thumbnail when the homeserver stored one as `mxc://`.
  At most 3 requests run at once, and only the newest 80 messages per timeline
  are considered. The cache holds about 300 entries and is cleared when the
  account changes. A failure shows nothing and is not retried during the
  session. Clicking the card opens the link in the browser.

## Verified

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`
  (also with `--features capture`), workspace tests and the offline UI tests.
- Offline UI tests: Return on a mention list picks it without emitting the
  composer's submit event (this test fails without the fix). Clicking a mention
  in a message opens that person's profile. Pointer focus alone does not keep
  the action bar up, but keyboard focus does. Unit tests cover link selection,
  mention Markdown and Open Graph parsing.

## Not verified

- Link previews against a live homeserver (Synapse/Conduit `/preview_url`,
  image thumbnails), and how card layout changes the scroll position on a long
  history.
- Physical macOS check of the new mention style, card styling in the
  bubble/IRC/compact styles, and dark mode.
- Linux and Windows.
