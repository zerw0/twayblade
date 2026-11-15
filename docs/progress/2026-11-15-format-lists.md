# Lists in the Format menu

Lists in the Format menu (2026-11-15, `feature/camera-linux-windows`, issue #39).

Implemented: the composer's right-click Format submenu gains Bulleted List and
Numbered List after a separator, in both the message and thread reply fields.
They act on whole selected lines like Quote: Bulleted List prefixes `- `,
Numbered List prefixes `1. `, `2. `, … Applying the same kind again removes the
markers; applying the other kind switches them (`*` bullets count as bullets).
A caret keeps its position in the line's text. Shortcuts: Cmd-Shift-8 and
Cmd-Shift-7 on macOS (bound to the typed `*` and `&`, as Quote is bound to
`>`), Ctrl-Shift-8 and Ctrl-Shift-7 elsewhere; they appear in Settings >
Keyboard. The Markdown goes through the existing core compose path, so the
message carries a `formatted_body` list with the Markdown as plain fallback.

Verified: compilation, strict clippy, unit tests of the line transform (toggle,
switch, caret, Unicode round trips, grapheme-safe selections) and that the
result composes to a `formatted_body`. Not verified: the shortcuts on a
physical keyboard (the macOS typed-character binding is untested by hand), the
native right-click menu on screen, how Element renders the sent list, Linux
and Windows. #39 stays open for that live check.
