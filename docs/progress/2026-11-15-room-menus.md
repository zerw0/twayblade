# Room and profile menus

Menus (2026-11-15, `feature/camera-linux-windows`, UI feedback): the room list's
right-click menu gained "Mark as unread" (shown when the room has nothing
unread) and "Leave…", which opens a small confirmation dialog before the account
leaves the room or direct message; core has new `Account::set_unread` and
`Account::leave_room`. The avatar menu in the title bar gained "Create Room…"
and "Start a Direct Message…" next to the existing profile, status and account
settings entries. Status already existed as presence (Online, Away, Invisible).
Not added: Archive, Invite contact and Add account, which have no backing
feature (the app is single-account). Verified by an offline test of the leave
dialog and compilation. Not verified: the native context and dropdown menus
themselves (offline tests cannot open them), the unread marker and leaving
against a real homeserver, and how Element shows the marker.
