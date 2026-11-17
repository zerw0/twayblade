# Images show the picture, not the filename (#198)

## Built
- An image message no longer has a "filename · size" text line: the picture
  stands alone. The filename stays in the accessible label ("Open image …")
  and the viewer.
- A caption is shown as text: core now keeps an `m.image`'s caption (ruma
  `caption()`, the body when it differs from the filename) as the message
  body; before, image bodies were dropped.
- While the preview is downloading, or when there is none, a placeholder of
  the preview size (280×180, or 560×420 for large previews) with an image icon
  holds the picture's place, instead of an "Open Image" button. It still opens
  the viewer. "Load Image" (automatic download off, #174) is unchanged.
- Offline UI tests open a taller preview window (920 px), since the sample
  conversation's image now takes its full height.

## Verified
- Compile, clippy, `twayblade` and `twayblade-core` tests; offline preview
  capture of Bubbles with the placeholder.

## Not verified
- Live account with real image previews and captions (incl. from Element).
- Linux, Windows.
