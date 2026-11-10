#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
repo_root="$PWD"

# Nix sets DEVELOPER_DIR/SDKROOT and compiler variables. xcode-select itself
# respects DEVELOPER_DIR, so resolve Apple's selected tools in a clean process.
apple_env=(/usr/bin/env -i "HOME=$HOME" "USER=${USER:-$(/usr/bin/id -un)}"
    "TMPDIR=${TMPDIR:-/tmp}" 'PATH=/usr/bin:/bin:/usr/sbin:/sbin')
apple_developer_dir="$("${apple_env[@]}" /usr/bin/xcode-select -p)"
apple_env+=("DEVELOPER_DIR=$apple_developer_dir")
apple_sdk_root="$("${apple_env[@]}" /usr/bin/xcrun --sdk macosx --show-sdk-path)"
if [[ -d "$apple_developer_dir/SDKs/MacOSX26.5.sdk" ]]; then
    # CLT's 27.x SDK currently references a SwiftUI macro plugin it does not ship.
    apple_sdk_root="$apple_developer_dir/SDKs/MacOSX26.5.sdk"
fi
apple_sdk_root="${MATRIX_MACOS_SDK_ROOT:-$apple_sdk_root}"
apple_env+=("SDKROOT=$apple_sdk_root")
"${apple_env[@]}" /usr/bin/xcrun --find swift >/dev/null

# Use Cargo from the caller's environment: standard Rust or an optional Nix shell.
# Apple tooling above stays isolated from either environment.
cargo build --locked -p matrix-client-bridge
cargo run --locked -p matrix-client-bridge --features bindgen --bin matrix-bindgen -- \
    generate --library target/debug/libmatrix_client_bridge.dylib --language swift \
    --out-dir macos/Generated/raw
mkdir -p macos/Generated/Swift macos/Generated/FFI
cp macos/Generated/raw/MatrixBridge.swift macos/Generated/Swift/
cp macos/Generated/raw/MatrixBridgeFFI.h macos/Generated/FFI/
cp macos/Generated/raw/MatrixBridgeFFI.modulemap macos/Generated/FFI/module.modulemap

"${apple_env[@]}" \
    /usr/bin/xcrun swift build --package-path macos --configuration debug --sdk "$apple_sdk_root" --build-system native

app="$repo_root/macos/build/Matrix Client.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks"
cp macos/.build/debug/MatrixClientMac "$app/Contents/MacOS/"
cp target/debug/libmatrix_client_bridge.dylib "$app/Contents/Frameworks/"
cp macos/Info.plist "$app/Contents/Info.plist"
bridge_install_name="$("${apple_env[@]}" /usr/bin/otool -D target/debug/libmatrix_client_bridge.dylib | tail -n 1)"
"${apple_env[@]}" /usr/bin/install_name_tool -change "$bridge_install_name" '@rpath/libmatrix_client_bridge.dylib' "$app/Contents/MacOS/MatrixClientMac"
"${apple_env[@]}" /usr/bin/install_name_tool -id '@rpath/libmatrix_client_bridge.dylib' "$app/Contents/Frameworks/libmatrix_client_bridge.dylib"
if ! "${apple_env[@]}" /usr/bin/otool -l "$app/Contents/MacOS/MatrixClientMac" | rg -q '@executable_path/../Frameworks'; then
    "${apple_env[@]}" /usr/bin/install_name_tool -add_rpath '@executable_path/../Frameworks' "$app/Contents/MacOS/MatrixClientMac"
fi
"${apple_env[@]}" /usr/bin/codesign --force --deep --sign - "$app"
printf 'Built %s\n' "$app"
if [[ "${1:-}" == "--run" ]]; then
    shift
    exec /usr/bin/open -n -W "$app" --args "$@"
fi
