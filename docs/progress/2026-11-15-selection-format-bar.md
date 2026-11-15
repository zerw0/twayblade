# Formatting bar over selected text (#89)

Branch `feature/selection-format-bar`.

## Behaviour

- Selecting text in the room or thread composer shows a small bar above the
  start of the selection with Bold, Italic, Code, Link, Quote, Bulleted list
  and Numbered list (Lucide icons, tooltips with the shortcuts).
- A press applies the same Markdown edit as the shortcut and the right-click
  menu. It is handled on mouse down and stopped there, so the composer keeps
  focus and the (now wrapped) selection: formats can be stacked.
- The bar disappears when the selection collapses (typing, clicking, arrow
  keys) or the composer loses focus. It rises in over 200 ms, instantly under
  Reduce Motion.
- It is its own small view that observes one composer, so caret and selection
  changes re-render only the bar. It follows the room's composer when rooms
  change, and the thread composer separately.
- The controls are not in the Tab order (the composer must keep focus);
  keyboard users have the shortcuts and the context menu. Each control has an
  accessible label and the bar is a labelled toolbar.
- Preview: `--preview-selection`.

## Verification

- Compile: formatting and strict Clippy.
- Offline tests: 154 app tests, including one that selects text, finds the
  bar, presses Bold, and checks the Markdown, the composer's focus and the
  kept selection, then that collapsing the selection hides the bar.
- Capture of the preview reviewed (the bar above "the release").
- Not verified: mouse-drag selection on a physical Mac, multi-line selections,
  VoiceOver, Linux and Windows.
