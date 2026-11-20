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
build=(python3 scripts/cargo-pruned build --locked -p twayblade)
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
app="$repo_root/target/$profile/bundle/Twayblade.app"
mkdir -p "$app/Contents/MacOS"
cp "target/$profile/twayblade" "$app/Contents/MacOS/"
# macOS 26 draws its current window controls (and other system chrome) only for
# apps built against its SDK; the pinned Nix SDK is older and would get the
# small legacy traffic lights. Record SDK 26.0 in the executable, keeping the
# deployment target; the bundle is signed afterwards.
"${apple_env[@]}" /usr/bin/vtool -set-build-version macos 14.0 26.0 -replace \
    -output "$app/Contents/MacOS/twayblade" "$app/Contents/MacOS/twayblade"
cp macos/Info.plist "$app/Contents/Info.plist"
# The bundle shows the Cargo version; CFBundleShortVersionString takes only
# the numeric part ("0.1.1-rc.1" -> "0.1.1").
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${version%%[-+]*}" "$app/Contents/Info.plist"
# Local builds keep the development identifier, so they never share
# notification permission or preferences with an installed release. CI sets
# TWAYBLADE_BUNDLE_ID=com.twayblade.desktop for the distributed bundle.
if [[ -n "${TWAYBLADE_BUNDLE_ID:-}" ]]; then
    if [[ ! "$TWAYBLADE_BUNDLE_ID" =~ ^[A-Za-z0-9.-]+$ ]]; then
        printf 'TWAYBLADE_BUNDLE_ID is not a bundle identifier.\n' >&2
        exit 1
    fi
    /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $TWAYBLADE_BUNDLE_ID" "$app/Contents/Info.plist"
fi
# Remove the superseded helper from a previously built development bundle.
rm -rf "$app/Contents/Helpers/Twayblade Call.app"
mkdir -p "$app/Contents/Resources/licenses"
cp assets/Twayblade.icns "$app/Contents/Resources/"
cp crates/media/licenses/libwebrtc-a545de4.md "$app/Contents/Resources/licenses/"
# A stable identity keeps Keychain approval across rebuilds; ad-hoc signing
# ("-") changes the signature every build. Create one with
# macos/create-dev-certificate.sh, or set TWAYBLADE_SIGN_IDENTITY.
identity="${TWAYBLADE_SIGN_IDENTITY:-}"
if [[ -z "$identity" ]] && "${apple_env[@]}" /usr/bin/security find-identity -p codesigning 2>/dev/null \
    | grep -q '"Twayblade Development"'; then
    identity="Twayblade Development"
elif [[ -z "$identity" ]] && "${apple_env[@]}" /usr/bin/security find-identity -p codesigning 2>/dev/null \
    | grep -q '"Matrix Client Development"'; then
    # Identity created before the rename; still stable across builds.
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
