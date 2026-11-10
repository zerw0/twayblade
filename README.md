# Matrix Client

Native Rust Matrix desktop client with macOS as its primary platform and Linux/Windows targets. Current M0 prototype: homeserver discovery, password sign-in, OS-backed sessions, encrypted SDK stores, room sidebar and SDK timeline/history/send queue. macOS builds/checks pass; user screenshots confirm password sign-in, live room loading and history rendering. Live message delivery/E2EE acceptance, verification/recovery, browser authentication and calls are pending.

The interface uses bold Mac headings, a compact sign-in window, blue actions, system appearance and a split-view workspace. Controls are custom-rendered Iced. See [macOS UI direction](docs/architecture/MACOS_UI.md).

See [PLAN.md](PLAN.md) for scope and acceptance gates and [AGENTS.md](AGENTS.md) for contributor instructions.

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for macOS/Linux setup, credential storage,
preview modes, checks and troubleshooting.

```sh
nix develop
cargo run -p matrix-client-desktop
```

Enter an HTTPS homeserver root URL and choose **Continue**. On password-capable servers, username/password fields appear. Sign-in saves the session and database secret in macOS Keychain, Windows Credential Manager or Linux Secret Service, with no plaintext fallback. Launch loads the saved device and starts SDK sync. Select a room to load history; **Load Earlier** fetches another page. Enter/Send queues plain text and shows local delivery state. Drafts are kept in memory per room and cleared on logout; they do not persist across restart. Native sliding sync is required in this increment; older-server compatibility remains pending.

Use a dedicated test account while this increment is unverified. Linux needs an unlocked Secret Service desktop keyring and DBus session. An unavailable OAuth metadata response is inconclusive, not proof of unsupported OAuth. Root URLs are currently required; delegated user-ID discovery and path-prefixed servers are pending.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Windows: use a Rust toolchain >= 1.96 and the Visual Studio C++ build tools, then run the same Cargo commands. Nix shell covers macOS/Linux. Platform runtime acceptance is tracked in the plan and requires native execution.

The GitHub Actions workflow is a three-OS draft and has not run. Cargo/Nix inputs are locked. macOS check/build, formatting, Clippy and 16 unit tests pass. Minimum Rust 1.96 is declared but not independently tested; the local Nix shell supplies Rust 1.99. See [compatibility evidence](docs/compatibility/STATUS.md).

## Interface previews

These modes do not access Keychain, authenticate or synchronize. The empty workspace preview has no sample conversations; the populated conversation preview uses explicitly labeled offline sample data with sending disabled.

```sh
nix develop --command cargo run -p matrix-client-desktop -- --preview
nix develop --command cargo run -p matrix-client-desktop -- --preview-workspace
nix develop --command cargo run -p matrix-client-desktop -- --preview-conversation
```

Add `--capture /tmp/matrix-preview.pam` to a preview command to capture only its own content window in RGBA PAM format and exit. Normal launches ignore the capture flag. Tab/Shift-Tab moves focus, Command-F focuses room search and Command-Q quits on macOS. Physical keyboard acceptance is pending.

Conversation presentation includes display names (Matrix ID on hover), local timestamps, consecutive-message grouping and **Jump to Latest** while reading older messages. The selection palette is subdued; native macOS accent/semantic-color integration remains planned.

Spaces navigation uses the SDK's joined-space filters. Select a space in the left
rail to filter conversations, or **Home** to show only conversations outside all known spaces.
Nested entries are marked with an arrow; a parent includes SDK-provided descendants.
Badges aggregate unread notifications in joined conversations. Shared rooms keep
one identity and their drafts when switching spaces. The SDK currently exposes
two navigation levels; server hierarchy discovery, membership and space management
are still pending. Live spaces need acceptance on a connected test account.

Home excludes rooms belonging to any known space, including nested membership.
It waits for the initial space snapshot before showing ungrouped rooms. Space
membership is based on SDK-supplied filters and cached state; live server-state
completeness remains an acceptance check.

The redesigned interface loads San Francisco from macOS at runtime, uses compact
room rows and vector navigation icons, a quiet conversation toolbar, grouped
message avatars and an inset rounded composer. Account/reconnect/sign-out controls
are available from **Account** at the bottom of the sidebar. Inspect Home without
credentials using `--preview-home`; other previews continue to work.
