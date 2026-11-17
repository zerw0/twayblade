# Calls: shared screens fit their tile (#202)

## Fixed
- A shared screen was cut off at the bottom: the video image filled the
  tile's width at its natural aspect and ran past the tile, whose
  `overflow_hidden` cut it. A `size_full` image under the view's auto-height
  root ignored the tile's height, so `ObjectFit::Contain` had no box to fit.
  The image is now pinned to the tile's edges (absolute, top/left 0,
  size_full); screens fit with letterboxing, cameras still fill (Cover).

## Verified
- Offline capture with `--preview-call-screen`, before (bottom cut) and after
  (whole screen, side bars).
- Compile, clippy, `twayblade` tests.

## Not verified
- A live call with Element Call sharing a screen; camera tiles in a live call.
