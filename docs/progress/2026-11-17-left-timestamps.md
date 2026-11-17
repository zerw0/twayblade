# Always-on timestamps in a left column (#193)

## Built
- Minimal style with Settings › Chat › Timestamps set to Always: every message
  line, and every membership line, leads with its time in a fixed 38 pt column
  at the group's left edge; the avatar and text follow. The time beside the
  sender name is dropped, since the first line already shows it. Times are
  `HH:MM`, so the column lines up.
- IRC uses the same time column helper; its look is unchanged.
- On-hover timestamps and the Bubbles style keep the trailing time.
- `--preview-timestamps` turns Always on in the offline preview.

## Verified
- Compile: `cargo clippy -p twayblade --all-targets --all-features -D warnings`.
- Offline UI: preview capture with `--preview-style-minimal --preview-timestamps`.

## Not verified
- Live account, Linux, Windows.
