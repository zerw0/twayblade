# Discord colour scheme (#180)

## Built

- New **Discord** scheme in Settings › Appearance, with light and dark
  palettes and Discord's blurple (#5865f2) as its accent.
- Dark matches Discord's dark theme: chat #313338, channel list #2b2d31,
  server rail and title bar #1e1f22, composer #383a40, text #dbdee1. The one
  exception is secondary text, lifted from #949ba4 to #a1a8af so it reaches
  4.5:1 on the composer.
- Schemes can now set their own title bar and space rail colours instead of
  deriving them from the sidebar (used by Discord only).

## Verified

- Compile: strict workspace Clippy. Offline tests: palette contrast tests for
  Discord light and dark; the scheme switch test covers the new tile.
- Offline capture of the main window in Discord dark reviewed.

## Not verified

- Side-by-side comparison with the Discord app on a physical Mac.
