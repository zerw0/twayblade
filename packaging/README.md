# Packaging groundwork

Unsigned, unpublished inputs for distribution packages (#52–#56). Nothing here
signs, uploads or publishes.

- `icons/com.twayblade.desktop.svg`: the app icon (source). `icons/render.sh`
  regenerates the 512 px PNG and the Windows `twayblade.ico` from it. The
  `.ico` is also embedded in `twayblade.exe` (`crates/app/twayblade.rc`).
- `linux/com.twayblade.desktop.desktop` and `linux/com.twayblade.desktop.metainfo.xml`:
  desktop entry and AppStream data, validated in CI. The window's Wayland
  `app_id` is the same ID, so docks match it to the entry and icon. The
  `matrix:` URI handler is left out until the app accepts a URI on launch.
- Linux packages (#53), built by the Linux job in `build.yml` from the same
  stripped release binary, unsigned:
  - `.deb` with `cargo deb` and `.rpm` with `cargo generate-rpm`, configured
    in `crates/app/Cargo.toml`. Library dependencies are detected from the
    binary; a Secret Service provider and `xdg-desktop-portal` are recommended.
  - AppImage with `linux/build-appimage.sh` (linuxdeploy pinned by tag and
    SHA-256).
  - `aur/PKGBUILD`: template for `twayblade-bin` from the release tarball,
    which now carries the desktop entry, AppStream data and icons. Pushing to
    the AUR needs explicit authorization.
  - Versions: a `v*` tag's version, otherwise `<Cargo version>+git<sha>`;
    `-` becomes `~` so pre-releases sort before the final release.
- Windows installer (#54): `windows/twayblade.wxs`, built by the Windows job
  in `build.yml` with WiX 5 into `Twayblade-<version>-x86_64.msi`. Per-machine
  install in Program Files with a Start menu shortcut carrying the
  AppUserModelID `com.twayblade.desktop`, which Windows needs before it shows
  the app's toast notifications. Major upgrades replace older versions (and a
  pre-release of the same version). Unsigned. No MSIX: it cannot be
  installed at all without a trusted signature. WiX 6 and later need a
  decision on the Open Source Maintenance Fee EULA, so the build stays on 5.
- `winget/`: manifest templates for `zerw0.Twayblade`. VERSION and SHA256 come
  from a published release; submitting to winget-pkgs needs the signed MSI and
  explicit authorization.
- The Nix package (`nix/package.nix`) writes its own desktop entry; align it
  with these files when the first native package is built.
- The macOS bundle comes from `macos/Info.plist` and `macos/build.sh`;
  `macos/make-dmg.sh` packs it into a compressed disk image with an
  Applications shortcut (`dist/Twayblade-<version>-<arch>.dmg`).
- `homebrew/twayblade.rb`: cask for our own tap (#52). The app is ad hoc
  signed, so it stays out of homebrew/cask, which requires notarisation. Its
  caveat explains Open Anyway on first launch, and it does not strip
  quarantine. `scripts/homebrew-cask.py --version X --sums SHA256SUMS` fills
  the version and the DMG's SHA-256 (tests: `python3
  scripts/test_homebrew_cask.py`). Creating and pushing the tap repository
  (`zerw0/homebrew-twayblade`) needs explicit authorization.
- Release flow: pushing a `v*` tag builds every platform and creates a **draft
  pre-release** with the zips, tarball, DMG, `SHA256SUMS` and notes drafted by
  `scripts/release-notes.py` from the PRs merged since the previous tag. A
  person reviews and rewords the notes, then publishes; stable is a separate
  promotion. Pushing a tag needs explicit authorization.

Still open: MSI signing, winget submission, the tap
repository, signing and notarisation (needs credentials and explicit
authorization), promotion from pre-release to stable, and the in-app updater
download path.
