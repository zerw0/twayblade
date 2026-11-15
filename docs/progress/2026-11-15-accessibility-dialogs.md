# Accessibility pass, part 1

Accessibility pass, part 1 (2026-11-15, `fix/accessibility-dialogs`, issue #51):
a code audit found no icon-only button without an accessible name and no
clickable container without one worth fixing, but three gaps: the image viewer
exposed no role or label, the create/DM/join/invite sheets were not dialogs, and
the side panels (search, thread, profile, room info) were unnamed. The viewer is
now a dialog named "Image viewer, Image N of M" with the image labelled by file
name and sender and the position as a status; the management sheet is a dialog
named by its heading; the side panels are labelled regions. Verified by an
offline test of those names and compilation. Not verified: VoiceOver, Orca or
Narrator, focus order and focus return, keyboard-only use of every control, or
contrast in every scheme. The rest of #51 stays open.
