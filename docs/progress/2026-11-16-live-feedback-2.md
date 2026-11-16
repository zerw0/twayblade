# Offline notice on Windows (#109)

## Behaviour

`twayblade_platform::network::available()` now has a Windows implementation,
so the offline notice there appears within the 2 s polling interval instead of
waiting for sync's long-poll timeout. Windows had no check before this change.

The check finds a default route (IPv4 or IPv6, prefix length 0) with
`GetIpForwardTable2`. That route only counts if `GetIfEntry2` reports its
interface as up, with the medium connected, and not a software loopback. It
asks the OS only: no traffic, no hostnames. If the OS call fails, the check
answers "available" and sync's own timeout decides.

Known limit: a full-tunnel VPN adapter that stays up keeps a default route, so
with every physical link down the notice still waits for sync. The macOS check
has the same limit.

## Verification

- Compile on macOS: `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
  The Windows code is not compiled on macOS. Only the `check (windows-2022)` CI
  job compiles it.
- Unit tests: the `twayblade-platform` network tests pass on macOS.

## Not verified

- Physical Windows: Wi-Fi and cable off, then on again; queued messages send
  on reconnect; sleep/wake.
- VPN and Hyper-V virtual switch behaviour.

# Membership lines start at the message text (#104 follow-up)

## Behaviour

Membership lines ("Fabian joined", "Fabian left") were centred. The owner
asked during live acceptance for them to sit on the left. They now start at the
same x as message text in each chat style:
- Minimal: after the avatar gutter (32 + 10 px; 0 without avatars).
- IRC: after the time and name columns (178 px).
- Bubbles: at the sender-name inset (26 + 8 + 10 px; 10 without avatars).

Also restored the doc comment on `accessible_message`, which #106 had attached
to `delivery_opacity`.

## Verification

- Offline captures of all three styles: in each one, "Robin joined" lines up
  with the message text.
- `cargo test -p twayblade --all-features --locked`: 179 passed.
- Not verified: live account on the physical Mac.

# Switching back to an account reopens its last room (#82)

## Behaviour

Live acceptance found that switching accounts and back opened no room. The
window reset its navigation on every account change. It now remembers each
account's last space and room (by user ID, for the life of the window). When
the account becomes active again and its room list contains that room, the
room reopens. Choosing a room yourself, or a notification click, takes
precedence.

The same acceptance pass showed that clicking a notification started a
second instance. The cause was the development setup: a debug and a release
development bundle shared one bundle ID, and macOS launched the other copy.
The stale debug bundle was removed. This still needs a recheck with one
bundle.

## Verification

- `switching_back_to_an_account_reopens_its_last_room` fails without the
  change and passes with it. `cargo test -p twayblade --all-features --locked`:
  180 passed.
- Not verified: two live accounts on the physical Mac.
