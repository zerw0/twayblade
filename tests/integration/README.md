# Local Matrix integration checks

## Native MatrixRTC audio and video

Inside `nix develop`, with a local Docker engine:

```sh
npm ci --prefix tests/integration/browser --ignore-scripts
PLAYWRIGHT_BROWSERS_PATH=target/playwright tests/integration/browser/node_modules/.bin/playwright install chromium
MATRIX_TEST_DOCKER_CONTEXT=colima-matrixclient-test bash tests/integration/run.sh call
```

The call profile runs digest-pinned Synapse, LiveKit 1.9.0, lk-jwt-service 0.9.0,
Element Call 0.28.2 and a TLS proxy. The native headless client and pinned
Chromium/Element peer join an encrypted room. Assertions cover memberships,
both participant lists, encrypted key exchange (counts only), exactly two
LiveKit participants and decrypted generated audio/video on both sides. A
native/native case checks the same media path. No synthetic Matrix membership
or key events, physical devices, GPUI views or WebKit are involved.

Service ports bind to loopback and only this Compose project's call services
are stopped afterward, including on failure. Existing generated test data is
retained; both pre-rename and current fixture server aliases are supported.
Native clients trust the fixture CA through the account certificate option;
Chromium trusts only the fixture leaf's SPKI. No host trust roots are changed.
Browser consoles and credential-bearing network URLs are never forwarded.
`MATRIX_TEST_CALL_FILTER` can select one native call test during diagnosis.

Physical microphone/speaker/camera permissions and media on this Mac require a
person. Public-server Element Web, Linux and Windows are not attempted.
[Architecture, key formats and UI contracts](../../docs/architecture/CALLS.md).

## Other Matrix checks

This opt-in fixture uses pinned Synapse v1.164.2 (image digest recorded in
`compose.yaml`), SQLite and two generated test accounts. It binds only
`127.0.0.1:18448`, with a local CA and a signed server certificate. No system
trust roots are changed. Registration and federation are disabled; the bootstrap
registers the test accounts through Synapse's shared-secret registration API.

Requirements: a running **local** Docker engine with Compose, and the project's
Rust toolchain. No accounts/passwords from the normal app are used. Generated
configuration, TLS keys, account passwords and SDK stores stay under the ignored
`test-data/local/integration/` directory. Never share that directory or enable
HTTP/SDK debug logs for these tests.

```sh
bash tests/integration/run.sh
```

The default command starts the fixture, runs the opt-in Rust integration test,
and stops its container afterward. `up` leaves the service running; `down` removes
only this Compose project's containers/network and retains local data. The test
is ignored during ordinary `cargo test` runs and hard-rejects a non-loopback URL.
Certificates are renewed automatically when nearing expiry.

On macOS with Colima, use a dedicated VM without changing the default context:

```sh
colima start --profile twayblade-test --cpu 2 --memory 3 --disk 10 --activate=false
MATRIX_TEST_DOCKER_CONTEXT=colima-twayblade-test bash tests/integration/run.sh
colima stop --profile twayblade-test
```

Docker Desktop or a native Linux Docker daemon can use its current local context.
Remote Docker endpoints are rejected. Windows users can use a Bash environment
with access to the same local Docker engine and Cargo; this harness has not been
accepted on Windows.

## What the test exercises

- HTTPS certificate validation using only the fixture's CA.
- Two generated password sessions and the application's unified SDK sync service.
- A real SDK-backed profile fetch.
- An encrypted room, the application's timeline/send queue and recipient decryption.
- Encrypted wire event inspection with a check that the unique test text is absent.
- Reopening the recipient's encrypted SQLite store with its original SDK session
  and device identity, then decrypting the earlier message again.
- Revoking the generated test sessions on successful completion.

This is **SDK/application-service integration**, not Element interoperability or
UI interaction acceptance. Restoration here retains the SDK session in test
memory: it does not test OS Keychain persistence, a separate process restart,
crash recovery or the normal `auth::restore()` path. It does not exercise browser
SSO, verification/cross-signing, backup recovery, attachments, or calls. Failed
runs can leave generated sessions on the isolated server; they never touch the
app's normal credential entry.

## Optional Nix invocation

```sh
MATRIX_TEST_DOCKER_CONTEXT=colima-twayblade-test \
  nix develop --command bash tests/integration/run.sh
```

Docker is provided by your host; the project shell supplies Rust tooling.

