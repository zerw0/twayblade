# Native MatrixRTC calls

Decision: 2026-11-11. The technical layer uses Matrix SDK signalling and
LiveKit/libwebrtc media. Claude owns the GPUI views. Screen sharing remains later work; phase 2a
ringing and resilience contracts are described below. This document describes the native implementation; acceptance
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
Keys are sent when call membership changes and retried on the 5-second
keepalive only after a failed or timed-out delivery, never on every tick.
Membership-change notifications trigger rotation when any participant device
leaves. The new key is installed and distributed first; the sender switches to
its index after 5 seconds (matrix-js-sdk `useKeyDelay`, as in Element Call
0.28.2), so remaining members don't drop audio while it arrives. The departed
member is already unsubscribed (see Reception), so the delay only means they
could decrypt our media for those seconds.

## Reception

Media subscribes only to current, unexpired call members' tracks, re-evaluated
on every membership change; being in the LiveKit room is not enough. When a
member departs, their tracks are unsubscribed, consumers and video slots
cleared, and every key index installed for them is overwritten with random
material, so frames under old keys no longer decrypt. A publication without
AES-GCM frame encryption is never subscribed: the publisher is marked
`media_blocked` in the participant contract and the call continues for everyone
else. Our own media is never published unencrypted.

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
Remote audio plays through the platform audio device module. Frame-counting
sinks (decoded audio/video evidence) exist only with the media crate's
`evidence` feature, which core enables for its tests; release builds attach no
extra sink per remote track.

Microphone/playout use libwebrtc's PlatformAudio ADM. Stable device IDs are
validated against a fresh enumeration before switching. Device lists refresh
with session updates and phase 2a hot-plug listeners; physical device-removal
acceptance remains open. `None` for the selected input/output retains the SDK's
own choice without claiming a resolved OS device ID. Camera
capture uses AVFoundation on a dedicated worker, a serial callback queue,
`alwaysDiscardsLateVideoFrames`, bounded pixel dimensions and a native SDK video
source. Camera-off stops capture; switching replaces the source. The platform
crate owns microphone/camera prompts and typed denied/restricted errors.
`macos/Info.plist` declares both usage descriptions. `CaptureSource` also feeds
the native screen adapter described below. Webcam capture remains implemented
only on macOS; Linux/Windows camera lists and capture are still source gaps.

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
Those phase-1 results did not exercise Linux or Windows. The later screen
adapter and cross-platform contracts below have separate verification gates.
Phase 2a below adds ringing, notification, device-removal and reconnect
contracts with separate acceptance limits.

## Phase 2a: ringing, incoming calls and resilience

Technical contracts only; call views and ringtone playback remain with Claude.
The normal `start_call(room, video)` path notifies, while `join_call` and
`accept_call(room, video)` join without sending another invitation.

Outgoing notifications use `org.matrix.msc4075.rtc.notification`, matching
Element Call 0.28.2's pinned SDK `MatrixRTCSession.sendCallNotify`. Content
includes `notification_type` (`ring` for a DM, `notification` for a group),
`notify_type` for older readers, `sender_ts`, a 90-second `lifetime`,
`m.call.intent`, explicit `m.mentions.user_ids`, `slot_id` and an `m.reference`
to the acknowledged membership event. The modern default slot is **`m.call#ROOM`**;
Element maps the compatibility membership's empty `call_id` to `ROOM`. Opening
`org.matrix.msc4143.rtc.slot` requires the room's state-event permission. A
notification failure is typed and cleans up the local call. Group start is the
explicit notify action; joining an existing call never rings it again.

Read both that event and `m.call.notify` / `org.matrix.msc4075.call.notify`.
Legacy notify support is not legacy VoIP invite/answer support. Sender clocks
more than 20 seconds ahead of the server use the server timestamp; lifetime is
capped at two minutes. Own-user events from every device, ignored users,
nonjoined rooms, expired invitations, other slots and mismatched membership
references are rejected. Room mentions require permission; direct user mentions
do not. The caller device is resolved from SDK-parsed current membership,
not trusted from arbitrary JSON. `verified` is true only when SDK decryption
metadata authenticates that exact sender/device as verified with no forwarder.
Unencrypted or unverified invitations are explicitly unverified.

`Snapshot.incoming_calls` is newest first, one entry per room, at most **8**:
`IncomingCall { room_id, room_name, caller_user, caller_device, caller_name,
kind: Audio|Video, notify_type: Ring|Notification, event_id, since, expires,
verified, ringing }`. Times are Unix milliseconds. The observer uses SDK sync
handlers and room updates; timers only expire SDK memberships/invitations.
Intake is bounded at 32 events, payloads at 16 KiB, and dismissal tombstones at
256 until expiry. Selection does not control reception. Caller departure,
room exit or any own-user membership (including another device answering)
removes the invitation. `accept_call(room, video)`, `decline_call(room)` and
`dismiss_incoming(room)` are frontend actions; queue acceptance precedes the
confirmed snapshot. Decline sends `org.matrix.msc4310.rtc.decline` with an
`m.reference` to the notification. The pinned Element SPA **consumes** this event
and leaves its outgoing ringing call; its embedding host supplies the incoming
decline action. Local dismissal sends no event. Sticky-event delivery/replay
(MSC4354) remains outside this SDK/state-mode implementation.

