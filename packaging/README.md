# Packaging groundwork

Unsigned, unpublished inputs for distribution packages (#52–#56). Nothing here
signs, uploads or publishes.

- `linux/com.twayblade.desktop.desktop` and `linux/com.twayblade.desktop.metainfo.xml`:
  desktop entry and AppStream data for the deb, rpm, AUR and AppImage packages.
  The icon (`com.twayblade.desktop`) is not drawn yet, and the `matrix:` URI
  handler is left out until the app accepts a URI on launch.
- The Nix package (`nix/package.nix`) writes its own desktop entry; align it
  with these files when the first native package is built.
- The macOS bundle comes from `macos/Info.plist` and `macos/build.sh`;
  `macos/make-dmg.sh` packs it into a compressed disk image with an
  Applications shortcut (`dist/Twayblade-<version>-<arch>.dmg`).
- `homebrew/twayblade.rb`: cask template for our own tap. `VERSION` and
  `SHA256` come from a release's DMG and `SHA256SUMS`. It assumes the release
  bundle identifier `com.twayblade.desktop` (development builds use
  `com.twayblade.desktop.development`) and can only ship once the DMG is
  Developer ID signed and notarised.
- Release flow: pushing a `v*` tag builds every platform and creates a **draft
  pre-release** with the zips, tarball, DMG, `SHA256SUMS` and notes drafted by
  `scripts/release-notes.py` from the PRs merged since the previous tag. A
  person reviews and rewords the notes, then publishes; stable is a separate
  promotion. Pushing a tag needs explicit authorization.

Still open: deb/rpm/AppImage build jobs, MSI/MSIX and winget, the tap
repository, signing and notarisation (needs credentials and explicit
authorization), promotion from pre-release to stable, and the in-app updater
download path.
