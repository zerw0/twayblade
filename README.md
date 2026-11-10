# Matrix Client

A native Rust Matrix desktop client using **GPUI + GPUI Kit** for one shared
macOS/Linux/Windows frontend. macOS is the primary UX platform.

The current prototype connects server-name (`.well-known`) discovery, password sign-in, OS-backed
sessions, encrypted SDK stores, room/space navigation, timelines, pagination and
plain-text sending to the shared Rust core. Consecutive messages share sender
headers; sender names open readonly profiles. Room-owned composers preserve drafts
and editing state across navigation. Home excludes rooms belonging to spaces.

OAuth/legacy SSO, verification/recovery, attachments, emoji/reactions, threads,
Element Call and release packaging remain planned. Stickers and alternative
timeline styles/palettes are deferred past 1.0.

## Run

Install the platform dependencies in [DEVELOPMENT.md](DEVELOPMENT.md), then:

```sh
cargo run --locked -p matrix-client-gpui
```

On macOS, build and launch the development app with:

```sh
bash macos/build.sh --run
```

Offline modes never read the credential store or connect to Matrix:

```sh
cargo run --locked -p matrix-client-gpui -- --preview
cargo run --locked -p matrix-client-gpui -- --preview-home
cargo run --locked -p matrix-client-gpui -- --preview-profile
cargo run --locked -p matrix-client-gpui -- --preview-large
```

A normal launch restores the saved account. Only one instance can run at a
time. Use dedicated test accounts. The homeserver must support native sliding sync.

The GPUI app builds/runs locally on macOS arm64, and offline captures/interaction
tests pass. A separate isolated Synapse test passed encrypted SDK/application
service delivery and same-device encrypted-store reopening. Live GPUI account
flows, Element interoperability and native Linux/Windows execution still need
acceptance. GPUI controls are custom-rendered; AppKit widgets, Liquid Glass and
complete accessibility are not implied.

See [PLAN.md](PLAN.md), [AGENTS.md](AGENTS.md), [architecture](docs/architecture/GPUI.md)
and [compatibility evidence](docs/compatibility/STATUS.md). Use a feature branch for
contributions; repository-local AI tooling and credentials are ignored.
