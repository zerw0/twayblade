# DMG, cask template and draft pre-releases

DMG, cask template and draft pre-releases (2026-11-15,
`feature/camera-linux-windows`, issues #52 and #55).

Implemented:
- `macos/make-dmg.sh` packs a built `Twayblade.app` into a compressed,
  verified disk image with an Applications shortcut, keeping the bundle's
  signature. It does not sign or notarise.
- `packaging/homebrew/twayblade.rb`, a cask template for our own tap (version
  and checksum placeholders, Apple Silicon, macOS 14+, zap of the app's data
  folder and preferences; Keychain sessions are left alone).
- `build.yml`: the macOS job also uploads the DMG. A `v*` tag now creates a
  draft pre-release (before: a published release with GitHub's generated
  notes) whose notes come from `scripts/release-notes.py`, run against the PRs
  merged since the previous tag, plus `SHA256SUMS`. Nothing is public until a
  person publishes the draft.

Verified: the DMG script on this Mac against the debug bundle (image created,
`hdiutil verify` passes, mounted contents are the app and the Applications
link, code signature intact); the cask with `ruby -c` and `brew style`; the
workflow with actionlint (one pre-existing shellcheck note on the checksum
line). Not verified: the workflow on GitHub (it runs on main pushes and tags,
not on pull requests), the DMG built from a release build in CI, installing
from the cask, Gatekeeper on an unsigned image, and anything about signing.
No tag was pushed and nothing was published.
