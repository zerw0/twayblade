# Linux packages (#53)

Branch `feature/linux-packages`.

## Behaviour

- The Linux job in `build.yml` strips the release binary and builds, from it:
  - `twayblade_<version>_amd64.deb` (`cargo deb`) and
    `twayblade-<version>-1.x86_64.rpm` (`cargo generate-rpm`), configured in
    `crates/app/Cargo.toml`. Library dependencies are detected from the
    binary (`dpkg-shlibdeps`, `find-requires`/`ldd`). Libraries the app opens
    at run time are declared explicitly, found by inspecting the CI binary:
    Wayland client and EGL, EGL/Vulkan for rendering, X11, GBM and DRM (rpm
    requires them by soname, so Fedora and openSUSE both resolve them). Both
    recommend a Secret Service provider (GNOME Keyring, KWallet or KeePassXC),
    which keeps the session, plus `xdg-desktop-portal`, PipeWire and ALSA for
    screen sharing and calls.
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
- Packages carry the app icon (now the one described in
  `2026-11-16-app-icon.md`), with `render.sh` for the PNG and Windows `.ico`.
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
- The build workflow, run manually on this branch, produced the deb, rpm,
  AppImage and tarball (x86_64). The deb's detected dependencies were
  inspected: libc, D-Bus, GLib, PulseAudio, XCB and xkbcommon; the runtime-
  opened libraries above were missing and are now declared.
- Not verified: installing, launching,
  upgrading and removing on Debian/Ubuntu, Fedora, openSUSE and Arch, icon and
  dock matching on GNOME and KDE (Wayland and X11).
