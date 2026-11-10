# Development on macOS and Linux

macOS is the primary UX and acceptance platform. Linux is a supported build
target, but has not yet passed native runtime acceptance. See
[compatibility evidence](docs/compatibility/STATUS.md) for verified coverage and
[PLAN.md](PLAN.md) for outstanding work.

## Shared tooling

Install Nix with flakes enabled and Git. Use this repository's pinned development
shell rather than installing Rust tools globally:

```sh
git clone git@github.com:zerw0/matrixclient.git
cd matrixclient
nix develop
rustc --version
cargo --version
```

The shell provides Cargo, rustc, rustfmt, Clippy, pkg-config and CMake, plus Linux
display libraries and DBus build inputs. `flake.lock` pins Nix inputs and
`Cargo.lock` pins Rust dependencies. The declared minimum Rust version is 1.96;
the current local Nix environment provides 1.99. Minimum-version acceptance is
still pending.

The first shell/build requires network access and may download and compile many
dependencies. Keep both lockfiles committed. For ordinary builds, use `--locked`
so an unexpected dependency change fails visibly.

Optional direnv integration: install direnv/nix-direnv through your existing
system configuration, enable its shell hook, inspect `.envrc`, then run
`direnv allow`. `.envrc` contains only `use flake`; do not put credentials in it.

## macOS

Use a normal logged-in macOS desktop session. Ensure the Xcode Command Line Tools
are available; `xcode-select -p` should show the active developer directory.
If absent, install them with `xcode-select --install` before building.

From the Nix shell:

```sh
cargo run --locked -p matrix-client-desktop
```

Or run directly through the shell without entering it interactively:

```sh
nix develop --command cargo run --locked -p matrix-client-desktop
```

The application uses macOS Keychain for the saved session and database passphrase.
The first access may prompt for permission. Unlock the login keychain if needed.
A denied/locked Keychain is an error; there is no plaintext fallback. Passwords
are not saved. Do not share Keychain exports, session records or recovery keys.

San Francisco is loaded from the installed macOS font at startup and is not
bundled for redistribution. Controls are rendered by Iced. Native menu-bar,
vibrancy, system accent colors, VoiceOver and media-permission acceptance remain
separate work; an unsigned development executable is not a signed release app.

Check light/dark appearance, Retina rendering, keyboard focus, Command-F search,
Command-Q, window resizing and the Account menu on physical hardware.

## Linux

Run from your graphical desktop session with working graphics drivers. The Nix
shell supplies Wayland, X11, libxkbcommon, OpenGL/Vulkan loader libraries and DBus
development inputs; the desktop/display server and a Secret Service provider
must be provided by your host system.

Requirements for authenticated runs:

- A Wayland or X11 session. Preserve `WAYLAND_DISPLAY` or `DISPLAY` and
  `XDG_RUNTIME_DIR` from that session.
- The desktop session's DBus user bus, normally exposed through
  `DBUS_SESSION_BUS_ADDRESS`.
- An unlocked Secret Service provider, such as the one configured by your
  desktop environment. No mock or plaintext credential backend is used.
- Installed fonts and usable host graphics drivers. Nix userspace libraries do
  not install or configure the host GPU driver.

Use the same commands as macOS:

```sh
nix develop
cargo run --locked -p matrix-client-desktop
```

On NixOS, configure the required graphical session, drivers and desktop keyring
in your system configuration. On other Linux distributions, a mismatch between
Nix graphics libraries and host drivers may require host-specific integration;
record the distribution, session type and renderer when reporting a failure.
Linux X11 and Wayland runtime behavior are not yet verified in this project.

Do not launch through sudo: it changes the user session and credential-store
context. A new private DBus bus alone does not provide an unlocked Secret Service
keyring. Headless CI builds can pass without a desktop or keyring; that does not
establish that interactive authentication works.

## Credential-free UI previews

These modes skip Keychain/Secret Service access, login and synchronization:

```sh
cargo run --locked -p matrix-client-desktop -- --preview
cargo run --locked -p matrix-client-desktop -- --preview-workspace
cargo run --locked -p matrix-client-desktop -- --preview-conversation
cargo run --locked -p matrix-client-desktop -- --preview-home
```

The populated previews use explicitly labeled sample data and disable sending.
Home shows only the standalone sample room; space-owned rooms are excluded.
Previews need a graphical session even though they do not need a keyring.

For visual QA, add `--capture /tmp/matrix-preview.pam` to a preview command.
It writes only the application's own content window as RGBA PAM and exits.
The capture flag has no effect during normal account sessions. Keep captures out
of commits, especially if another tool captures real account data.

## Checks

Run inside `nix develop`:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked -p matrix-client-desktop
```

Use `--offline` only after the dependency cache is populated. It cannot replace
the first fetch. The workflow in `.github/workflows/check.yml` runs native OS
checks; review its actual results before claiming CI or platform acceptance.

Unit tests cover selected filtering, draft/session UI boundaries and message
grouping. Test real login, restart with the same device, live sending, encrypted
interoperability, space membership changes and media separately with dedicated
test accounts. Do not use production accounts or send test messages to unrelated
rooms as part of automated checks.

## Local state and credentials

The current credential entry uses service `matrix-client.development` and account
`active-session-v1`. The SDK data directory is resolved by `directories::ProjectDirs`
with `("", "MatrixClient", "MatrixClient")`; its `sessions/<UUID>` child contains
SDK stores. The exact base path follows the platform and XDG configuration.
Session secrets and the SDK store passphrase are stored in the OS credential
store, not in repository configuration. Logout revokes the server session and
removes that secret record; encrypted store files remain. Recovery after failed
logout or interrupted login is still under development.

`.gitignore` excludes environment secrets, credential/key files, local session
exports, SDK databases, logs, build outputs and captures. It allows example env
templates. Never use real secrets in examples: ignore rules are only one layer
of protection and do not remove files already committed.

Before publishing changes:

```sh
git status --short
git diff --cached --stat
git diff --cached --check
```

Inspect staged files locally and run a secret scanner when available. Do not post
secret-bearing diagnostics in an issue. Preserve redacted error categories and
describe whether the evidence is a build, preview, live interoperability check,
or physical platform test.

## Troubleshooting

- **Nix daemon/cache permission denied:** retry from a normal terminal with access
  to your Nix installation. A sandbox denial is not a Rust compile failure.
- **crates.io DNS/fetch failure:** restore network access, then retry. Offline
  builds work only when every locked dependency is cached.
- **Waiting for Cargo's build lock:** another build/run process owns the target
  directory. Let it finish; do not delete lockfiles or kill unrelated processes.
- **Keychain/Secret Service unavailable:** unlock the correct desktop credential
  store and run as the same logged-in user. Preview mode can validate layout
  independently.
- **Empty Home:** Home deliberately hides rooms belonging to known spaces. Select
  a space in the rail. Initial membership loading also keeps Home empty briefly.
- **No rooms/history or sync failure:** this increment requires native sliding
  sync support. Record the server's supported capabilities and SDK version;
  legacy-sync fallback is not implemented.
- **Unable to decrypt:** verification/recovery/key-backup UI is incomplete.
  Re-login or deleting local stores is not a substitute for preserving device
  keys. Keep the current session/state until a recovery path is established.