References: [Synapse Docker instructions](https://github.com/element-hq/synapse/blob/develop/docker/README.md),
[Synapse shared-secret registration](https://element-hq.github.io/synapse/latest/admin_api/register_api.html).

## OS Keychain lifecycle acceptance (macOS)

With an unlocked login Keychain, run in the project shell:

```sh
MATRIX_TEST_DOCKER_CONTEXT=colima-twayblade-test bash tests/integration/run.sh keychain
```

This invokes the normal authentication implementation with a unique test
credential slot and local data root. The fixture CA is scoped to the test client;
no host trust settings or application credentials are changed. Separate child
processes log in, sync a room, send, restore the original device and cached
history from Keychain, then invoke normal logout. Assertions check that failed
sign-in and successful logout delete the scoped Keychain entry and that logout
removes the SDK store. Only device/room IDs and paths are written as evidence;
credentials stay in OS secret storage. This is automated application-service
acceptance, combined with separately recorded operator GPUI checks; it is not
physical UI automation or Element interoperability. The test is ignored in the
ordinary suite. Release builds now use a credential service separate from debug
builds; release signing and migration acceptance remain unverified.

## Functional workspace service checks

```sh
MATRIX_TEST_DOCKER_CONTEXT=colima-twayblade-test bash tests/integration/run.sh services
```

Starts the isolated fixture, runs the following checks sequentially, then stops
only its container. Normal workspace tests skip these opt-in checks.

- `encrypted_delivery_profile_and_same_device_store_restore`: SDK-to-SDK E2EE.
- `timeline_metadata_and_send_queue_actions`: replies, reaction toggle, message
  kinds, thread count, permission-induced failed echo, SDK retry/cancel.
- `members_typing_and_read_receipts`: names/roles, typing start/stop, fully-read
  separator and receipt propagation to another SDK client.
- `pagination_preserves_message_identity`: 90 pre-sync historical messages,
  pagination to history start, stable identities and no duplicates.
- `room_list_service_tracks_recency_and_details`: SDK list order after activity,
  name/topic refresh, membership removal and unique room identities.

Run `keychain` separately for the macOS cross-process lifecycle. All service
checks hard-reject a non-loopback fixture URL and revoke their generated sessions
on success. Metadata/action checks use unencrypted rooms; media transfer and
Element interoperability are outside these tests.

## OAuth/OIDC fixture

Run `nix develop --command bash tests/integration/run.sh oauth` on macOS with an
unlocked Keychain and local Docker. The separate `oauth` Compose profile pins
MAS 1.27.0, Synapse 1.164.2 and PostgreSQL 17 by digest. Only TLS loopback ports
19448/19449 are published; PostgreSQL has no published port and uses tmpfs.
Generated credentials/configuration remain in ignored `test-data/local/oauth/`;
no host trust settings change. Fixture rate limits are raised and access tokens
expire after five seconds. The runner stops only its three OAuth services.

`oauth_cancel_and_wrong_state_discard_checkpoint` checks callback rejection and
cleanup. `oauth_keychain_lifecycle_across_processes` drives MAS registration and
consent using Python HTTP forms, then restores/refreshes/signs out in separate
processes. This exercises real OAuth endpoints and OS storage; the operator still
checks system-browser interaction on a real OAuth homeserver.

Legacy SSO: `nix develop --command bash tests/integration/run.sh sso` starts a
separate Synapse at TLS loopback port 20448 with MAS as its local OIDC provider.
The fixture requests `openid`, maps the stable subject, and relaxes Synapse's
OIDC metadata URL checks for its internal Docker HTTP backchannel. Browser TLS
and signed ID-token validation remain enabled. The runner tests
`sso_local_oidc_token_exchange_and_restore` and stops its services afterward.

Session recovery: `nix develop --command bash tests/integration/run.sh recovery`
uses the normal fixture plus a separate Synapse on TLS loopback port 21448 with
five-second nonrefreshable tokens. `revoked_device_and_soft_logout_recovery`
revokes a device from a second SDK client, checks sync termination and retained
local data, then checks failed/successful soft reauthentication and hard cleanup.

Encryption usability: `nix develop --command bash tests/integration/run.sh security`
generates a fresh user and runs `secure_backup_fresh_store_recovery_and_sas`.
It covers setup, wrong key, a fresh store restoring old encrypted history live,
SAS/cancel, other-device sign-out and recovery-key reset. The original device is
offline during backup restoration to exclude key gossip as the recovery source.
The runner prefers an existing local `colima-twayblade-test` context; override
with `MATRIX_TEST_DOCKER_CONTEXT`. Avoid two fixture VMs sharing the same ports.

Run `bash tests/integration/run.sh media` for generated-user plain/encrypted file
and image round trips, server thumbnail/download, oversize rejection, queue cancel
and persisted upload restart. This does not simulate interrupting an HTTP request
mid-transfer or exercise native file dialogs/clipboard/drop.

Thread acceptance: `nix develop --command bash tests/integration/run.sh threads`
uses the same isolated Synapse and generated accounts. It starts a thread through
the SDK, exchanges replies, paginates, checks scoped receipts and unread replies
arriving with the thread closed. Run fixture commands sequentially.

Spaces: `nix develop --command bash tests/integration/run.sh spaces` generates a
fresh account pair and raises only the isolated server's creation/join/invite
limits. The check creates 55 accessible directory children to cross Synapse's
50-room hierarchy page cap, plus an inaccessible private child. It also covers
restricted joins, invites, encrypted creation, multiple parents and selective leave.

Viewer: `nix develop --command bash tests/integration/run.sh viewer` uses a fresh
fixture user for plain/encrypted PNG, animated GIF/WebP, a 9000px image, corrupt
bytes, explicit original-byte save and cancellation. It checks that viewer fetches
do not put decrypted content in the SDK media cache. Offline GPUI tests cover
keyboard, focus, clipboard, gestures and save prompts; native application opening,
physical animation and Element interoperability still need operator acceptance.

Notification checks use the same isolated Synapse fixture and generated accounts:
`bash tests/integration/run.sh notifications` covers SDK defaults, keywords, room
modes, muted unread exclusion and second-client observation;
`bash tests/integration/run.sh notification-delivery` checks SDK-decrypted encrypted
items and private previews. Neither establishes native OS delivery or Element acceptance.

Search: `nix develop --command bash tests/integration/run.sh search` creates a fresh isolated account and checks server pagination, cached encrypted results with a limitation flag, global search across both sources and SDK event context for older results. This does not establish UI scrolling or Element interoperability.

Presence: `nix develop --command bash tests/integration/run.sh presence` runs fresh isolated account pairs with Synapse presence enabled and disabled, then restores the normal enabled setting while stopped. It checks positive capability evidence, shared-room visibility, away/status and absent disabled-server data; there are no production configuration changes.
