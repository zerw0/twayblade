# More colour schemes (#178)

## Built

Eight more schemes in Settings › Appearance, each with the theme's own
accent:

| Scheme | Light | Dark |
|---|---|---|
| Dracula | (dark only) | Dracula |
| Tokyo Night | Day | Night |
| One Dark | One Light | One Dark |
| Monokai | (dark only) | Monokai |
| Everforest | Light (medium) | Dark (medium) |
| Rosé Pine | Dawn | Rosé Pine |
| Kanagawa | Lotus | Wave |
| Tomorrow (terminal.sexy) | Tomorrow | Tomorrow Night |

- Surfaces use each theme's published values. Where a theme's secondary
  (comment) colour or text falls below 4.5:1 on any surface it is drawn on,
  it is moved toward black or white just far enough; the palette contrast
  tests enforce this for every scheme and variant.
- Dracula and Monokai have no light palette: they always use dark mode and
  hide the Palette variant control.
- Stored as `scheme` values `dracula`, `tokyo_night`, `one_dark`, `monokai`,
  `everforest`, `rose_pine`, `kanagawa`, `tomorrow`.

## Verified

- Compile: strict workspace Clippy. Offline tests: palette contrast and
  accent tests for all new schemes and variants; the scheme switch UI test
  clicks every tile.
- Offline capture of the Appearance tiles reviewed.

## Not verified

- Each scheme on a physical Mac in both variants.
