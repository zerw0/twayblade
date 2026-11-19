# Screenshots and the sign-in logo

- The sign-in screen shows the app icon (64 pt, `crates/app/assets/app-icon.png`,
  192 px) instead of a generic chat glyph in an accent circle.
- `--preview-showcase` (with the other `--preview-*` flags) replaces the
  preview's test-oriented sample (undecryptable and failed messages, notes
  about sample data) with a plain team conversation in #General: a thread,
  a quoted reply, reactions, a file, an image (`--preview-showcase-image <png>`)
  and read receipts, for the account "Jordan Lee". The "Sample data" notes on
  sign-in, Settings, threads and the composer are hidden in this mode only.
- `scripts/screenshots.sh` takes real macOS window captures (title bar and
  shadow) of the showcase into `docs/screenshots/`; the README shows them in
  a grid and says they are sample data.

## Verification

- `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings`,
  `cargo test -p twayblade --all-features` (246 passed).
- `scripts/screenshots.sh` run on macOS; all eleven captures checked by eye.

## Not verified

- PNGs with a Display P3 profile show as the placeholder in the timeline;
  the script strips the profile. Not investigated further.
- Linux/Windows look of the sign-in logo.
