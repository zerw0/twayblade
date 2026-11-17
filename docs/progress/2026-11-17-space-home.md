# Space home (#177)

## Built

- The former Browse Rooms page is now **Space Home**, modelled on Element's:
  - Header: the space's picture, name, **Public space** / **Private space**,
    member count and topic, with **Invite** (primary) and, for people who
    may add rooms, **Add Room…**.
  - **Rooms and spaces**: a searchable tree. Subspaces come first and have
    a disclosure arrow; expanding one loads its rooms (first page; a link
    opens a large subspace as its own home). Rows show picture, name,
    Suggested, room and member counts, access hints and topic. Joined rooms
    show **Joined** and open on click; joinable ones have **Join**; an arrow
    opens a subspace's own home.
  - Search matches names and topics, including rooms inside subspaces that
    have been loaded, and keeps their parents visible.
- Opened by clicking the space name at the top of the sidebar, or the space
  context menu's **Space Home** (formerly "Browse rooms").
- A dialog opened from a full page (Invite, Add Room…) returns to that page
  when it closes instead of dropping back to the conversation.
- Core: `DirectorySnapshot` gains topic, avatar, members and access;
  `DirectoryRoom` gains avatar, members and child count. Backend
  `space_children` loads a subspace without touching the open directory.
- Space graphs that list an ancestor do not recurse.

## Verified

- Compile: strict workspace Clippy. Offline tests: UI test opens the home
  from the space name, expands a subspace, searches (match inside a subspace
  keeps its parent; no match shows a status), and returns from Invite.
- Local Synapse (`run.sh spaces`): the public space reports public access
  and at least two members; the subspace row reports its child; opening the
  subspace lists its joined room with members. The profile then stops at the
  known voice-channel failure (#176).
- Offline capture of the page reviewed.

## Not verified

- A live account and Element side by side; keyboard access to open a joined
  room from the tree (rows open by pointer; the sidebar remains the keyboard
  path); space settings from the header (no space settings page yet).
