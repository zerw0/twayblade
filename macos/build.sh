#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
repo_root="$PWD"

# Cargo comes from the caller's toolchain (standard Rust or an optional Nix shell).
# Keep Apple's signing/tool-discovery environment separate from Nix SDK overrides.
apple_env=(/usr/bin/env -i "HOME=$HOME" "USER=${USER:-$(/usr/bin/id -un)}"
    "TMPDIR=${TMPDIR:-/tmp}" 'PATH=/usr/bin:/bin:/usr/sbin:/sbin')
apple_developer_dir="$("${apple_env[@]}" /usr/bin/xcode-select -p)"
apple_env+=("DEVELOPER_DIR=$apple_developer_dir")
features=()
for arg in "$@"; do
    if [[ "$arg" == --capture ]]; then features=(--features capture); fi
done
cargo build --locked -p matrix-client-gpui "${features[@]}"
app="$repo_root/target/debug/bundle/Matrix Client.app"
mkdir -p "$app/Contents/MacOS"
cp target/debug/matrix-client-gpui "$app/Contents/MacOS/"
cp macos/Info.plist "$app/Contents/Info.plist"
"${apple_env[@]}" /usr/libexec/PlistBuddy -c 'Set :CFBundleExecutable matrix-client-gpui' "$app/Contents/Info.plist"
"${apple_env[@]}" /usr/bin/codesign --force --deep --sign - "$app"
printf 'Built %s\n' "$app"
if [[ "${1:-}" == --run ]]; then
    shift
    exec /usr/bin/open -n -W "$app" --args "$@"
fi