`CallPhase::Ringing { since, expires }` is outgoing waiting-for-pickup, separate
from established encrypted transport. A peer joining stops it. Decline or
90-second expiry leaves the session with `EndReason::Declined` or `RingExpired`;
explicit leave cancels the attempt. A joined peer wins a simultaneous decline.
`Reconnecting` follows LiveKit's signal/ICE recovery events. The deadline is
**45 seconds**: it permits several SDK retries while remaining shorter than the
120-second membership fallback. The driver leaves with `ReconnectTimeout` on
budget exhaustion, followed by bounded cleanup. In-flight bounded signalling
operations can complete before the deadline is observed. The SDK owns ICE
restart and republishing; this layer does not build a second transport.

On reconnection, refresh membership and redistribute the current sender key;
preserve its index and pending delayed sender switch. Restart the existing
delayed leave, or replace it only after `NotFound`, then republish membership.
This handles a delayed leave firing during an outage without orphaning the old
ID. End/cancellation clears membership and cancels the delayed leave only after
that clear is acknowledged; otherwise leave the server timer armed. If cancel
cannot be acknowledged, retain the account reservation through the old
15-second timer before another local session can reuse its state key. A wholly
unreachable homeserver cannot acknowledge immediate cleanup: server delayed
leave and bounded membership expiry remain the final safeguards.

macOS device changes use CoreAudio property listeners for the device list and
system input/output defaults, plus AVFoundation connect/disconnect notifications.
Other targets have a two-second list-refresh timer. Selected-device removal
chooses the OS default only; never an arbitrary first microphone/camera.
CoreAudio supplies stable default audio UIDs and AVFoundation the default camera.
Windows now queries the communications endpoint (console default if absent),
and Linux queries PulseAudio/PipeWire-Pulse server defaults. Exact IDs take
priority; a unique PulseAudio description can match index-only SDK devices.
On unsupported default discovery, or with no replacement, report
`DeviceUnavailable` and mute/stop affected capture. Physical default-device
resolution and hot-plug remain acceptance gaps. Existing permission
grants are checked without prompts every two seconds on macOS; revocation
mutes/stops the affected track and reports its typed permission error.

`CallState.events` contains at most 16 `(serial, CallEvent)` entries per call.
Deduplicate by serial for toasts; reset the cursor for each new call. Events are `DeviceFallback { kind:
Input|Output|Camera, from, to }` and `PermissionRevoked { kind, error }`.
The confirmed local selection and mute/video fields update with these events.
Frames and secrets remain outside snapshots.

Incoming rings use platform notifications with caller/room titles, generic
“Incoming call” bodies and the existing enabled, preview, sound and room-mute
preferences. Private previews hide caller/room names. Standard OS notification
presentation delegates Focus/DND to the OS; no critical/time-sensitive bypass.
macOS categories, freedesktop actions and Windows toast actions expose Accept
(audio) and Decline; default activation opens the room. Cold activation opens
only, and live actions revalidate account, invitation ID and expiry. Notifications
are withdrawn on removal/expiry/account cancellation. The UI consumes `ringing`
for ringtone playback; no audio assets are bundled.

Phase 2a verification is recorded separately in PLAN.md. The ringing fixture
uses the real pinned SPA to originate notifications and react to declines;
its host decline path uses the SPA's bundled `sendRtcDecline` API.
The standalone SPA emits `Room.timeline` before Megolm decryption, while its
outgoing decline lifecycle listens only to that initial event. The ringing
fixture explicitly forwards the original decrypted decline through the room
emitter to model the embedding host's decrypted-event boundary. Contents,
sender, event IDs and the actual call lifecycle are unchanged. This is qualified
host-boundary evidence, **not** standalone encrypted-decline acceptance or a
live Element Web/widget test. No plaintext fallback is introduced. Receiving a Matrix event in
that SDK is distinct from an Element Web incoming-call interface. Synthetic
SFU interruption checks do not establish Wi-Fi changes, physical device removal,
OS notification interaction, permission revocation, or Linux/Windows acceptance.

