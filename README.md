<div align="center">

# Twayblade

**A native Matrix client for macOS, Linux and Windows, written in Rust.**

[Download](../../releases) · [Roadmap](PLAN.md) · [Development](DEVELOPMENT.md)

</div>

> [!NOTE]
> Twayblade is in early development and not ready for daily use yet.
> Try it with a test account.

## Features

- **End-to-end encryption** with device verification, secure backup and recovery
- **Sign in** with a password, OAuth or single sign-on; sessions live in your OS keychain
- **Spaces, rooms and direct messages**, with favourites, invites and a public room directory
- **Threads, replies, reactions**, formatted messages, images and files
- **Voice and video calls** on native MatrixRTC (LiveKit media, end-to-end encrypted, interoperable with Element Call), with screen sharing
- **Search** across conversations and messages, including by sender
- **Native notifications** that follow your Matrix settings
- **Offline mode**: saved conversations stay readable without a connection
- **Themes, scaling and chat styles**: Bubbles, IRC or Minimal

Calls are tested most on macOS. Webcam capture on Linux and Windows is
implemented but has not been tried on real hardware, and calls there still need
physical checks. Old-style `m.call.*` calls are not planned for 1.0. See the
[compatibility status](docs/compatibility/STATUS.md) for what has been verified.

Coming next: installers and signed packages, and in-app updates.

## Install

Grab the latest build from [Releases](../../releases).

Builds are not signed yet. On macOS, right-click **Twayblade.app** › **Open** the
first time; on Windows, choose **More info** › **Run anyway**.

Your homeserver needs native sliding sync, as in current Synapse releases.

## Build from source

Requires [Rust](https://rustup.rs) 1.99 or newer.

```sh
git clone https://github.com/zerw0/twayblade.git
cd twayblade
cargo run --release -p twayblade
```

Explore the interface without an account:

```sh
cargo run -p twayblade -- --preview
```

## License

Twayblade is licensed under the [MIT License](LICENSE). Third-party dependencies
retain their own licenses.
