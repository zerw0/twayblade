# Linux and Windows webcam capture

Webcam capture on Linux and Windows (2026-11-15, `feature/camera-linux-windows`,
issue #48). Linux uses Video4Linux2 (mmap streaming, YUYV or NV12, devices are
`/dev/video*` capture nodes); Windows uses Media Foundation (source reader,
NV12 output, devices by symbolic link). Both pick 720p at 15 fps or more when
offered, run on a dedicated `twayblade-camera` thread and feed the same
`NativeVideoSource` path as macOS. Shared conversion and mode choice live in
`camera/convert.rs` (YUYV to I420, bounds checks, mode ranking).

Verified: the shared converter and mode choice by offline tests; the macOS
build, strict Clippy and tests. The Linux and Windows backends are not
compiled on the development Mac; only CI's ubuntu and windows checks compile
them. Not verified: any real webcam, Element receiving the video, permission
behaviour (Windows privacy settings, V4L2 device access). Known gaps:
MJPEG-only cameras are unsupported, stepwise V4L2 sizes are only used for
720p, the Flatpak camera portal is not used, and closing a camera takes up to
one frame interval. Physical checks belong to #49.
