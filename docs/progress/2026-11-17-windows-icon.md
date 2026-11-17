# Windows installer icon (#194)

## Built
- The MSI Icon table entry is now `Twayblade.ico` instead of `TwaybladeIcon`.
  Windows Installer shows an advertised shortcut's icon, and the Apps &
  Features icon, only when that name ends in `.ico` or `.exe`; the taskbar
  takes the icon from the Start menu shortcut through the AppUserModelID.
- `twayblade.exe` already embedded the icon: the 0.1.1-rc.3 exe has
  RT_GROUP_ICON 1 with seven sizes (16–256 px), checked with pefile.

## Verified
- The rc.3 exe's icon resources and the `.ico` frames (rendered and inspected).
- The MSI builds in a manual run of the Build binaries workflow on this branch.

## Not verified
- On Windows: Start menu, taskbar, Apps & Features and Explorer icons after
  installing the new MSI. An existing icon cache may need a sign-out.
