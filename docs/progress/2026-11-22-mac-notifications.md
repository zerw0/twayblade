# macOS notifications and call alerts

## Cause

- On the reporter's Mac no notification request reached macOS in three days
  (unified log: badge updates only). `preferences.json` had
  `notifications.enabled: false`; its `.bak` from 17 Nov had `true`. Any field
  or value a build could not read (development and release builds share the
  file) reset every preference to defaults, and notifications default to off.
- Group-room calls announce themselves with `notification_type:
  "notification"`, not `"ring"`. Only rings produced an OS alert, so a call
  starting in a group room showed nothing even with notifications on.
- "Open System Settings" always opened the development bundle's entry
  (`com.twayblade.desktop.development`), also in release builds.

## Built

- Preferences ignore unknown fields. A section that cannot be read (for
  example a theme name from another build) falls back to its own defaults; the
  other sections are kept. A corrupt file or another schema version is still
  backed up and reset as before.
- Notifications are on by default, so the system permission prompt appears
  once an account opens. Saved choices are unchanged: a file that says `false`
  stays off.
- A group call's start is a normal notification ("Name started a call", opens
  the room). Rings keep Accept/Decline.
- "Open System Settings" uses the running bundle's identifier.
- Notifications show a round avatar when message previews are on: the person
  in a direct chat, the room elsewhere (each falls back to the other); calls
  use the room's picture. Fetching it waits at most 4 s, then the
  notification goes without it. macOS keeps the app icon on the left (only
  Communication Notifications, which need an Apple entitlement and real
  signing, replace it) and shows the avatar as the attached image on the
  right. Linux sends it as `image-path`; Windows uses it as the toast's logo,
  cropped round. Image files live in the cache folder's
  `notification-images` and are removed after a day.
- Diagnosis, not code: on the reporter's Mac, clicking a notification
  launched a second, older copy. LaunchServices had an rc.6 bundle in the Nix
  store registered under the same identifier, and usernoted opened that path.
  Unregistering it (`lsregister -u`) fixed it there.

## Verified

- `cargo fmt --check`, clippy (workspace and `--features capture`), workspace
  tests and the 250 offline UI tests on macOS. Tests cover unknown preference
  fields and an unreadable section, which avatar a notification picks, and the
  round mask.

## Not verified

- A live group call start, message notifications and the attached avatar on a
  physical Mac with this build; the permission prompt on a fresh install.
- Linux and Windows: the image code was not compiled locally (no targets
  here); CI compiles it, delivery is untested.
