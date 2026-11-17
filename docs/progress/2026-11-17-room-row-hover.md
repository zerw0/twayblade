# Room row hover animation (#179)

## Built

- Sidebar room rows fade their hover fill in and out over 150 ms with the
  app's ease-out curve, instead of switching it on and off. The fill colour
  is unchanged (the GPUI Kit ghost hover colour); pressing still shows it at
  once, and the selected row keeps its selection colour without a hover fill.
- Only the row under the pointer and the row it just left animate
  (`hovered_room` / `unhovered_room`), so a long list does not start an
  animation per row.
- Reduce Motion: the fill appears and disappears without a transition.
- Shared helper: `motion::hover_fill`.

## Verified

- Compile: strict workspace Clippy. Offline tests: a UI test hovers one row,
  then another, and checks which row fades in and which fades out.

## Not verified

- The motion itself on a physical Mac (tests run with motion settled; the
  pointer cannot be driven in captures).
