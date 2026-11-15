# Archive and Invite a Contact

Avatar menu features (2026-11-15, `feature/camera-linux-windows`), completing
the menu items left out of the room-menus change.

Implemented:
- Archive: the room list's right-click menu gains "Archive" or "Move out of
  Archive". Archiving sets the Matrix `m.lowpriority` tag (core
  `Account::set_archived`, exposed as `RoomSummary::archived`), so it syncs to
  other clients; Element shows these rooms under Low priority. The tag and
  `m.favourite` exclude each other: the SDK clears one when setting the other.
  Archived rooms leave Favourites, Rooms and Direct messages for an Archive
  section at the bottom of the sidebar, collapsed by default. Inside a space it
  lists that space's archived rooms; Home lists every archived room, from any
  space. The avatar menu's "Archive" (disabled when nothing is archived) opens
  Home with the section expanded.
- Invite a Contact: a dialog from the avatar menu shows your Matrix ID and its
  `https://matrix.to/#/<user>` link, with Copy Link (which then reads
  "Copied") and Start a Direct Message… for someone already on Matrix. It opens
  focused on the sheet and follows the same Escape, Tab and focus-return rules
  as the other dialogs.
- Previews: `--preview-archive` and `--preview-invite-contact`.

Not built: Add Account. The app is single-account in core (one keychain slot,
`AuthError::ExistingSession`) and in the backend; issue #82 specifies the
technical work for Codex first.

Verified: compilation, strict clippy, offline tests (archiving moves a room to
the Archive, the avatar menu's Archive shows it from Home, favouriting
restores it; Copy Link puts the matrix.to link on the clipboard), and macOS
captures of both previews.

Live, isolated Synapse (`tests/integration/run.sh archive`): two devices of one
fixture account. Archiving through `Account::set_archived` reaches the app's
own room list projection and the other device, which stores exactly one tag,
`m.lowpriority` (what Element reads); archiving clears an existing favourite,
restoring clears the tag, and favouriting an archived room moves it out of the
Archive on both devices.

Live, real account on a physical Mac (with the user's permission, one direct
message without a favourite, restored afterwards): the room's right-click menu
offers Archive; choosing it moved the conversation into a collapsed Archive
section through the homeserver. The avatar menu shows Invite a Contact… and an
enabled Archive, which expanded the section on Home. "Move out of Archive"
returned the conversation to its original place and the empty section went
away. Invite a Contact showed the real Matrix ID and link; Copy Link put
exactly that matrix.to link on the clipboard (the previous clipboard was
restored) and Escape closed the dialog. That check found the row shifting when
"Copy Link" became "Copied"; the button now has a fixed minimum width.

Not verified: how Element shows the archived room, opening the copied link in
another client, scrolling to the Archive when a long room list hides it (the
avatar menu expands the section but does not scroll), Linux and Windows.
