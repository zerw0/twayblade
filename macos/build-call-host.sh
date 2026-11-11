#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
profile="${1:-debug}"
build=(cargo build --locked -p twayblade-call-host)
if [[ "$profile" == release ]]; then build+=(--release); fi
"${build[@]}"
helper="target/$profile/bundle/Twayblade Call.app"
mkdir -p "$helper/Contents/MacOS"
cp "target/$profile/twayblade-call-host" "$helper/Contents/MacOS/"
cp macos/CallHost-Info.plist "$helper/Contents/Info.plist"
python3 crates/call-host/prepare-assets.py --dest "$helper/Contents/Resources/element-call"
/usr/bin/env -i "HOME=$HOME" "USER=${USER:-$(/usr/bin/id -un)}" 'PATH=/usr/bin:/bin:/usr/sbin:/sbin' \
    /usr/bin/codesign --force --deep --sign - "$helper"
