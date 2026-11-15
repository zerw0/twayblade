# Message search sender filter

Message search sender filter (2026-11-15, `feature/search-sender-filter`, issue #35):
a From menu next to Has Files limits results to one person of the room you are
viewing (or Anyone), using the existing `SearchQuery.sender`. Verified by a
capture, an offline UI test and compilation; not run against a real server, and
it lists only the current room's people (60 at most), so searching all rooms for
someone who is not in this room needs their Matrix ID typed, which is not built.
