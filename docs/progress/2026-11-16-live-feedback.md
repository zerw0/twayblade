# Feedback from the live check: offline notice, membership lines (#105, #104)

## Offline notice (#105)

- Cause: with Wi-Fi off, sync requests hang rather than fail. The local fault
  proxy in `stall` mode (new test `room_list_reports_offline_when_requests_hang`)
  shows the room list reporting Offline 30 s after the network goes, when the
  long poll times out.
- Likely also in the report: this Mac has a wired link (`en5`) on the same
  network as Wi-Fi, so turning Wi-Fi off alone keeps it online.
- New `platform::network`: every 2 s it checks whether the machine has a route
  to the internet. On macOS that is the system reachability for "any address"
  (`SCNetworkReachability`); on Linux, a default route in `/proc/net/route` or
  `ipv6_route`. It does not count interface addresses, because VM bridges and
  VPN tunnels keep them when every real link is down. Windows has no check
  yet, so sync's own timeout applies there.
- The strip "No internet connection. Showing saved messages." shows when
  either sync or this check says offline.

## Membership changes in the timeline (#104)

- Core used to drop every timeline item that is not a message. Membership
  changes are now `MessageKind::Membership` with a typed `Membership` (kind,
  target). Unknown and no-op changes stay hidden.
- They show as a quiet centred line, worded in the frontend: "Robin joined",
  "You invited Sam", "Alex removed you", and so on. Consecutive changes group
  together whoever made them, never with messages, and have no avatar or name
  header.
- The latest-event visibility check, which sends the read receipt, now also
  covers these lines, so a room ending in a membership change still gets
  marked read.
- Keyboard navigation (↑/↓, F6) skips them.

## Verification

- Compile: `cargo fmt --check` and strict Clippy pass (macOS host).
- Offline tests: 178 app tests and the core and platform tests pass. New
  ones cover membership wording, grouping, and route-table parsing.
- Local Synapse with the fault proxy: `room_list_reports_offline_when_requests_hang`
  passes; Offline is reported after 30 s of hanging requests and the list
  reconnects.
- Capture of the sample timeline with "Robin joined" reviewed.

## Not verified

- The new reachability check on a real network loss: Wi-Fi and cable both
  off, on the physical Mac.
- Linux route detection on a real machine.
- Membership lines with the live account.
- The large empty space below the last message (#103): not reproduced, so
  no change here.
