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
