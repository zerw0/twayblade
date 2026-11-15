# Update notice in the title bar; Homebrew tap cask (#56, #52)

## Built

- **Update notice (#56).** While a newer release is available, the title bar
  shows a small "Update" button with an accent dot, before the bell. It opens
  Settings › General, which has the notes, the package-manager command or
  the checksum-verified download. Nothing is shown when up to date. Its
  accessible name is "Twayblade X is available. Open update settings".
  Preview: `--preview-update-notice`.
- **Homebrew tap cask (#52).**
  - `packaging/homebrew/twayblade.rb` is for our own tap with the ad hoc
    signed DMG. A caveat explains Open Anyway on first launch; the cask does
    not remove quarantine.
  - `scripts/homebrew-cask.py` fills the version and the arm64 DMG's SHA-256
    from a release's `SHA256SUMS`. It refuses non-release versions, a missing
    DMG and malformed hashes.
  - The tap repository is not created: that publishes, and needs your
    authorization.

## Verified

- Compile: `cargo fmt --check` and strict Clippy pass (macOS host).
- Offline UI test: no notice while up to date; with an update, the notice
  appears and opens Settings.
- `python3 scripts/test_homebrew_cask.py`: 3 tests pass. `ruby -c` accepts
  the cask.
- Capture of the title bar with the notice reviewed.
- CI: `check.yml` now runs the release-notes and Homebrew cask script tests on
  Linux (not yet seen running on GitHub).

## Not verified

- The tap itself: no repository, no `brew install` from it, no real release
  to fill it from.
- `brew audit` and `brew style`: Homebrew is not installed here.
