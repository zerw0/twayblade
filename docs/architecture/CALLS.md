# Native MatrixRTC calls

Decision: 2026-11-11. The technical layer uses Matrix SDK signalling and
LiveKit/libwebrtc media. Claude owns the GPUI views. Screen sharing and ringing
are later work. This document describes the native implementation; acceptance
levels are recorded in `PLAN.md`.

## Protocol and compatibility target

The fixture pins Element Call **0.28.2**, image digest
`sha256:c61fc9e4e04f75690c693d4556d01c09a2b9246f3fc8d35222d3aac36ba29906`,
and its Matrix JS SDK commit `3f05dc4d104a3dfc753c5e3549a18c74fab4b076`.
This release was checked on 2026-11-11. Its `Compatibility` mode uses state
memberships and the HKDF/window-10/ring-256 media key provider exercised here.
The fixture explicitly selects that mode. Current Element Call also supports
MatrixRTC 2.0 sticky events, authenticated MSC4143 transport discovery and
hashed media identities; those paths are not implemented here. This phase
selects one shared focus, rather than implementing multi-SFU migration.
Legacy `m.call.*` signalling is a separate compatibility target, outside 1.0.

| Proposal | Targeted wire identifiers |
| --- | --- |
| MSC3401 / MSC4143 session membership | `org.matrix.msc3401.call.member`; `application: m.call`, `scope: m.room`, empty `call_id`; state key `_USER_DEVICE_m.call` on ordinary room versions |
| MSC3757 / MSC3779 room versions | Same state key without its leading underscore |
| MSC4143 focus discovery | SDK `.well-known` discovery of `org.matrix.msc4143.rtc_foci`; LiveKit `livekit_service_url` |
| Session focus | `foci_preferred` with `livekit_alias`; `focus_active: {type: livekit, focus_selection: oldest_membership}` |
| MSC4195 deployment compatibility | Existing lk-jwt-service `/sfu/get`: OpenID assertion, room alias and device ID → WSS SFU URL and JWT |
| MSC4140 delayed events | Ruma's Synapse-compatible delayed state endpoint with `org.matrix.msc4140.delay`; update/restart/cancel through its unstable delayed-event API |
| Media keys, as used by Element Call 0.28.2 | `io.element.call.encryption_keys`, only through SDK Olm-encrypted to-device sending |

The new MSC4195 `get_token` endpoint and new MSC4140 scheduling revision are
not substituted for these pinned deployment APIs. The SDK/Ruma source identifies
its delayed-state implementation with MSC4140 revision `3ee73abe5f81252b00877cfb5db941ee9aa6c18d`.

`core::calls::native` uses SDK/Ruma membership parsing and expiry calculations.
Only joined room members in the empty room-call slot are admitted. The oldest
active membership selects the service and alias; local discovery starts an
empty call. An existing call without a usable focus fails instead of splitting
the call across services. Service URLs must be HTTPS without userinfo, query
credentials or fragments; the returned SFU URL must be WSS with the same rules.
Ordinary members need permission to write the membership state event; the
fixture grants only that event type power level zero.

Membership expires after two minutes without refresh. The original creation
time is retained and expiry is extended before a minute remains. A delayed
empty state event is scheduled for 15 seconds and restarted every five seconds.
After refreshing membership, restart the existing delayed-leave ID. Only
schedule a replacement if the server reports that ID was removed. Servers
that retain it must not get a second orphaned delayed leave. Servers rejecting
scheduling use bounded membership expiry. Failure to restart an accepted delayed leave
ends the native session. Drop/cancellation also attempts a bounded empty state
write. Account reservations remain held during that cleanup.

## Keys and trust

Generate independent 16-byte sender keys with the OS CSPRNG (`getrandom`).
Application-owned material is zeroized on drop. Send only to exact current
call `(user_id, device_id)` pairs, never wildcard devices or every room member.
The SDK performs device lookup, Olm session establishment and encryption.
Periodic retries handle key queries/delivery that have not completed yet.
Membership-change notifications trigger rotation when any participant device
leaves. The new sender index is selected even if delivery to a remaining device
is retrying; there is no fallback to the departed participant's key.

