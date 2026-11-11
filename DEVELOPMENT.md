# Development

The primary frontend is **GPUI + GPUI Kit**, shared across macOS, Linux and
Windows, with Matrix services in Rust. macOS remains the primary UX platform.
See [PLAN.md](PLAN.md) and [acceptance evidence](docs/compatibility/STATUS.md).

## Get the source

Install Git and Rust using [rustup](https://rustup.rs/). The minimum is Rust 1.99,
the version that is built and tested.

```sh
rustup component add rustfmt clippy
git clone https://github.com/zerw0/twayblade.git
cd twayblade
git switch -c feature/my-feature
```

The repository is private. Follow [AGENTS.md](AGENTS.md): use one branch per
feature and a separate worktree for concurrent work. Keep Cargo.lock committed;
use `--locked`. The first GPUI build downloads and compiles many dependencies.
GPUI Kit is pinned to 0.9.1 with its matching GPUI snapshot. Cargo.lock also pins
the compatible Taffy/unicode-properties versions needed by Kit's test features.
Native calls also build the pinned LiveKit Rust bindings and download their
libwebrtc prebuilt. Its build/license and the headless encrypted-media fixture
are documented in [CALLS.md](docs/architecture/CALLS.md). Physical call media on
macOS needs a later operator check; Linux/Windows call acceptance remains pending. Nix is optional and has a separate section below.

## macOS

Requirements: Rust/Cargo, Git, pkg-config, CMake and Apple developer tools. Existing
Homebrew users can install the auxiliary tools with `brew install pkg-config cmake`.
Check `xcode-select -p`; if tools are absent, use `xcode-select --install` or
install/select full Xcode. The current Kit runtime-shader configuration built and
ran with the installed Command Line Tools; other configurations may need Xcode.

Run from a logged-in graphical session:

```sh
cargo run --locked -p twayblade
```

Or build and launch an ad-hoc signed development app:

```sh
bash macos/build.sh --run
# Optimised build for everyday use (output in target/release/bundle):
bash macos/build.sh --release --run
```

Release builds use a separate credential entry from debug builds, so sign in
once per build type.

Output: `target/debug/bundle/Twayblade.app`. The script uses Cargo from the
current shell and resolves Apple signing tools in a clean environment, including
when invoked inside Nix/direnv. The bundle is for local development. Transitive
Nix library dependencies, clean-machine packaging, signing/notarization and
minimum macOS-version acceptance remain release work.

To reopen the existing bundle without rebuilding or signing it again:

```sh
open "target/debug/bundle/Twayblade.app"
```

The build helper reuses a running instance for a normal launch. Offline previews
launch separately.

Ad-hoc signatures change with every build, so macOS asks for Keychain access
again after each rebuild. Create a local signing identity once:

```sh
bash macos/create-dev-certificate.sh
```

It adds a self-signed "Twayblade Development" code-signing certificate to
your login keychain (it never leaves this Mac and is not valid for
distribution). `macos/build.sh` then signs with it automatically, or with
`TWAYBLADE_SIGN_IDENTITY` when set. Approve the first prompt with Always
Allow for this app's session item; later builds keep the same designated
requirement and stop prompting. Do not grant access to all applications.
Without the identity, the build falls back to ad-hoc signing and prints a hint.

A normal launch restores the saved Keychain session. A second instance refuses
to start. Use a dedicated test account for connected development. An unavailable keychain is an error; there is
no plaintext credential fallback.

## Linux

Install Rust and native compiler/display dependencies. The Ubuntu/Debian CI
baseline for the workspace is:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake libdbus-1-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-dev \
  libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev libvulkan-dev \
  libfontconfig1-dev libfreetype6-dev libssl-dev libxcb1-dev libxkbfile-dev
cargo run --locked -p twayblade
```

Run from Wayland or X11 with usable graphics drivers and fonts. GPUI uses its
Linux graphics backend; this setup has not yet passed Linux native acceptance.
Preserve `WAYLAND_DISPLAY` or `DISPLAY`, `XDG_RUNTIME_DIR` and your desktop's
`DBUS_SESSION_BUS_ADDRESS`. Login requires an unlocked Secret Service provider;
a private/headless DBus bus alone does not provide one. Do not run the app with sudo.

Use equivalent packages on other distributions. On NixOS, configure the host
session, graphics drivers and keyring in the system configuration. Build success
does not establish rendering, keyring or media acceptance on X11/Wayland.

## Windows

Install Git for Windows, Rust via rustup with the MSVC target matching your
machine, and Visual Studio 2022 Build Tools with **Desktop development with C++**,
the Windows SDK and CMake tools. Use a current graphics driver compatible with
GPUI's Windows backend.

From PowerShell in the checkout:

```powershell
rustup component add rustfmt clippy
cargo run --locked -p twayblade
```

Windows uses the configured Credential Manager backend, with no plaintext
fallback. Windows native execution, input, notifications, media and packaging
acceptance remain pending. All three platforms now use the same GPUI source.

## Offline previews

These modes skip the credential store, login and synchronization. Samples are
labeled offline; sending and account operations are disabled. You can type local
drafts, switch rooms and open sample profiles.

```sh
cargo run --locked -p twayblade -- --preview
cargo run --locked -p twayblade -- --preview-home
cargo run --locked -p twayblade -- --preview-profile
cargo run --locked -p twayblade -- --preview-details
cargo run --locked -p twayblade -- --preview-signin
cargo run --locked -p twayblade -- --preview-thread
cargo run --locked -p twayblade -- --preview-settings notifications
cargo run --locked -p twayblade -- --preview-notification
cargo run --locked -p twayblade -- --preview-directory
cargo run --locked -p twayblade -- --preview-invites
cargo run --locked -p twayblade -- --preview-viewer
cargo run --locked -p twayblade -- --preview-large
```

`--preview-notification --preview-no-message-preview` shows the private “New message”
sample. These modes send no native notification and request no permission. Native
macOS delivery requires the release `.app`; use `bash macos/build.sh --release --run`.

`--preview-large` supplies 10,000 sample messages to Kit's virtual message
scroller. It is a smoke fixture, not a measured performance guarantee.

To capture only the app's rendered content and exit:

```sh
cargo run --locked -p twayblade --features capture -- \
  --preview --capture /tmp/matrix-gpui.png
```

The `capture` feature enables Kit's test/render support; it requires additional
build dependencies. Captures require an offline preview and are rejected for
normal sessions. Keep captures outside commits. On macOS, the build helper enables
capture support automatically when passed `--capture`:

```sh
bash macos/build.sh --run --preview --capture /tmp/matrix-gpui.png
```

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo build --locked -p twayblade
```

GPUI interaction tests run through Kit's headless test windows: actual buttons,
text input, emoji/accent text, room switching, draft restoration and profile
open/close. They do not establish VoiceOver/IME or physical platform acceptance.
CI is defined in `.github/workflows/check.yml`; inspect its actual results before
claiming it passed. Use `--offline` only after dependencies are cached.

## Isolated Matrix integration tests

With a **local** Docker engine and Compose running:

```sh
bash tests/integration/run.sh
```

See the [integration guide](tests/integration/README.md) for generated accounts,
loopback TLS, reproduction and exact limits. This test never accesses the normal
application credential entry. It checks core services and SDK store reopening;
it does not prove live GPUI login/restoration or Element interoperability.

## Optional Nix environment — macOS and Linux

The pinned shell supplies Rust, rustfmt, Clippy, pkg-config, CMake, Python,
Node.js and OpenSSL (the latter two support the isolated browser call peer),
plus Linux display, font and DBus inputs. Keep flake.lock committed.

```sh
nix develop
cargo run --locked -p twayblade
# macOS development bundle, in the same shell:
bash macos/build.sh --run
```

Without an interactive shell:

```sh
nix develop --command cargo test --locked --workspace --all-features
nix develop --command cargo run --locked -p twayblade -- --preview
```

Optional direnv: inspect `.envrc`, enable your existing direnv/nix-direnv setup,
then run `direnv allow`. It contains only `use flake`. Nix packages do not configure
the host GPU, desktop, user DBus session or keyring; non-NixOS graphics integration
may need host-specific setup. Apple signing tools are isolated from Nix SDK settings.

## AI design tooling

UI work uses the user-requested [UI/UX Pro Max skill](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill)
and GPUI Kit's component/coding/design guides. The selected skill revision is
recorded in AGENTS.md. Install it repo-locally with your agent's skill installer at
`.agents/skills/ui-ux-pro-max`; its canonical upstream path is
`.claude/skills/ui-ux-pro-max`. The local copy and AI tool state are ignored.
Python 3 is needed only for its local design searches, not to run the application.
There is no GPUI stack preset: use relevant UX/style domains and verify actual
GPUI Kit APIs. Do not apply web landing-page output to the desktop app.

## Local state and troubleshooting

The app uses credential service `twayblade.development`, account
`active-session-v1`. SDK stores live under ProjectDirs
`("", "Twayblade", "Twayblade")` in `sessions/<UUID>`, next to the
`instance.lock` file. The session and store passphrase stay in the OS credential
store; passwords are not saved. Logout revokes the device, then removes the
credential record and its store. Revoked-session recovery remains incomplete.

`.gitignore` excludes local credentials, SDK databases, test state, build outputs,
AI folders and installed skills. AGENTS.md, PLAN.md and product docs remain tracked.
Before publication, inspect staged changes and run a secret scanner.

- **Cargo build lock:** let the other build finish; do not remove lockfiles or
  kill unrelated processes.
- **Dependency fetch failure:** restore network access and retry; offline mode
  cannot replace the initial fetch.
- **Keychain/Secret Service unavailable:** unlock the correct desktop keyring and
  run as the same user. Offline previews are independent of credentials.
- **Empty Home:** rooms belonging to spaces appear only in those spaces. Initial
  membership loading also holds Home empty.
- **Missing history:** native sliding sync is required; no legacy fallback exists.
- **Unable to decrypt:** verification/recovery UI is pending. Preserve device
  keys and state; re-login or store deletion is not a recovery procedure.
