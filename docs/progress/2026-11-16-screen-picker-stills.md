# Stills in the screen and window chooser (#47)

## Built

- Where the OS has no picker (X11, Windows), the in-app chooser now shows a
  small still of each screen and window next to its name, in a 64×40 frame
  (16:10, contain). Sources without a still keep the screen or window icon in
  the same frame, so rows line up.
- `media::screen::sources` grabs the stills after listing: one short-lived
  capturer per source (a started capturer can't switch sources), the first
  frame within 400 ms, for the first 16 sources and at most 3 s in all. Frames
  are box-averaged to fit 192×120 BGRA and made opaque. The list query's
  timeout grows by that budget.
- Stills stay in memory only and are left out of `Debug` output, like window
  titles. The view uploads each still once and releases the GPU image when
  the source changes, the tab switches or the chooser closes.
- Preview: `--preview-call-picker` (generated stills; one window has none).

## Verified

- Compile: `cargo fmt --check` and strict all-target, all-feature Clippy pass
  (macOS host).
- Offline tests: downscaling (fit, aspect, colour averaging, padded strides,
  no enlarging, short buffers refused); `Debug` output leaks neither titles
  nor pixels; the chooser UI test checks two screen stills, one window still
  after switching tabs, and none once closed.
- Capture of the preview reviewed.

## Not verified

- Real stills on X11 and Windows: the capture path does not run on macOS.
  On Windows, Graphics Capture may flash its border on each window while the
  stills are taken; minimised windows give no frame and keep the icon.
- How long the stills take with many windows on a physical machine.
- Orca and Narrator.