Incoming keys need SDK decryption metadata identifying the Olm sender device,
matching event sender/device claims, current joined-room/call membership and
16-byte material. Clear events, missing authenticated devices, forwarded keys,
other rooms/calls, expired membership and oversized messages are rejected.
Current messages carry `media_key`, `member_id` and
`member.claimed_device_id`; deprecated `keys` and `member.id` remain accepted.
The explicit member ID must match the current state-mode `user:device` ID.
Current Element Call omits `session` and `sent_ts`: SDK Olm replay protection
and current membership remain mandatory. When those legacy fields are present,
validate room-call application/scope/ID and timestamp age/skew, including the
membership's creation time. An older transport's opaque `member.id` UUID is
accepted only with its legacy session binding and without a conflicting modern
`member_id`. Compatibility parsing of that older schema does not establish
interop with its whole client. Both padded and unpadded standard base64 work.
No Olm or frame cryptography is implemented locally.

Primary implementation references:
[ToDeviceKeyTransport](https://github.com/matrix-org/matrix-js-sdk/blob/3f05dc4d104a3dfc753c5e3549a18c74fab4b076/src/matrixrtc/ToDeviceKeyTransport.ts),
[MatrixKeyProvider](https://github.com/element-hq/element-call/blob/v0.28.2/sdk/src/media-backend/livekit/MatrixKeyProvider.ts).

Encrypted room joins require media E2EE. There is no clear-media fallback.
SFU credentials and keys have no Debug representation and never enter UI
snapshots. JWT exchange uses a separate HTTP client with the account's explicit
root certificate, no Matrix access token, no redirects and an HTTP deadline.
LiveKit WebSockets reuse that trust through a scoped native transport; fixture
CA trust does not change the keychain or disable certificate validation.
The SDK WebSocket registry is process-wide, so each guarded binding matches
both SFU origin and the exact SDK bearer token (including server-refreshed
room tokens). Two accounts on the same SFU cannot borrow each other's roots.
The transport registry is bounded to eight active bindings. Tokens stay in
memory; requests and failures are never formatted into logs.

## Native media and licensing

A separate `twayblade-media` crate keeps libwebrtc, AVFoundation capture and
frame conversion independent of Matrix state and GPUI. Pins:

- `livekit = 0.10.1` (`88705b15086ecbe2c26ee91ede6203f974a0fc62`), Apache-2.0.
- `libwebrtc = 0.4.0`, `webrtc-sys = 0.3.53`, Apache-2.0 Rust bindings.
- `webrtc-sys-build = 0.3.21`, prebuilt `webrtc-a545de4`,
  `mac-arm64-release` on this Mac. WebRTC is BSD-3-Clause; its bundled dependency
  notices are preserved in `crates/media/licenses/libwebrtc-a545de4.md`.

Final macOS binaries need `-ObjC` to retain libwebrtc's static Objective-C
categories. App, core-test and media-test build scripts apply it. Library
consumers must also apply it to their final executable.

The per-participant LiveKit key provider explicitly uses HKDF, salt
`LKFrameEncryptionKey`, ratchet window 10, key ring 256, failure tolerance -1,
and AES-GCM-128 frame encryption. PBKDF2 defaults are not used. These match the
pinned [Element Call MatrixKeyProvider](https://github.com/element-hq/element-call/blob/v0.28.2/sdk/src/media-backend/livekit/MatrixKeyProvider.ts).
Publishing initializes the frame cryptor and current sender index before
enabling capture content. Message E2EE and media E2EE have separate evidence.

Microphone/playout use libwebrtc's PlatformAudio ADM. Stable device IDs are
validated against a fresh enumeration before switching. Device lists refresh
with session updates/keepalives; device-removal recovery is still an acceptance
gap. `None` for the selected input/output means the system default. Camera
capture uses AVFoundation on a dedicated worker, a serial callback queue,
`alwaysDiscardsLateVideoFrames`, bounded pixel dimensions and a native SDK video
source. Camera-off stops capture; switching replaces the source. The platform
crate owns microphone/camera prompts and typed denied/restricted errors.
`macos/Info.plist` declares both usage descriptions. `CaptureSource` leaves the
screen-capture boundary available for a later phase.

## App contract and threading

`NativeCall::start(...).drive()` runs a continuous core task; consumers observe
`CallHandle.updates`. Watching updates cannot cancel signalling transactions.
One semaphore reservation per account prevents concurrent native calls. The
backend's account/request ticket gates admission and snapshot publication.
Logout/account replacement/backend shutdown, explicit leave and loss of room
membership end the call. Controls have bounded queues; confirmed state follows
the native operation, while asynchronous errors appear in `CallState.error`.

`Snapshot.call` carries:

- `room_id`, direct/group `kind`, `phase`, optional typed `error`;
- `participants[]`: `user_id`, `device_id`, optional `display_name`, `speaking`,
  `muted`, `video`, `connection_quality` (Excellent/Good/Poor/Lost);
- `local`: `muted`, `video`, optional `input_device`, `output_device`, `camera`;
- `devices`: `inputs`, `outputs`, `cameras`, each containing stable `id`/`name`;
- `video_tracks[]`: track `id`, owning `user_id`/`device_id`, `local`;
- the earlier top-level optional `muted`, `video`, `screen_sharing` fields remain
  for contract compatibility. Screen sharing is false in this phase.

`Snapshot.room_calls` remains SDK-derived room activity, with device-level
participants and optional earliest membership time. Membership alone does not
invent mute/video/speaking/quality values.

`MatrixApp` exposes `start_call`, `join_call`, `leave_call`, `set_muted`,
`set_video`, `select_input(id)`, `select_output(id)`, `select_camera(id)` and
`call_video_handle(track_id)`. These are technical contracts, without call views.
A control action accepts a queue request; the next confirmed snapshot reports
its result.

No video frames enter `Snapshot`. A `LatestFrame` handle has one replaceable
`Arc<VideoFrame>` slot, permanently retired on track/call removal so late
decoder writes cannot revive it. Camera switches cancel the previous consumer. The UI reads `latest()` on
its render cadence and retains at most the frame being rendered. The format is
tightly packed opaque BGRA8, top-to-bottom, stride `width * 4`, with clockwise
rotation degrees and a microsecond timestamp. The decoder consumer makes one
BGRA conversion outside GPUI; snapshots copy only track metadata. There is no
CVPixelBuffer/GPUI surface implementation claimed in this phase.

Native SDK video queues hold one latest frame; audio queues hold ten 10-ms
frames. App media events are capped at 64, controls and incoming keys at 32.
Exhausted media-event delivery closes the session. The SDK's own event source
is unbounded internally and is drained by a dedicated task; replacing SDK
internals is not claimed.

## Isolated verification

Run within `nix develop`:

```sh
npm ci --prefix tests/integration/browser --ignore-scripts
PLAYWRIGHT_BROWSERS_PATH=target/playwright tests/integration/browser/node_modules/.bin/playwright install chromium
bash tests/integration/run.sh call
```

The fixture retains Synapse, digest-pinned LiveKit 1.9.0 and lk-jwt-service 0.9.0,
and serves a digest-pinned Element Call SPA to Playwright 1.56.1 / Chromium 1194
in a temporary headless profile. No embedded widget driver is used. Browser
trust is restricted to the fixture certificate's SPKI; native trust uses the
account CA option. Fake capture supplies explicit generated PCM tone and YUV
video patterns, not physical microphones/cameras. Output contains assertion
stages and counts only. The prior signalling branch supplied fixture service
configuration, certificate-trust checks, LiveKit server evidence and
membership/key-exchange assertions; no WebKit/network-policy/CSP code is reused.

Final validation on 2026-11-11: pinned-shell formatting, strict locked workspace
all-target/all-feature Clippy and all-feature tests passed (141 tests, 24 opt-in
service tests skipped). `run.sh call` passed both cases in 141 seconds. The
native debug app bundle built and was signed ad hoc; this is compilation/packaging
acceptance, separate from physical call operation.

The isolated macOS tests assert both membership/participant lists, encrypted
key receipt counts in both directions and exact LiveKit identities (two
participants). Audio assertions count decoded non-silent generated-tone blocks
on each side (native RMS and browser AudioWorklet); video assertions count
new decoded test-pattern frames (native stream and browser video-frame callback).
They cover native/Element Call and native/native. The sustained native case
also spans a membership expiry refresh and delayed-leave renewal.
These synthetic checks do not establish physical capture/playout acceptance.

Acceptance still requires a person to test actual microphone/speaker/camera
permissions and media on this Mac. Public-server Element Web interoperability,
Linux and Windows are not attempted. Screen sharing, MSC4075 ringing and
notifications, device removal and network changes remain open M5 work.
