# macOS-first desktop interface

Superseded frontend decision: the user subsequently approved one GPUI + GPUI Kit
frontend for macOS/Linux/Windows. See [the current architecture](GPUI.md). The
SwiftUI/Iced details below are retained as design and implementation history.

Decision: 2026-11-10, approved by the user. Use SwiftUI/AppKit on macOS and
Iced on Linux/Windows, sharing the Rust core through a narrow UniFFI boundary.
macOS is the primary UX platform. The Iced implementation notes below describe
the earlier prototype; the approved frontend section records the migration.

## Reference and target composition

The user's 2026-11-10 Cinny screenshot is the information-layout reference:
space/home rail, compact room list, room header, conversation, composer and an
optional member inspector. Adapt that structure to a rounded macOS visual style;
do not reproduce Cinny's browser chrome or imply a web-based chat UI.

- Space rail: Home/All, DMs, joined spaces and a compact add/join affordance.
- Sidebar: selected space name, search, rooms/DM sections, subtle unread badges,
  compact row spacing, rounded selection and distinct room identities.
- Header: room avatar/name and real search, members, details and call actions as
  those features become functional. Avoid decorative inactive toolbars.
- Timeline: real display names with Matrix identity available, compact avatars,
  timestamps, consecutive-message grouping, date/unread separators, reply context,
  attachment previews, reactions and visible delivery/decryption states.
- Inspector: optional members/details/thread panel with independent scrolling;
  close it to maximize the conversation area.
- Composer: rounded field, attachments, formatting, emoji/sticker pickers and
  send action. Add each control with its working behavior and keyboard support.
- macOS treatment: proportional bold/semibold headings, restrained hierarchy,
  soft separators, 6–12 point corner radii and system appearance. Keep the main
  conversation surface quiet and the navigation visually distinct.

Use semantic color roles (window, sidebar, field, label, secondary label,
separator, accent and selection). The current palette approximates Mac colors
and tracks system light/dark mode; it does **not** read native semantic colors or
the user's accent setting yet. Add a macOS platform adapter for actual system
colors, high-contrast/reduced-transparency preferences and live appearance/accent
changes, with portable defaults. Do not promise vibrancy without implementation.

Acceptance: compare light/dark and high-contrast appearances on macOS, inspect
Retina rendering, long room/display names, populated timelines and attachments,
and verify keyboard/VoiceOver order. Screenshots with synthetic data must be
marked as offline previews. Preserve Linux/Windows native operation.

## Earlier Iced implementation

- Standard decorated, centered 640 × 700 login window; minimum 480 × 560.
- Narrow 360-pixel form, scrollable for smaller windows.
- Compact labels, 14-pixel inputs/actions, restrained rounded borders, blue accent.
- System light/dark theme query on launch and subscription to appearance changes.
- Server selection first, account form second, with a Change server action.
- Masked password field, Return-to-continue/sign-in, disabled controls during work.
- Plain product language rather than raw Matrix login flow identifiers.
- Shared appearance module that can evolve with the eventual conversation UI.
- Explicit macOS San Francisco family with bold/semibold headings and portable fallback.
- Signed-in 1120 × 760 split view with sidebar/search, unread counts and room selection.
- Unified SDK room/encryption synchronization with reconnect and connection indicators; live acceptance pending.
- Tab/Shift-Tab navigation, Command-F search and Command-Q handling.
- Credential-free previews and app-window-only capture for visual QA.

The native workspace preview was launched and inspected on macOS arm64. Build,
Clippy and the current regression tests pass. An observed monospaced bold fallback was
corrected. Native menus, live room loading, physical keyboard and accessibility
remain separate acceptance steps.

These are custom-rendered Iced controls. They do not establish AppKit control
integration, system accent matching, vibrancy, accessible contrast or screen-reader
coverage. Check native input behavior and appearance on real hardware.

## Next platform work

1. Validate light/dark transition, Retina rendering, resizing, text selection,
   keyboard focus/Tab order, VoiceOver and password input behavior.
2. Add application/Edit/Window menus and standard Command shortcuts through a
   macOS platform adapter; retain familiar equivalents on other OSes.
