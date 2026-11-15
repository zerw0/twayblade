# Multiple accounts, phase 2: background accounts (#97)

Branch `feature/background-accounts`, based on `feature/multiple-accounts`
(#86, phase 1), since this builds on its saved-account index and switching.

## Behaviour

- **Background sync.** Every saved account other than the open one is
  opened in the background after the open account connects. Each keeps its
  room-list sync running with no room selected: no timeline, security,
  presence, typing or call work. A task restores them while holding the
  background lock, and reads the open account only once that lock is held.
  A switch waits for it, so two clients never open the same store.
- **Switching** promotes the background client to the open one instead of
  opening its store again, and demotes the previous open account to the
  background. A signed-out or revoked account is not demoted. The client and
  its `instance_id` survive the switch. Sign-out opens the next account the
  same way.
- **Notifications from inactive accounts** use the same OS notifications,
  preferences and preview setting as the open account. Muted rooms of that
  account are skipped. Nothing in an inactive account counts as focused.
  Clicking one switches to the account, waits until its room is listed, then
  opens the room, and the thread when the notification came from one. The
  click is forgotten if the switch fails or ends on another account.
- **Unread per account.** Each saved account entry carries its unread count
  (muted rooms and spaces excluded) and its mentions. Counts survive switches
  and are published only when they change. The Dock badge adds the other
  accounts' unread to the open one's.

## UI

- Avatar menu: each account that is not open shows a count after its ID. The
  accent marks mentions; plain unread uses the quiet badge, as in the
  sidebar. The open account shows none, since its counts are in the sidebar.
  The row's accessible name says "N mentions" or "N unread".
- The avatar gets a small dot when other accounts have something new: accent
  for mentions, neutral otherwise. Its accessible name adds "N mentions in
  other accounts" or "N unread in other accounts".
- `--preview-accounts-menu` opens the avatar menu for captures (capture
  builds only).

## Verification

- Compile: `cargo fmt --check` and strict all-target, all-feature Clippy pass
  (macOS host).
- Offline tests (160 app tests):
  - Counts exclude muted rooms and spaces, unchanged counts do not publish,
    and removed accounts are ignored.
  - Row labels announce mentions before unread, and the open account shows
    no count.
  - A notification from another account waits for the switch, opens the
    room once it lands, is ignored for an unknown account, and is forgotten
    when the switch ends elsewhere.
- Local Synapse (`run.sh inactive-accounts`, new). Alice and Bob sync side by
  side, Bob with no room selected:
  - Bob's room list reports the unread message.
  - Bob's notification stream delivers it.
  - After Bob's room list is stopped and started again on the same client (a
    switch), it reconnects and counts the next message.
- Capture of the avatar menu with an inactive account's mention count, and
  the avatar dot, reviewed.

## Not verified

- The running app with two real accounts: background restore at launch,
  switching both ways, and a notification click from the inactive account.
  Needs a live check with two isolated accounts.
- Incoming calls on an inactive account do not notify yet. Calls stay with
  the open account (#97 lists this).
- A cold launch from an inactive account's notification opens the launch
  account, not that one. These notifications carry no launch target.
- Memory and socket cost per extra account was not measured.
- Linux and Windows notifications and keychains, and Element interop, were
  not checked.
