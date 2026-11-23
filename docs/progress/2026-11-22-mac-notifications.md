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

## Verified

- `cargo fmt --check`, clippy (workspace and `--features capture`), workspace
  tests and the 250 offline UI tests. A new preferences test covers unknown
  fields and an unreadable section.

## Not verified

- A live group call start and message notifications on a physical Mac with
  this build; the permission prompt on a fresh install.
- Linux and Windows delivery of the new group-call notification.
