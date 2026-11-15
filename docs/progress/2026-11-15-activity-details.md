# Notifications popover, phase 2 (#99)

Branch `feature/activity-details`, based on `feature/notifications-popover`
(#91, phase 1).

## Behaviour

- **Message previews.** Each mention and unread row shows its room's latest
  message on a second line:
  - In a room, the sender's name and the text ("Sam: …"); in a direct chat,
    only the text; your own messages start with "You:".
  - Media, stickers, polls, undecryptable and deleted messages are named
    ("Image", "Encrypted message", …).
  - The text is the first line, at most 160 characters.
  - With message previews off in Settings › Notifications, no preview is
    shown.
  - Core reads the SDK's latest-event value through `RoomExt::latest_event`
    and starts tracking only the rooms the popover lists.
  - Previews are requested when the popover opens and again when a listed
    room gets newer activity. A newer request replaces an unfinished one, and
    results from another account are dropped.
- **Missed calls.**
  - A ring counts as missed when it disappears while ringing without being
    answered (its room is not the current call) or declined or dismissed in
    this app. Group-call announcements without ringing are not missed calls.
  - The list holds at most 20, newest first, with caller, video or voice,
    room and time.
  - A missed call clears when its room is opened (from the popover or
    anywhere else), or with Mark All as Read.
  - Kept for the running session only. Missed calls are not read back from
    room history after a restart.
  - Each one counts on the bell's badge.
- **Mark All as Read** (double-check icon in the header, shown while there is
  something to clear):
  - For every listed room, sends a read marker and a public read receipt for
    its latest event (unthreaded, so thread unread clears too) and removes a
    manual unread mark.
  - Clears the missed calls.
  - A failing room does not stop the others and shows an error afterwards.
  - Invitations stay.

## Verification

- Compile: `cargo fmt --check` and strict all-target, all-feature Clippy pass
  (macOS host).
- Offline tests (161 app tests, plus a core unit test). New:
  - Missed-call detection: still ringing, answered, declined, timed out, and
    group announcements.
  - The missed list's order, limit and deduplication.
  - Preview wording (sender, direct chat, own message, media, encrypted).
  - Bell counting of missed calls.
  - The popover showing a missed call and a preview, and Mark All as Read
    clearing rooms and calls.
- Local Synapse (`run.sh activity`, new):
  - Bob's preview of Alice's message is its first line, with Alice as the
    sender.
  - Bob's unread count goes to zero after `mark_rooms_read`.
  - An unknown room is reported as an error.
- Capture of `--preview-activity` (sample data) reviewed.

## Not verified

- Missed-call detection against a real ring, from Element or a second
  Twayblade, on the live account or local Synapse.
- Encrypted-room previews were not checked live. The local test room is
  unencrypted.
- VoiceOver reading of the new rows. Linux and Windows.
