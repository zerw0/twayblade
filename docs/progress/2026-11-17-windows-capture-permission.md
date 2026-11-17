# Windows/Linux: calls and voice messages refused the microphone (#196)

## Fixed
- `media_permissions::request` and `check` returned `Unsupported` on every OS
  but macOS, and core maps any non-restricted permission error to
  `PermissionDenied`. Every Windows and Linux call therefore failed before it
  connected ("Twayblade can't use your microphone or camera. Allow it in
  System Settings → Privacy & Security."), and voice recording stopped the
  same way. Windows and Linux have no per-app grant API for an unpackaged
  desktop app; their privacy switches apply when the device opens. Both
  functions now return `Ok` there; macOS is unchanged.
- Messages per OS: Windows points to Settings › Privacy & security (also for an
  unavailable device and a microphone that won't start, since Windows blocks at
  device open); macOS keeps System Settings › Privacy & Security; the
  restricted message no longer says "this Mac".

## Verified
- Compile and tests on macOS; a non-macOS unit test (`desktops_without_a_grant_api_do_not_refuse_capture`) runs in Linux and Windows CI.

## Not verified
- A real call and a voice recording on Windows and Linux, including with the
  Windows microphone privacy switch turned off (#49).
