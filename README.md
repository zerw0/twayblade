# Twayblade

A native desktop client for [Matrix](https://matrix.org), written in Rust.

One codebase for macOS, Linux and Windows, drawn with [GPUI](https://www.gpui.rs) and
built on the official [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk).
No Electron, no web view for chat. macOS is the primary platform.

> **Status: early development.** Twayblade is not ready for daily use. Expect missing
> features and rough edges, and use a test account. See [what works](#what-works) and
> the [plan](PLAN.md).

## What works

- **Sign in** with a password, OAuth/OIDC or single sign-on in your browser; sessions are
  kept in the OS keychain, never in plain files
- **End-to-end encryption**: device verification with emoji, secure backup and recovery
- **Spaces, rooms and direct messages**, with favourites, invites and room creation
- **Conversations**: threads, replies, reactions, edits, formatted messages, images and
  files, read receipts and typing indicators
- **Search** conversations, and messages (encrypted rooms: history already on this device)
- **Notifications** that follow your Matrix push rules, with private previews
- **Make it yours**: light, dark and curated colour schemes, interface scale, fonts,
  and Bubbles, IRC or Minimal chat styles

Calls (Element Call) and the public room directory are planned.

## Download

Builds for macOS (Apple silicon), Linux (x86_64) and Windows (x86_64) are produced by
the [Build binaries](../../actions/workflows/build.yml) workflow for every change on
`main`, and attached to [releases](../../releases) for version tags.

They are not signed by a developer certificate yet:

- **macOS**: unzip, then right-click `Twayblade.app` › Open the first time.
- **Windows**: SmartScreen may warn; choose More info › Run anyway.
- **Linux**: extract and run `./twayblade` from a graphical session.

Your homeserver needs native sliding sync (MSC4186), as in current Synapse
releases.

## Build from source

You need [Rust 1.99+](https://rustup.rs) and your platform's build tools
(see [DEVELOPMENT.md](DEVELOPMENT.md)).

```sh
git clone https://github.com/zerw0/twayblade.git
cd twayblade
cargo run --locked --release -p twayblade
```

On macOS, build and open the app bundle:

```sh
bash macos/build.sh --release --run
```

Try the interface without an account, using offline sample data:

```sh
cargo run --locked -p twayblade -- --preview
```

## Project

| | |
|---|---|
| [PLAN.md](PLAN.md) | Roadmap, milestones and what has been verified |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Setup for macOS, Linux and Windows, tests, troubleshooting |
| [AGENTS.md](AGENTS.md) | Contributor rules: architecture, security, branches |
| [docs/](docs/) | Architecture notes and compatibility evidence |

The code is split into three crates: `twayblade-core` (Matrix, sessions, encryption),
`twayblade-platform` (keychain, notifications, OS integration) and `twayblade` (the app).

Contributions go through feature branches and pull requests. Run
`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace` before opening one.
