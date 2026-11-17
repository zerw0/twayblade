# Windows installer: desktop shortcut prompt (#195)

## Built
- The MSI now has a short wizard: Welcome, then a Shortcuts page ("Twayblade
  is always added to the Start menu." and a ticked "Create a desktop
  shortcut" box), then Install. Repair/Remove keep the standard maintenance
  pages. No licence page.
- The desktop shortcut (same AppUserModelID and icon as the Start menu one) is
  installed only when `DESKTOPSHORTCUT` is set; it defaults to `1`, so silent
  installs get it unless `DESKTOPSHORTCUT=""` is passed.
- CI adds `WixToolset.UI.wixext/5.0.2` and builds with `-ext WixToolset.UI.wixext`.

## Verified
- The MSI builds in a manual run of the Build binaries workflow on this branch.

## Not verified
- On Windows: the Shortcuts page, ticking and clearing the box, upgrade from
  rc.3, and uninstall removing the desktop shortcut. The choice is not
  remembered across upgrades; an upgrade asks again.
- The wizard uses the WiX default banner and dialog bitmaps.
