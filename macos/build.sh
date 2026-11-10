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
preview=false
profile=debug
build=(cargo build --locked -p matrix-client-gpui)
app_args=()
for arg in "$@"; do
    case "$arg" in
        --run) run=true ;;
        # Optimised build for everyday use; debug builds are for development.
        --release) profile=release ;;
        --preview|--preview-*) preview=true; app_args+=("$arg") ;;
        *) app_args+=("$arg") ;;
    esac
    if [[ "$arg" == --capture ]]; then build+=(--features capture); fi
done
if [[ "$profile" == release ]]; then build+=(--release); fi
"${build[@]}"
app="$repo_root/target/$profile/bundle/Matrix Client.app"
mkdir -p "$app/Contents/MacOS"
cp "target/$profile/matrix-client-gpui" "$app/Contents/MacOS/"
cp macos/Info.plist "$app/Contents/Info.plist"
# A stable identity keeps Keychain approval across rebuilds; ad-hoc signing
# ("-") changes the signature every build. Create one with
# macos/create-dev-certificate.sh, or set MATRIX_CLIENT_SIGN_IDENTITY.
identity="${MATRIX_CLIENT_SIGN_IDENTITY:-}"
if [[ -z "$identity" ]] && "${apple_env[@]}" /usr/bin/security find-identity -p codesigning 2>/dev/null \
    | grep -q '"Matrix Client Development"'; then
    identity="Matrix Client Development"
fi
if [[ -z "$identity" ]]; then
    identity=-
    printf 'Signing ad hoc: macOS will ask for Keychain access after each build.\n' >&2
    printf 'Run macos/create-dev-certificate.sh once to stop that.\n' >&2
fi
"${apple_env[@]}" /usr/bin/codesign --force --deep --sign "$identity" "$app"
printf 'Built %s\n' "$app"
if [[ "$run" == true ]]; then
    # Bash 3.2 (macOS /bin/bash) treats an empty array as unset under `set -u`.
    # Connected launches reuse the existing instance; offline previews are independent.
    open_args=(-W)
    if [[ "$preview" == true ]]; then open_args+=(-n); fi
    exec /usr/bin/open "${open_args[@]}" "$app" --args ${app_args[@]+"${app_args[@]}"}
fi
