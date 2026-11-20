# Larger interface, current window controls and small fixes

- **Size.** 100% in Settings › Appearance now draws 1.2× the previous points
  (`theme::BASE_SCALE`): text, controls and spacing were sized for a compact
  panel and read as too small next to other chat apps. The default window is
  1240 × 820. Message hover actions use small (24 pt) buttons instead of
  extra-small, and reaction chips are 24 pt tall.
- **Window controls.** `macos/build.sh` and the Nix package record SDK 26.0 in
  the executable with `vtool` (deployment target stays 14.0) and sign it
  again, so macOS 26 draws its current, larger traffic lights; the pinned Nix
  SDK (14.4) got the small legacy ones. The vendored macOS window centres them
  in the titlebar whatever their size.
- **Update command.** The package-manager command in Settings › General is
  selectable and has a Copy button.
- **Seen by.** The line above the composer is left-aligned, and "Read" under
  your latest message is hidden when that line already covers it.
- **Emoji picker.** A click outside closes it; its own button still toggles.
- **Update checks** repeat every four hours instead of daily.

## Verification

- `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings`,
  `cargo test --workspace --all-features` (app 248 passed), including the new
  `a_click_outside_closes_the_emoji_picker`. UI tests keep the nominal scale.
- Debug bundle and `nix build .#default` checked on macOS 26 (Apple silicon):
  `otool` shows `sdk 26.0`, `minos 14.0`; both launch and show the larger,
  centred traffic lights.

## Not verified

- README screenshots predate the larger size.
- Linux and Windows at the new scale.
