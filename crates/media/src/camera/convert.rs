//! Frame conversion and capture-mode choice shared by the Linux and Windows
//! camera backends. Safe Rust only; the OS code hands in bounded byte slices.
#![cfg_attr(
    not(any(target_os = "linux", test)),
    allow(
        dead_code,
        reason = "mode choice and YUYV conversion are Linux-only; Windows uses the size limits and NV12 length"
    )
)]
use livekit::webrtc::video_frame::I420Buffer;

/// Largest frame accepted from any camera, matching the macOS backend.
pub const MAX_WIDTH: u32 = 1920;
pub const MAX_HEIGHT: u32 = 1080;
const MAX_STRIDE: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Yuyv,
    Nv12,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    /// Frames per second, rounded down; 0 when the device does not say.
    pub fps: u32,
}

/// Picks the mode to request: 1280x720 at 15 fps or more first, then the
/// largest size up to 720p at 15 fps or more, then the largest size up to
/// 1080p. Ties prefer NV12 (no per-pixel unpacking), then the higher rate.
pub fn choose_mode(modes: &[Mode]) -> Option<Mode> {
    let usable = |m: &&Mode| {
        m.width >= 2
            && m.height >= 2
            && m.width.is_multiple_of(2)
            && m.height.is_multiple_of(2)
            && m.width <= MAX_WIDTH
            && m.height <= MAX_HEIGHT
    };
    let rank = |m: &Mode| {
        let exact = m.width == 1280 && m.height == 720;
        let tier = if m.fps >= 15 && exact {
            3
        } else if m.fps >= 15 && m.height <= 720 {
            2
        } else {
            1
        };
        (
            tier,
            u64::from(m.width) * u64::from(m.height),
            m.format == PixelFormat::Nv12,
            m.fps,
        )
    };
    modes.iter().filter(usable).copied().max_by_key(rank)
}

fn dimensions_ok(width: u32, height: u32) -> bool {
    width >= 2
        && height >= 2
        && width.is_multiple_of(2)
        && height.is_multiple_of(2)
        && width <= MAX_WIDTH
        && height <= MAX_HEIGHT
}

