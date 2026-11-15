# Notifications popover (#88)

Branch `feature/notifications-popover`.

## Behaviour

- The title-bar bell now opens a popover anchored under it instead of
  Settings › Notifications. It lists:
  - **Invitations**, with Decline and Accept;
  - **Mentions**: rooms with mentions or keywords, newest first, with an accent
    count;
  - **Unread**: other unread conversations (not muted, archived or spaces),
    newest first, up to 20, with a neutral count.
  Each room row opens the room and closes the popover. The gear in the header
  opens Notification Settings. With nothing to show it says "You’re all caught
  up".
- The bell carries an accent badge with invitations plus mentions (99+ cap);
  plain unread messages do not count. Its accessible label includes the count.
- Opening moves focus into the popover, so Escape closes it and Tab moves
  through its rows; closing returns focus. A click outside closes it, and
  that same click on the bell does not reopen it. It rises in over 200 ms,
  instantly under Reduce Motion.
- Built only from the room list and invitations already in the snapshot. Not
  in this phase: message previews, missed calls and "Mark all as read"; each
  needs new data from core.
- Preview: `--preview-activity`.

## Verification

- Compile: formatting and strict Clippy.
- Offline tests: 156 app tests, including ordering and badge counting, and the
  bell opening the popover, Escape closing it and a row opening its room.
- Capture of the preview reviewed.
- Not verified: the live account, VoiceOver, Linux and Windows.
