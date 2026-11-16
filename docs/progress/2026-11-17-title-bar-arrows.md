# Back and Forward arrows in the title bar (#170)

## Built

- The compact title bar (#159) dropped the Back and Forward buttons; they are
  back on its left, after the window controls, disabled when there is
  nowhere to go. ⌘[ and ⌘] work as before.
- Settings › Appearance › **Show back and forward buttons** hides them (on by
  default).

## Verified

- Offline UI test: Back is disabled at first, enabled after opening a room,
  goes back on click, and disappears when the setting is off. 212 app tests
  pass.
