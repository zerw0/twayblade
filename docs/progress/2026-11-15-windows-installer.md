# Windows installer (#54)

Branch `feature/windows-installer`.

## Behaviour

- `packaging/windows/twayblade.wxs` (WiX 5) builds
  `Twayblade-<version>-x86_64.msi` in the Windows job of `build.yml`, from the
  same release binary as the zip.
- Per-machine install into `Program Files\Twayblade` with the licences, and an
  advertised Start menu shortcut whose AppUserModelID is
  `com.twayblade.desktop`, the ID the toast notifications already use. An
  unpackaged app's toasts only appear once such a shortcut exists.
- Major upgrades replace older versions, and a final release replaces its own
  pre-release (MSI compares only `major.minor.patch`; `-rc.1` is dropped from
  the MSI version but kept in the file name). Downgrades are refused.
- Apps & Features shows the icon and the project and issue links.
- `twayblade.exe` now embeds the app icon (`crates/app/twayblade.rc` via
  `embed-resource`, Windows targets only), so Explorer, the taskbar and Alt-Tab
  show it, and release builds use the Windows GUI subsystem, so starting the
  app no longer opens a console window. Debug builds keep the console.
- `packaging/winget/`: version, installer and locale manifest templates for
  `zerw0.Twayblade`.
- The app icon (`packaging/icons/`) is shared with the Linux packages (#53).

## Not done

- Signing: needs the release certificate and explicit authorization; the MSI is
  unsigned, so SmartScreen will warn.
- MSIX: not built, because it cannot be installed without a trusted signature.
- winget submission: needs a signed MSI and explicit authorization.
- WiX 6+: requires accepting the Open Source Maintenance Fee EULA; staying on
  WiX 5.0.2 until that is decided.

## Verification

- Compile on macOS: formatting and strict Clippy (the Windows resource step
  only runs for Windows targets).
- The build workflow, run manually on this branch, built the exe (with the
  embedded icon) and `Twayblade-0.1.0-x86_64.msi`. Inspected with msitools:
  `ALLUSERS=1`, the fixed UpgradeCode, the Apps & Features icon, the Start
  menu shortcut with `System.AppUserModel.ID = com.twayblade.desktop`, the exe
  and both licences in `Program Files\Twayblade`. The first build failed
  because WiX cannot derive a GUID for a two-file component; the licences
  are now one file per component.
- Not verified: installing, upgrading and
  uninstalling on a clean Windows machine, the Start menu shortcut, toast
  delivery through it, the embedded icon and the absent console window.
