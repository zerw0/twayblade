# Compact buttons in sign-in, panels and Settings

Compact buttons (2026-11-15, `feature/camera-linux-windows`), following the
earlier change to Security, Account and the management sheets.

Implemented:
- Sign-in: the welcome card's Sign in button is content-width and centred with
  a leading icon. The server step ends in a trailing footer, Back then
  Continue, as in macOS sheets. Browser, SSO and identity-provider choices
  stack centred at their own width; the password form's Sign in sits at the
  trailing edge under the fields; Cancel (while waiting for the browser) is
  centred.
- Session recovery: Re-authenticate and Sign in again are content-width with a
  leading icon; the reset confirmation puts Cancel and the destructive
  "Delete local data and continue" (now styled as danger) in a trailing row.
- Side panels: profile and member actions are content-width and aligned with
  the panel's leading edge instead of stretched with centred labels. Load More
  Results and Load Older are centred with a chevron. Try Again buttons in
  search, files, profile, the conversation and the room directory carry a
  refresh icon and no longer stretch.
- Settings: Open System Settings (notifications denied), the sample
  notification's Open conversation and View Release on GitHub are
  content-width; the first and last carry an external-link icon.

Verified: compilation, strict clippy, the offline UI test suite, and captures
on macOS of the welcome, server, identity-provider and soft-logout screens,
every Settings section and the profile and room info panels. Not verified:
the denied-notifications and update-available states on screen (no preview
for them), the password form and revoked-session confirmation on screen,
Linux and Windows.
