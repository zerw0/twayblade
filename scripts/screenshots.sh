#!/usr/bin/env bash
# Retakes the README screenshots (docs/screenshots) on macOS: real window
# captures, with title bar and shadow, of the app's offline showcase preview.
#
#   bash macos/build.sh && bash scripts/screenshots.sh
#
# Needs Screen Recording permission for the terminal (System Settings ›
# Privacy & Security) and the Command Line Tools' swiftc. Leave the pointer
# alone while it runs; it is parked in a corner before each capture.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
app="$root/target/debug/bundle/Twayblade.app/Contents/MacOS/twayblade"
out="${OUT:-$root/docs/screenshots}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
[[ -x $app ]] || { echo "Build the app first: bash macos/build.sh" >&2; exit 1; }

# The window ID of a process (largest window, or with "front" the frontmost),
# and a pointer parker. The Nix shell's SDK cannot build these; use the CLT.
cat >"$work/winid.swift" <<'EOF'
import CoreGraphics
let pid = Int32(CommandLine.arguments[1])!
let front = CommandLine.arguments.count > 2
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
var best: (Int, Double) = (0, 0)
for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid && (w[kCGWindowLayer as String] as? Int) == 0 {
    let b = w[kCGWindowBounds as String] as! [String: Double]
    let area = b["Width"]! * b["Height"]!
    if area > best.1 { best = (w[kCGWindowNumber as String] as! Int, area) }
    if front && best.0 != 0 { break }
}
print(best.0)
EOF
cat >"$work/park.swift" <<'EOF'
import CoreGraphics
let b = CGDisplayBounds(CGMainDisplayID())
CGWarpMouseCursorPosition(CGPoint(x: b.minX + 2, y: b.maxY - 2))
EOF
for tool in winid park; do
    env -u SDKROOT DEVELOPER_DIR=/Library/Developer/CommandLineTools \
        /usr/bin/swiftc -O "$work/$tool.swift" -o "$work/$tool"
done

# shot <name> [--front] <preview flags...>
shot() {
    local name=$1 front=""
    shift
    [[ ${1:-} == --front ]] && { front=front; shift; }
    "$app" --preview-showcase "$@" >/dev/null 2>&1 &
    local pid=$! id=0
    for _ in $(seq 1 40); do
        id=$("$work/winid" "$pid")
        [[ $id != 0 ]] && break
        sleep 0.25
    done
    osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $pid) to true" >/dev/null 2>&1 || true
    "$work/park"
    sleep 4
    [[ -n $front ]] && id=$("$work/winid" "$pid" front)
    screencapture -x -l "$id" "$work/$name.png"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    sips -Z 1600 "$work/$name.png" --out "$out/$name.png" >/dev/null
    echo "$name"
}

mkdir -p "$out"
shot sign-in --preview-signin
shot choose-server --preview-signin --preview-servers
# The image message shows the welcome card, cropped from that capture at 2x
# and without its colour profile (the app shows Display P3 PNGs as a placeholder).
sips -c 620 900 --cropOffset 175 780 "$work/sign-in.png" --out "$work/card.png" >/dev/null
sips -Z 512 --deleteColorManagementProperties "$work/card.png" --out "$work/card.png" >/dev/null
image=(--preview-showcase-image "$work/card.png")
shot chat "${image[@]}"
shot thread "${image[@]}" --preview-thread
shot minimal-style "${image[@]}" --preview-style-minimal --preview-timestamps
shot room-info "${image[@]}" --preview-details
shot call "${image[@]}" --preview-call-group
shot settings --front "${image[@]}" --preview-settings appearance
shot search "${image[@]}" --preview-spotlight
shot notifications "${image[@]}" --preview-activity
shot polls "${image[@]}" --preview-poll
