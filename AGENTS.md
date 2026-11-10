# Matrix Client contributor instructions

## Product and scope
- Read `PLAN.md` before implementation. Work through its milestones and keep its progress section accurate.
- Keep the shared Matrix application core in Rust. Use GPUI + GPUI Kit for the shared macOS/Linux/Windows frontend; the user approved replacing the split frontend.
- macOS is the primary UX and acceptance platform. Preserve Linux/Windows portability while prioritizing Mac conventions.
- Use compact typography, the system accent with contrasting foregrounds, system appearance, standard window decorations, and clear product language. Keep protocol identifiers and discovery diagnostics out of normal sign-in screens.
- Keep the chat UI native. Element Call may use a dedicated web runtime, behind a restricted bridge.
- Required release features: E2EE, spaces, threads, OAuth/OIDC, legacy SSO, and direct/group Element Call calls.
- Modern MatrixRTC direct calls and legacy `m.call.*` calls are separate compatibility targets.

## Architecture
- `gpui`: the only frontend (GPUI + GPUI Kit). `macos/` holds only the development bundle helper. The earlier Iced and SwiftUI frontends were deleted; do not reintroduce parallel frontends.
- GPUI is custom-rendered. Use semantic GPUI Kit components/tokens and platform conventions. Do not claim AppKit widgets, Liquid Glass or full accessibility without implementing and verifying them. Keep SDK services independent of GPUI.
- Keep SDK clients and credentials out of public UI snapshots. Bound update queues, cancel account/selection tasks, and reject stale snapshots.
- `core`: Matrix SDK integration, authentication, messaging, crypto, and session lifecycle. It owns Matrix state.
- `platform`: credentials, notifications, deep links, OS permissions, packaging integration.
- `call-host` (planned, M5): isolated Element Call runtime and capability-restricted IPC.
- `core` returns data and typed errors; user-facing wording belongs in the frontend.
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
- Build Rust in the pinned Nix shell. `macos/build.sh` packages the GPUI app. Generated bindings/build outputs stay ignored. Keep DEVELOPMENT.md focused on standard macOS/Linux/Windows setup, with Nix in its own optional section.
- Commit `Cargo.lock` and `flake.lock` once generated. Keep supported Rust/dependency versions explicit.
- Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and relevant tests in the shell.
- Add tests for meaningful behavior and regressions; do not add tests just to mirror a scaffold.
- Separate compile checks, local execution, interoperability, and physical platform acceptance in reports.
- A macOS build does not establish Windows/Linux support. Calls require actual media and permission checks on each OS.
- Keep documentation current and state incomplete features clearly. Never present fixtures as live Matrix data.
- Do not spawn subagents unless explicitly requested by the user.

## Branches and collaboration
- Create and switch to a dedicated branch before starting each feature: `feature/<short-name>`. Use `fix/<short-name>` for fixes and `docs/<short-name>` for documentation-only work. Do not implement features directly on `main`.
- Base new feature branches on `main` unless the task specifies another base. Keep each branch focused on one feature, including its related tests and documentation.
- Inspect Git status before switching or editing. Preserve existing work; never reset, discard or stash someone else's changes to make room for your task. Use a separate worktree when unrelated work or another contributor occupies the current checkout.
- Share changes through feature branches and reviewable pull requests. Follow the session's authorization for publishing and merging; branch creation alone does not authorize a push or merge.
- When finishing a feature, update `PLAN.md` with the actual implemented behavior, verification performed and remaining acceptance gaps. Keep compilation, offline UI tests, live integration and physical platform checks distinct.

## UI design guidance
- Apply the user-requested UI/UX Pro Max skill for UI design, implementation and review: `.agents/skills/ui-ux-pro-max/SKILL.md` when installed locally. Upstream: https://github.com/nextlevelbuilder/ui-ux-pro-max-skill (selected revision `9b675f0b13c9c28dec445e8ce4633c18c39b6467`). Its canonical skill path is `.claude/skills/ui-ux-pro-max/SKILL.md` upstream.
- The local skill is intentionally ignored. Contributors can install it repo-locally using their agent's skill installer; do not commit AI tool state or installed skill folders.
- GPUI is the implementation stack. The skill has no GPUI stack guide: use its relevant style/accessibility/interaction guidance and GPUI Kit's current APIs and coding/design guides. Do not substitute a web stack or apply mobile/marketing layouts to the desktop client.
- Keep semantic theme tokens, readable text contrast, visible keyboard focus, descriptive icon controls, stable message identities and virtualized long histories. Treat generated design suggestions as recommendations and verify their fit before saving or applying them.
- Keep `AGENTS.md`, `PLAN.md` and product documentation tracked even though AI tool folders are ignored.
