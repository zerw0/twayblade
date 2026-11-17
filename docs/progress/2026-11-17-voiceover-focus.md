# VoiceOver follows keyboard focus (#166)

## Cause

GPUI's macOS window adds its own view as a subview of the window's content
view and makes it the first responder. `a11y_init` attached AccessKit with
`SubclassingAdapter::for_window`, which subclasses the content view. AppKit
asks the first responder for the focused element, so `AXFocusedUIElement`
stayed the window, although every node's own `AXFocused` was correct and
GPUI named the right node in `TreeUpdate.focus`.

## Built

- Vendored gpui-pre-macos 0.3.12 with a one-line change (`vendor/README.md`):
  the adapter is attached to GPUI's view (`SubclassingAdapter::new`).
- Minimal repro (a GPUI Kit window with two buttons, focus moved by a timer)
  kept outside the repository; the upstream issue text is drafted for Zed.

## Verified

- Repro, read through the macOS Accessibility API: before the patch
  `AXFocusedUIElement` was always `AXWindow` while the buttons' `AXFocused`
  alternated; with it, it is the focused `AXButton` each time, and the tree
  is unchanged.
- Twayblade preview with a temporary (uncommitted) focus cycler: the focused
  element is reported as the Settings and Create a room buttons.
- Compile: strict workspace Clippy; app tests pass.

## Not verified

- VoiceOver speech with a real keyboard (Tab) on a physical Mac.
- Some focus stops have no accessible node (the cycler saw "none" between
  buttons, likely panel areas with a focus handle but no role); VoiceOver
  says nothing there. Follow-up.
- Setting `AXFocused` from an assistive app does not move GPUI focus.
