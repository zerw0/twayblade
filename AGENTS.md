# Matrix Client contributor instructions

## Product and scope
- Read `PLAN.md` before implementation. Work through its milestones and keep its progress section accurate.
- Build a native Rust desktop client for macOS, Linux, and Windows. Iced is provisional until M0 passes.
- macOS is the primary UX and acceptance platform. Preserve Linux/Windows portability while prioritizing Mac conventions.
- Use compact typography, restrained blue accents, system appearance, standard window decorations, and clear product language. Keep protocol identifiers and discovery diagnostics out of normal sign-in screens.
- Iced controls are custom-rendered: do not claim AppKit controls, native vibrancy, system accent matching or full accessibility without implementing and verifying them.
- Keep the chat UI native. Element Call may use a dedicated web runtime, behind a restricted bridge.
- Required release features: E2EE, spaces, threads, OAuth/OIDC, legacy SSO, and direct/group Element Call calls.
- Modern MatrixRTC direct calls and legacy `m.call.*` calls are separate compatibility targets.

## Architecture
- `desktop`: presentation state and Iced views. Never block the UI with networking or storage.
- `core`: Matrix SDK integration, authentication, messaging, crypto, and session lifecycle. It owns Matrix state.
- `platform`: credentials, notifications, deep links, OS permissions, packaging integration.
- `call-host`: isolated Element Call runtime and capability-restricted IPC.
- Use SDK timelines, sync, crypto, and stores rather than reimplementing protocol state or cryptography.
- Add crates and dependencies when needed; avoid empty abstraction layers and speculative features.

## Security
- Use the system browser for authentication. Validate callbacks and use PKCE for OAuth.
- Never log passwords, access/refresh tokens, recovery keys, message bodies, or authentication callback URLs.
- Do not persist credentials before OS-backed secret storage is implemented. No plaintext fallback.
- Never silently downgrade encryption. Treat message E2EE and media E2EE as separate acceptance checks.
- Restrict web origins, navigation, permissions, and bridge capabilities. Never put account tokens in call URLs.
- Use isolated test accounts and local test services. No production changes, publication, or credential reuse without authorization.

## Development and validation
- Use `nix develop` for project tooling; do not install global tools or invoke Nix-store binaries directly.
- Commit `Cargo.lock` and `flake.lock` once generated. Keep supported Rust/dependency versions explicit.
- Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and relevant tests in the shell.
- Add tests for meaningful behavior and regressions; do not add tests just to mirror a scaffold.
- Separate compile checks, local execution, interoperability, and physical platform acceptance in reports.
- A macOS build does not establish Windows/Linux support. Calls require actual media and permission checks on each OS.
- Keep documentation current and state incomplete features clearly. Never present fixtures as live Matrix data.
- Do not spawn subagents unless explicitly requested by the user.
