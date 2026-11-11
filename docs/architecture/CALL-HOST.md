# macOS call-runtime spike

Status: experimental M5 base; an encrypted call is not yet accepted. No call
views are implemented here. Ringing, incoming notifications and legacy
`m.call.*` signalling are out of scope.

## Process boundary

```text
GPUI app process                     separate helper .app
  Account / SDK encrypted store        Tao main thread / Wry / WKWebView
  WidgetDriver per selected room  <->   Element Call embedded assets
             private framed stdin/stdout pipes
```

`twayblade-call-host` is a separate executable. It has no Matrix SDK, credentials,
store, GPUI or authentication code. The parent clears inherited environment
variables and pipes standard input/output; standard error is discarded. The
macOS helper bundle has its own camera/microphone usage declarations and
standard window decorations. Its ephemeral webview serves local assets from
`twayblade-call://localhost`. Navigation, popups and downloads outside the
configured application are denied; frames are disabled. Release devtools are
disabled. Other web permission kinds are denied; camera/microphone use WebKit
prompts. Screen capture still needs physical OS permission testing.

The protocol uses a four-byte big-endian length and JSON, capped at 256 KiB
before allocation. Process queues hold 32 frames; overflow fails closed.
Health requests are answered by the webview main loop every two seconds; an
unanswered probe ends the process after approximately eight to ten seconds.
The owner kills and reaps the child on shutdown, reports clean/crashed/hung/
protocol exits, and uses kill-on-drop on cancellation. The asynchronous frame
reader retains partially received frames across health-check ticks.

Call discovery reserves an account instance/request ticket before dispatch.
Admission is rechecked after discovery and under the application state lock
immediately before spawn; publication checks the same ticket. One active call
intent is admitted per account. Logout, account replacement, backend shutdown
and loss of joined membership cancel the call task. Room selection alone does
not end a call. Child termination on task cancellation is asynchronous.

## SDK bridge

`core::calls::Account::call_widget` requires a joined, encrypted room, selects
an HTTPS LiveKit service from the SDK's well-known RTC foci and creates the
SDK's virtual Element Call widget with per-participant encryption. No configured
fallback SFU is accepted. The embedded client's well-known fetch receives only
that selected focus from initialization data; its Matrix operations use the SDK
widget API. No Matrix access/refresh token is sent to the helper or put in a URL.
The helper necessarily sees the widget's short-lived OpenID assertion and
per-call encryption keys. These are not Matrix session credentials and must
also never be logged. Console output and IPC payload logging are disabled.

The capability provider grants only this room's call membership reads,
membership writes under the current user's/device's state keys, encryption-key
room/to-device events, delayed membership maintenance, empty-key room
name/create/encryption metadata and the current user's membership. Timeline
messages, notifications/ringing, legacy call signalling, media downloads,
reactions, redactions and rageshakes are denied. Broad profile requests are
narrowed to the current user. OpenID is handled by the SDK's widget identity
operation. No capability/event content is logged.

SDK 0.19.1's to-device handler is account-wide: the parent additionally drops
incoming keys for other rooms or without the SDK's verified encrypted flag.
Outgoing keys require this room ID and recipients who are joined members in
the SDK store. Request/response IDs and outstanding requests are bounded at the
application bridge. The SDK itself still uses internal unbounded channels;
their replacement/bounding is upstream work, so there is no claim that all SDK
internal queues are bounded.

## Contracts for the UI contributor

`Snapshot.call: Option<CallState>` contains:

- `room_id: String`, `kind: Direct | Group` (resolved before runtime startup).
- `phase: Starting | Joining | Connected | Ended(EndReason)`; `Joining` means
  webview creation, and `Connected` means Element Call reported
  `io.element.join`. Neither establishes functioning or encrypted media.
- `participants: Vec<Participant>` with `user_id`, optional `display_name`,
  optional `muted` and `video`.
- `muted`, `video`, `screen_sharing`: `Option<bool>`; `error: Option<CallError>`.

`Snapshot.room_calls: HashMap<String, ActiveCall>` projects nonexpired SDK
MatrixRTC memberships across joined rooms. `ActiveCall` contains participants
and `started_ms: Option<u64>`, the earliest available active membership timestamp.
Participant lists deduplicate users across devices. Membership expiry is
reprojected once per second, including without a new room selection.

`MatrixApp::{start_call(room_id,video), join_call(room_id), leave_call(),
set_muted(bool), set_video(bool)}` return typed call errors. Start/join share the
only runtime spawn path; controls never start a process. `io.element.device_mute`
requests update state only after a matching response or widget notification.
Unknown values deliberately use `None` instead of pretending a device is muted,
video is off, or screen sharing has stopped. This is a deliberate refinement of
the proposed bool-only contract. Screen-sharing status has no widget action in
this pin. Claude owns action/view integration and user-facing wording.

## Pinned assets and builds

Element Call **0.17.0**, NPM `@element-hq/element-call-embedded`, archive:
`https://registry.npmjs.org/@element-hq/element-call-embedded/-/element-call-embedded-0.17.0.tgz`.
SHA-256: `ba0f3f3b04b62577587faed603c3768465b1f9db33b15583473265531ca00121`.
`crates/call-host/assets.manifest.json` pins the 154 individual dist assets.
`prepare-assets.py` verifies the archive and all assets, and preserves the
upstream AGPL/commercial license files. Third-party licenses remain separate
from Twayblade's MIT license. No generated vendor build is tracked.

0.28.2 was evaluated but requests newer widget operations absent from SDK
0.19.1. The 0.17.0 pin exposes device mute controls; its full compatibility with
the restrictive grants still requires the isolated signalling test to pass.
Wry 0.57.2 and Tao 0.37.2 are exact pins in Cargo.lock.

Inside `nix develop`, use `bash macos/build-call-host.sh` for just the ad hoc
signed helper; `bash macos/build.sh` embeds and signs it inside Twayblade.app.
The supervisor prefers the packaged helper. Launching an unbundled binary is
insufficient: this Mac exposed no `navigator.mediaDevices` without bundle usage
declarations, while the declared, signed helper exposed it.

## Isolation limits and acceptance gates

The CSP allows HTTP/WebSocket connections only to the selected JWT service
origin, plus local assets. The initialization bridge also rejects network URLs
containing credential parameters. In particular, a LiveKit client that puts its
SFU JWT in a WebSocket URL will be rejected: the no-tokens-in-URLs requirement
must be solved with an authenticated native transport or compatible upstream
transport, not by weakening the policy. A JWT response pointing to another
SFU origin is also rejected; trust is not expanded from a remote response.

CSP is not an OS network sandbox and does not constrain WebRTC ICE traffic.
The requested restriction of every outbound connection to the discovered SFU
is therefore **not established** by this spike. This is a production blocker,
along with the JWT URL transport issue and isolated call acceptance. The helper
has process separation, but no claim of complete operating-system confinement.

The TLS fixture trusts a generated CA only in the SDK. It never installs a CA
into the Mac's trust store or bypasses WebKit TLS validation. WKWebView trust of
the local service remains a separate acceptance problem. No fake WebKit media
devices are configured. The automated call scenario starts both inputs muted;
it must observe real widget joins, MatrixRTC memberships, encryption-key
reception and both participant lists, never synthetic membership/key events.

Physical acceptance requires a person: camera/microphone grant/deny, two call
windows, real audio/video, actual media E2EE, device removal, network changes
and screen sharing. Element Web interoperability is separate. Linux
WebKitGTK/Wayland/X11 and Windows WebView2 builds/media/permissions have not been
attempted. No milestone exit is claimed.
