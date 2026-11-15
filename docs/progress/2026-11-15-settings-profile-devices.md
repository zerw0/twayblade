# Settings profile header and Devices section

Settings profile and devices (2026-11-15, `feature/camera-linux-windows`, UI
feedback): the Settings sidebar starts with the account's photo, display name
and Matrix ID; selecting it opens the Account pane, where the name and photo are
changed. Account is no longer a row in the list. The device list moved from
Security into its own Devices section (Refresh, per-device Sign out and its
confirmation); Security keeps recovery and verification. The display name is
fetched with the avatar when the account loads. Verified by a capture of the
sidebar (sample data), the existing 149 offline tests and compilation. Not
verified: the Devices pane and the profile header with a live account, a real
avatar image in the header, and the device sign-out confirmation from the new
section.