Ringing implementation references:
[SDK MatrixRTCSession](https://github.com/matrix-org/matrix-js-sdk/blob/3f05dc4d104a3dfc753c5e3549a18c74fab4b076/src/matrixrtc/MatrixRTCSession.ts),
[SDK notification validation](https://github.com/matrix-org/matrix-js-sdk/blob/3f05dc4d104a3dfc753c5e3549a18c74fab4b076/src/matrixrtc/types.ts),
[SDK sendRtcDecline](https://github.com/matrix-org/matrix-js-sdk/blob/3f05dc4d104a3dfc753c5e3549a18c74fab4b076/src/client.ts),
[Element ringing lifecycle](https://github.com/element-hq/element-call/blob/v0.28.2/src/state/CallViewModel/CallNotificationLifecycle.ts).
The isolated ringing test is `native_matrixrtc_ringing_element_host`. Set
`MATRIX_TEST_ELEMENT_STANDALONE=1` to disable simulated decrypted-event forwarding;
the unforwarded encrypted-decline path was observed failing on 2026-11-12.

Final-source offline verification (2026-11-12): formatting, strict locked
all-target/all-feature Clippy, production library/binary Clippy without media
`evidence`, and all-feature workspace tests passed (190 passed, 30 opt-in tests
ignored). The production dependency graph enables no media evidence feature.
`run.sh services` passed 6/6 and `run.sh room-open` passed 4/4. Final-source
`run.sh call` passed 3/3 in 171.67 seconds: both existing media cases (including
20-second SFU interruption recovery), plus four explicitly qualified host-boundary
ringing scenarios. The native call retained membership and resumed bidirectional
decrypted audio/video after the interruption. PLAN.md also records the standalone
decline limitation and the earlier transient Element media failure; two subsequent
complete runs passed. No physical or cross-platform acceptance is claimed.


## Linux/Windows screen and audio-default contracts (2026-11-14)

`CallState.screen_backend` reports the platform route, not permission or runtime
acceptance. macOS keeps the ScreenCaptureKit system picker. Wayland uses the
SDK's PipeWire/XDG desktop portal generic capturer; the Rust route does not retry
through X11 when that request fails. X11 uses explicit screen/window
sources. Windows requires WinRT GraphicsCapture support and uses the pinned
SDK's WGC-enabled screen/window capturers (the SDK also enables its DirectX
fallback). The exact Windows capturer selected still needs runtime verification.
No web runtime or second rendering frontend is involved.

For macOS/Wayland, `Control::Screen(true)` opens the OS picker. For X11/Windows,
the source-selection view, not yet built, must use:

1. `Backend::request_screen_sources(account_instance, SourceKind) -> CallTicket`.
   Choices arrive in `Snapshot.call.screen_sources`, with an OS title and a typed
   `ScreenSelection { kind, id }`. The list is bounded to 128 choices, titles to
   512 characters, and is cleared on terminal call state. Source titles are
   omitted from Debug output and never logged or persisted.
2. Capture that returned ticket with the picker. After explicit user selection,
   call `Backend::share_screen_source(ticket, selection)`. The backend checks
   account/call generation before enqueueing and again in the call task; the
   native worker re-enumerates and validates the selected ID. Missing, duplicate
   or stale IDs fail rather than selecting the first screen/window.
3. `Control::Screen(false)` stops sharing. Existing native controls/views have
   not been expanded into an X11/Windows source picker by this technical change.

Capture and source-query workers each have one global permit. A timed-out or
cancelled caller does not release a native worker's permit until that worker
returns, preventing repeated requests from creating unbounded stranded workers.
Source queries wait at most five seconds, capture initialization ten seconds.
Initialization runs off the async worker; late returned captures are dropped
after call cancellation. Core also enforces a 120-second first-frame deadline
independently of the capture loop, keeping the call responsive if native capture
stalls. A blocking native call can outlive the caller's wait; OS calls are not
forcibly interrupted. Stop requests are neutral `Ended`; SDK permanent errors and
first-frame timeout become typed `ScreenError` values in `CallState.error`.
The SDK does not distinguish picker cancellation, denial and other permanent
failures; no permission cause is invented. A late startup after cancellation
sees its stop flag. Windows Runtime initialization and teardown stay on the
capturer-owning thread, with non-Send guards and capturer-first destruction.

`platform::media_devices::query_audio_defaults` uses one bounded blocking query
with a three-second caller budget. macOS keeps CoreAudio IDs. Windows queries
IMMDevice communications input/output endpoints, falling back only to the OS
console default. Linux reads the running PulseAudio/PipeWire-Pulse server's
actual default names and descriptions, without autospawning a daemon, creating
an audio stream or changing routing. Its context/mainloop has a two-second
budget. Server user/host/cookie fields are not copied. COM allocations are freed
and initialization is balanced on the same worker thread.

Core resolves exact SDK IDs first (Windows endpoint IDs compare ASCII case
insensitively), then a unique PulseAudio description if ADM exposes index-only
IDs. Unknown or ambiguous startup matches retain the SDK's existing choice.
Hot-plug fallback preserves an available selection, otherwise uses only a
resolved OS default or the SDK's explicit `default` entry; no arbitrary first
microphone is chosen. Camera activation likewise uses AVFoundation's real
default rather than the first enumerated camera. **Linux/Windows webcam capture
is still unimplemented**; this port does not relabel that source gap as merely
unverified hardware behavior.

All screen frames continue through the existing bounded native video source,
mandatory LiveKit frame E2EE and latest-frame handles. Screen failure stops the
share, not the encrypted call. Message E2EE and media E2EE remain distinct gates.

Verification: macOS fmt, strict Clippy and 203 offline tests passed; isolated
generated-media call regressions passed 3/3. Linux/Windows CI is pending. Physical XDG portal consent and
cancellation, X11/WGC source capture, actual selected Windows backend, desktop
lock/stop, multiple monitors, default-device mapping, playback and hot-plug on
Linux/Windows remain unverified. The source-picker view is not built yet. No physical or milestone acceptance is claimed.
