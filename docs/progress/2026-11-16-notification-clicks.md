# Notification clicks open their room (#111)

## Behaviour

During live acceptance of #82/#97 on the physical Mac, clicking a
notification did nothing. With `TWAYBLADE_LOG=1`, the log showed that the
response handler never ran. `gpui_kit::init` makes GPUI's own
`UNUserNotificationCenter` delegate current, which replaced the app's delegate
installed before launch. The app now installs its delegate again right after
`gpui_kit::init`. The app does not use GPUI Kit's system notifications, so
nothing depends on GPUI's delegate.

Once clicks arrived, a click for the other account switched accounts but left
no room open. The click opened the room in Home, and Home lists only rooms
outside spaces. The next update therefore closed a room that belongs to a
space. A click now opens the room where the sidebar lists it, in Home or in its
space, and waits until spaces have loaded. The same fix applies to clicks for
the open account.

Diagnostics added, with counts and flags only: `notification_response`,
`notification_click`, `ui_notification_other_account`, `ui_activation_resolve`
and `ui_account_changed`. `ui_selection_queued` now also records whether the
selection was cleared (`empty`).

## Verification

- `a_click_for_another_account_opens_its_room_after_the_switch` replays the
  logged sequence: click, switch, empty list, list, another update. It fails
  without the change (no room open) and passes with it.
- `cargo test -p twayblade --all-features --locked`: 181 passed.
  `twayblade-platform` tests pass.
- Physical Mac, live account (reported by the owner): a click on a
  notification for the inactive account switches to it and opens the room.

## Not verified

- A cold launch from a notification after the change.
- Linux and Windows notification clicks.
