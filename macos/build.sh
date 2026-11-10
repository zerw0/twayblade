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
run=false
build=(cargo build --locked -p matrix-client-gpui)
app_args=()
for arg in "$@"; do
    case "$arg" in
        --run) run=true ;;
        *) app_args+=("$arg") ;;
    esac
    if [[ "$arg" == --capture ]]; then build+=(--features capture); fi
done
"${build[@]}"
app="$repo_root/target/debug/bundle/Matrix Client.app"
mkdir -p "$app/Contents/MacOS"
cp target/debug/matrix-client-gpui "$app/Contents/MacOS/"
cp macos/Info.plist "$app/Contents/Info.plist"
"${apple_env[@]}" /usr/bin/codesign --force --deep --sign - "$app"
printf 'Built %s\n' "$app"
if [[ "$run" == true ]]; then
    # Bash 3.2 (macOS /bin/bash) treats an empty array as unset under `set -u`.
    exec /usr/bin/open -n -W "$app" --args ${app_args[@]+"${app_args[@]}"}
fi
