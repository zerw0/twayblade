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

## Test pre-release v0.1.0-rc.1 (#55)

Tagged on `main` after #101. The release workflow built every package and
created a draft pre-release with nine assets. Checks on the draft:
- `SHA256SUMS` matches the downloaded DMG, MSI and AppImage.
- The DMG mounts with `Twayblade.app` and an Applications link. The app is
  ad hoc signed, `codesign --verify --deep --strict` passes, and Gatekeeper
  (`spctl`) rejects it as expected.

Fixed here after the draft:
- The DMG was named from the bundle version (`Twayblade-0.1.0-arm64.dmg`)
  rather than the tag, so the cask script could not find it. It is now named
  after the tag, like the other packages.
- GitHub renames `~` to `.` in asset names, so the deb and rpm lines in
  `SHA256SUMS` did not match the downloadable files. The release job now
  renames the files before computing checksums.
- The distributed app carried the development identifier
  (`com.twayblade.desktop.development`). CI now builds it as
  `com.twayblade.desktop`; local builds keep the development identifier.

The rc.1 draft has the old names and identifier and stays unpublished. A
second test tag after this change can confirm the fixes. The draft is not
published: draft assets are not public, so the tap and the in-app download
cannot use it until a person publishes a pre-release.