3. Define close/hide/reopen and background-call behavior, then implement it.
4. Build a split-view conversation window: sidebar navigation, room list,
   conversation and optional thread inspector. No fabricated conversations.
5. Add native preferences, notification activation, browser callback handling,
   camera/microphone entitlements and signed application identity.

The user subsequently approved the SwiftUI macOS frontend described below.
Iced remains the portable frontend and a useful reference implementation.

## Joined-space navigation

A 64-point rail now sits before the conversation sidebar. It provides All,
joined-space initials, rounded selection, unread totals, hover names and a nested
entry marker. Selecting a space changes the sidebar title and filters by room ID.
The offline populated preview starts in a sample parent space and excludes rooms
from the other sample space. This preview is explicitly labeled and cannot send.

The core uses SDK SpaceService hierarchy filters; the UI retains drafts when
navigation hides a selected conversation. Two navigation levels are currently
exposed by the SDK. Discovery and space membership/management controls will be
added with working behavior and separate interoperability acceptance.

## Apple-design guidance and revised Home

Reference skill:
https://github.com/emilkowalski/skills/blob/main/skills/apple-design/SKILL.md

Apply typography hierarchy, restraint, familiar controls and responsive feedback
within Rust/Iced. Motion and native material/accessibility work need separate
implementation; this iteration adds no animation or transparency effects.

The current redesign loads San Francisco from the installed macOS font before
opening the window, uses vector icons and compact room rows, moves account
controls behind a bottom menu, and insets the conversation with rounded surfaces.
Home shows only rooms outside known spaces; it is not a duplicate space room list.
Inspect both Home and selected-space populated previews before reporting QA.

QA: both redesigned offline Home and selected-space captures were inspected.
The loaded macOS font uses semibold emphasis to avoid a renderer fallback seen
with the bold weight. This pass does not claim AppKit controls or native vibrancy.

## Additional reference: Apple Messages

The user's 2026-11-10 Messages screenshot is a second design reference alongside
Cinny. Use its circular avatars, conversation previews and timestamps, floating
toolbar controls, rounded composer and incoming/outgoing bubbles as direction.
Do not copy private conversation content, contact identities or media into
fixtures or repository assets.

Consider Messages-style bubbles for DMs. Group rooms still need sender identity,
threads, reactions and dense history. Preserve space navigation and Home's
exclusion of space-owned rooms. Keep conversation content readable, with glass
used for navigation and controls where appropriate.

### Approved frontend split — first implementation

The user approved **SwiftUI/AppKit for macOS and Iced for Linux/Windows**.
This supersedes the provisional Iced-only decision above. The goal is consistent
information architecture, density and behavior across both UIs, with platform
controls/materials. Liquid Glass is an Apple rendering feature, not a promise
of identical rendering in Iced.

`macos/` contains SwiftUI views and an AppKit window/menu host. `crates/bridge/`
uses pinned UniFFI 0.32.2 to generate the Swift boundary. Rust owns Matrix clients,
credentials, stores, sync, spaces and timelines. Swift owns only presentation
state, selection, drafts and safe snapshots. Blocking commands execute off the
main actor. A coalescing watch channel bounds updates; epoch/selection guards
reject stale account/timeline results and stopping aborts account tasks.

Wired behavior: password discovery/login, secure restoration/logout, joined
spaces/Home, room selection, timelines, plain sending and history pagination.
Native AppKit menus provide standard edit/quit/hide/window commands and search.
The interface uses semantic system colors, SF Symbols, native sidebar controls,
rounded message bubbles, selectable text and a glass composer on macOS 26+;
older macOS uses regular material. Home excludes all known space descendants.

The app builds and offline Swift/ABI checks pass. Window creation and preview
capture are exercised independently of saved sessions. Live SwiftUI restoration,
sending, pagination anchoring, VoiceOver, IME, reduced transparency and accent
changes still need acceptance. XCTest could not run with this CLT installation.
The bridge is an initial implementation, not completed M0 acceptance.

See DEVELOPMENT.md for build/preview commands and the Apple SDK workaround.
Element Call remains a separate restricted runtime spike.

Primary references:

- https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass
- https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views
- https://developer.apple.com/documentation/appkit/nsglasseffectview
- https://mozilla.github.io/uniffi-rs/latest/
