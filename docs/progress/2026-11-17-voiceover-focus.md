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

- Read through the raw macOS Accessibility API (`AXUIElementCopyAttributeValue`
  on the application's `AXFocusedUIElement`), as VoiceOver does:
  - Repro without the patch: always `AXWindow`. With it: the focused button
    (First, Second, a dropdown-menu button, a button inside a context-menu
    wrapper, a plain button) at every step.
  - Twayblade preview with a temporary (uncommitted) focus cycler: every Tab
    stop is reported with its label: search field, Notifications, Settings,
    Account and status, Home, the space tile, Add a space, Space Home,
    Create a room, room rows, Create Room….
- System Events (AppleScript/JXA) cannot address some of these elements and
  reports them as missing; that is a limitation of the probe, not of the
  app (#187).
- Compile: strict workspace Clippy; app tests pass.

## Not verified

- VoiceOver speech with a real keyboard (Tab) on a physical Mac.
- Setting `AXFocused` from an assistive app does not move GPUI focus.
