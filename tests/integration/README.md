# Local Matrix integration checks

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
colima start --profile matrixclient-test --cpu 2 --memory 3 --disk 10 --activate=false
MATRIX_TEST_DOCKER_CONTEXT=colima-matrixclient-test bash tests/integration/run.sh
colima stop --profile matrixclient-test
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
SwiftUI interaction acceptance. Restoration here retains the SDK session in test
memory: it does not test OS Keychain persistence, a separate process restart,
crash recovery or the normal `auth::restore()` path. It does not exercise browser
SSO, verification/cross-signing, backup recovery, attachments, or calls. Failed
runs can leave generated sessions on the isolated server; they never touch the
app's normal credential entry.

## Optional Nix invocation

```sh
MATRIX_TEST_DOCKER_CONTEXT=colima-matrixclient-test \
  nix develop --command bash tests/integration/run.sh
```

Docker is provided by your host; the project shell supplies Rust tooling.

References: [Synapse Docker instructions](https://github.com/element-hq/synapse/blob/develop/docker/README.md),
[Synapse shared-secret registration](https://element-hq.github.io/synapse/latest/admin_api/register_api.html).
