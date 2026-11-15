# Linux packages (#53)

Branch `feature/linux-packages`.

## Behaviour

- The Linux job in `build.yml` strips the release binary and builds, from it:
  - `twayblade_<version>_amd64.deb` (`cargo deb`) and
    `twayblade-<version>-1.x86_64.rpm` (`cargo generate-rpm`), configured in
    `crates/app/Cargo.toml`. Library dependencies are detected from the
    binary (`dpkg-shlibdeps`, `find-requires`/`ldd`). Both recommend a Secret
    Service provider (GNOME Keyring, KWallet or KeePassXC), which keeps the
    session, and `xdg-desktop-portal` for screen sharing on Wayland.
  - `Twayblade-<version>-x86_64.AppImage` with
    `packaging/linux/build-appimage.sh`, using linuxdeploy pinned by tag and
    SHA-256 and run without FUSE.
  - The existing tarball, which now also carries the desktop entry, AppStream
    data, icons and licence for the AUR package.
- Every package installs the binary, desktop entry, AppStream data, a scalable
  and a 512 px icon, and the licences. CI validates the desktop entry and
  AppStream data first.
- `packaging/aur/PKGBUILD`: `twayblade-bin` template from the release tarball.
- Versions: a `v*` tag's version, otherwise `<Cargo version>+git<sha>`; `-` is
  written as `~` so `0.1.0~rc.1` sorts before `0.1.0` in dpkg and rpm.
- The app icon is new (`packaging/icons/com.twayblade.desktop.svg`: the
  twayblade's two leaves under a flower spike), with `render.sh` for the PNG
  and Windows `.ico`.
- The main and Settings windows set the Wayland `app_id`
  `com.twayblade.desktop`, so docks match the window to the desktop entry and
  icon. GPUI sets no X11 `WM_CLASS`, so the desktop entry no longer claims a
  `StartupWMClass`.
- The `matrix:` URI handler stays out until the app accepts a URI on launch.

## Verification

- Compile on macOS: formatting and strict Clippy.
- `desktop-file-validate` and `appstreamcli validate` pass locally.
- `cargo deb` and `cargo generate-rpm` (pinned versions) ran in a Debian
  container on arm64 with a stand-in binary: file lists, permissions,
  recommends and metadata are as intended. This does not check the real
  binary's library dependencies or the x86_64 build.
- Not verified: the AppImage build (x86_64 only), the CI job itself (the build
  workflow runs on `main`, tags and manual runs), installing, launching,
  upgrading and removing on Debian/Ubuntu, Fedora, openSUSE and Arch, icon and
  dock matching on GNOME and KDE (Wayland and X11).