/// Packed YUYV (YUY2) to planar I420. Chroma of two rows is averaged.
/// Returns `None` for odd or oversized frames, a short stride or short data.
pub fn yuyv_to_i420(src: &[u8], stride: usize, width: u32, height: u32) -> Option<I420Buffer> {
    if !dimensions_ok(width, height) {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    if stride < w * 2 || stride > MAX_STRIDE || src.len() < stride * (h - 1) + w * 2 {
        return None;
    }
    let mut buffer = I420Buffer::new(width, height);
    let (sy, su, sv) = buffer.strides();
    let (y, u, v) = buffer.data_mut();
    let (sy, su, sv) = (sy as usize, su as usize, sv as usize);
    for row in 0..h {
        let line = &src[row * stride..row * stride + w * 2];
        for (x, luma) in line.iter().step_by(2).enumerate() {
            y[row * sy + x] = *luma;
        }
    }
    for row in (0..h).step_by(2) {
        let top = &src[row * stride..row * stride + w * 2];
        let bottom = &src[(row + 1) * stride..(row + 1) * stride + w * 2];
        for x in 0..w / 2 {
            let i = x * 4;
            let avg = |a: u8, b: u8| (u16::from(a) + u16::from(b)).div_ceil(2) as u8;
            u[(row / 2) * su + x] = avg(top[i + 1], bottom[i + 1]);
            v[(row / 2) * sv + x] = avg(top[i + 3], bottom[i + 3]);
        }
    }
    Some(buffer)
}

/// Validates an NV12 frame's layout: Y plane then interleaved UV, with the
/// given row stride. Returns the byte length the frame must have at least.
pub fn nv12_len(stride: usize, width: u32, height: u32) -> Option<usize> {
    if !dimensions_ok(width, height) || stride < width as usize || stride > MAX_STRIDE {
        return None;
    }
    Some(stride * height as usize * 3 / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(format: PixelFormat, width: u32, height: u32, fps: u32) -> Mode {
        Mode {
            format,
            width,
            height,
            fps,
        }
    }

    #[test]
    fn yuyv_planes_and_chroma_average() {
        // 2x2: row 0 Y=10,20 U=100 V=200; row 1 Y=30,40 U=120 V=220.
        let src = [10, 100, 20, 200, 30, 120, 40, 220];
        let mut buffer = yuyv_to_i420(&src, 4, 2, 2).expect("valid frame");
        let (sy, su, sv) = buffer.strides();
        let (y, u, v) = buffer.data_mut();
        assert_eq!(&y[..2], &[10, 20]);
        assert_eq!(&y[sy as usize..sy as usize + 2], &[30, 40]);
        assert_eq!(u[0], 110);
        assert_eq!(v[0], 210);
        let _ = (su, sv);
    }

    #[test]
    fn yuyv_honours_a_padded_stride() {
        let mut src = vec![0u8; 8 * 2];
        src[..4].copy_from_slice(&[1, 50, 2, 60]);
        src[8..12].copy_from_slice(&[3, 70, 4, 80]);
        let mut buffer = yuyv_to_i420(&src, 8, 2, 2).expect("padded stride");
        let (_, _, _) = buffer.strides();
        let (_, u, v) = buffer.data_mut();
        assert_eq!((u[0], v[0]), (60, 70));
    }

    #[test]
    fn yuyv_rejects_bad_frames() {
        let src = vec![0u8; 64];
        assert!(yuyv_to_i420(&src, 4, 3, 2).is_none(), "odd width");
        assert!(yuyv_to_i420(&src, 4, 2, 3).is_none(), "odd height");
        assert!(yuyv_to_i420(&src, 3, 2, 2).is_none(), "short stride");
        assert!(yuyv_to_i420(&src[..6], 4, 2, 2).is_none(), "short data");
        assert!(yuyv_to_i420(&src, 4, 0, 2).is_none(), "empty");
        let big = vec![0u8; 4096 * 2 * 2];
        assert!(yuyv_to_i420(&big, 4096 * 2, 4096, 2).is_none(), "too wide");
        assert!(yuyv_to_i420(&big, 20_000, 2, 2).is_none(), "huge stride");
    }

    #[test]
    fn nv12_layout_is_checked() {
        assert_eq!(nv12_len(1280, 1280, 720), Some(1280 * 720 * 3 / 2));
        assert_eq!(nv12_len(1280, 1281, 720), None);
        assert_eq!(nv12_len(100, 1280, 720), None);
        assert_eq!(nv12_len(1920, 3840, 2160), None);
    }

    #[test]
    fn mode_choice_prefers_720p() {
        let modes = [
            mode(PixelFormat::Yuyv, 640, 480, 30),
            mode(PixelFormat::Yuyv, 1280, 720, 10),
            mode(PixelFormat::Yuyv, 1280, 720, 30),
            mode(PixelFormat::Yuyv, 1920, 1080, 5),
        ];
        let chosen = choose_mode(&modes).expect("a mode");
        assert_eq!((chosen.width, chosen.height, chosen.fps), (1280, 720, 30));
    }

    #[test]
    fn mode_choice_falls_back_and_skips_unusable() {
        let modes = [
            mode(PixelFormat::Yuyv, 640, 480, 30),
            mode(PixelFormat::Yuyv, 800, 600, 24),
            mode(PixelFormat::Nv12, 3840, 2160, 30),
            mode(PixelFormat::Yuyv, 641, 481, 30),
        ];
        let chosen = choose_mode(&modes).expect("a mode");
        assert_eq!((chosen.width, chosen.height), (800, 600));
        assert!(choose_mode(&[mode(PixelFormat::Yuyv, 3840, 2160, 30)]).is_none());
        assert!(choose_mode(&[]).is_none());
    }

    #[test]
    fn mode_choice_prefers_nv12_on_ties() {
        let modes = [
            mode(PixelFormat::Yuyv, 1280, 720, 30),
            mode(PixelFormat::Nv12, 1280, 720, 30),
        ];
        assert_eq!(
            choose_mode(&modes).map(|m| m.format),
            Some(PixelFormat::Nv12)
        );
    }
}
