# Short timelines rest against the composer (#103)

## Behaviour

A conversation shorter than the window used to sit at the top of the timeline
with an empty band between the latest message and the composer. GPUI Kit's
`MessageScroller` (0.9.1) creates its list with `ListAlignment::Top` and does
not expose the list state, so the app now carries an adapted copy in
`crates/app/src/ui/message_scroller.rs` (Apache-2.0, see
`vendor/LICENSE-APACHE`) that uses `ListAlignment::Bottom`. Spare room now sits
above the first message, as in Element and other chat clients. The thread panel
uses the same component and behaves the same way.

The copy keeps the parts Twayblade uses: tail following, splice/remeasure,
`scroll_to_item`, the scrollbar, scroll bounce and the jump-to-latest button.
The unused bottom fade and style hooks were dropped.

In a bottom-aligned list that fits the window there is no reading anchor;
`logical_scroll_top()` reports the live edge. The pagination-anchor test now
uses a window short enough for the sample timeline to scroll, and the
style-switching test no longer asserts an anchor in a window where everything
fits (it still checks that every message stays visible).

## Verification

- Compile: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- Offline UI tests: `cargo test -p twayblade --all-features --locked` (177
  passed). New `short_timeline_rests_against_the_composer` fails with top
  alignment (82 px above the first message, 385 px below the last) and passes
  with bottom alignment.
- Offline capture of the default preview: the sample timeline still fills the
  window and ends at the composer. The preview cannot show a short timeline.

## Not verified

- Live account: the short direct-message room from the #103 report on the
  physical Mac.
- Back-pagination and unread jumps in long live rooms after the alignment
  change.
- Linux and Windows.
