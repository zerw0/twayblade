# Chat feedback from live use (#114–#117)

All four came from the owner's live use on the physical Mac on 2026-11-16.

## Less space between senders (#114)

In the Minimal and IRC styles every group already shows its time beside the
sender, so the centred pause marker (for example "18:50") only repeated it in a
tall row of its own. These two styles no longer show the marker. Bubbles keeps
it, because a bubble shows no time. Membership lines in Minimal and IRC show
their own time ("Fabian joined 09:06") and use less vertical padding.

## Mentions open the profile (#115)

A mention links to `https://matrix.to/#/@user:server` (sometimes
percent-encoded, sometimes with `?via=`) or to `matrix:u/user:server`. Clicking
one now opens that person's profile in the side panel, named as in the room's
member list. Room, event and other links behave as before, and web links open
in the browser. `mentioned_user` has unit tests for the accepted and rejected
forms.

## Delete by emptying an edit (#116)

When you edit a message, remove all of its text and press Return, the edit bar
asks: "Delete this message for everyone? Return to delete, Escape to keep it",
and shows a Delete button. Return or Delete then redacts the message with the
SDK timeline (`Conversation::delete` in core). Escape cancels the edit. Typing
text again withdraws the question, so emptying the field later asks again. A
failed deletion shows "The message could not be deleted. Try again."

## Thread panel width and reply field (#117)

The thread panel set `max_w(relative(0.36))`. Inside the 360 px panel region
that resolved to about 130 px, so replies wrapped one character per line. The
panel is now 360 px and never wider than its region, which caps it at 32% of
the window. The reply field uses the main composer's box and send button.

## Verification

- Compile: `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- Offline UI tests: `cargo test -p twayblade --all-features --locked`, 183
  passed. Core library tests pass.
  - `the_thread_panel_fits_its_region` fails both with the old percentage cap
    and without the region cap.
  - The editing test now covers the delete question, withdrawing it by typing,
    and confirming it.
- Offline capture (Minimal): no centred pause markers. The membership line
  shows its time.
- Physical Mac: release builds are running, and the owner's screenshots led to
  the thread fixes.

## Not verified

- Deleting a message on a live homeserver, and how it shows in Element.
- Clicking a mention pill on the live account.
- Attachments in thread replies: the thread composer has no attach button yet
  (#118).
- Linux and Windows.
