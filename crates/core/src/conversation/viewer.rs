//! Full-resolution media is decrypted by the SDK in memory, with its disk cache off.
use super::{Conversation, MessageType, sanitize_filename};
use image::{AnimationDecoder, ImageDecoder};
use matrix_sdk::media::{MediaFormat, MediaRequestParameters};
use std::{
    io::Cursor,
    path::Path,
    sync::{Arc, LazyLock},
};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_DIMENSION: u32 = 8192;
/// Total decoded budget for animations (all frames together).
const MAX_DECODED: u64 = 96 * 1024 * 1024;
/// A single still may use the full 8192² RGBA budget while decoding, so 48 MP
/// phone photos open; it is then scaled down for display.
const MAX_STILL_DECODED: u64 = MAX_DIMENSION as u64 * MAX_DIMENSION as u64 * 4;
/// Longest side kept in memory for display. Save/Open use the original bytes.
const MAX_DISPLAY: u32 = 4096;
const MAX_FRAMES: usize = 240;
static DECODER: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(1)));

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("image_unavailable")]
    Unavailable,
    #[error("image_download")]
    Download,
    #[error("image_limits")]
    Limit,
    #[error("image_corrupt")]
    Corrupt,
    #[error("image_cancelled")]
    Cancelled,
    #[error("image_save")]
    Save,
}
pub struct ViewerImage {
    pub filename: String,
    pub width: u32,
    pub height: u32,
    /// RGBA frames; the frontend converts them to its renderer's channel order.
    pub frames: Vec<image::Frame>,
    pub clipboard_png: Arc<Vec<u8>>,
    original: Arc<Vec<u8>>,
}
impl ViewerImage {
    /// Call only on a blocking worker after an explicit user destination choice.
    pub fn save_blocking(&self, path: &Path) -> Result<(), ImageError> {
        super::attachments::atomic_write_blocking(&self.original, path)
            .map_err(|_| ImageError::Save)
    }

    pub async fn save(&self, path: &Path) -> Result<(), ImageError> {
        super::attachments::atomic_write(self.original.clone(), path.to_path_buf())
            .await
            .map_err(|_| ImageError::Save)
    }
}
fn limits_with(max_alloc: u64) -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(max_alloc);
    limits
}
fn limits() -> image::Limits {
    limits_with(MAX_DECODED)
}
fn display_frame(image: image::DynamicImage) -> image::Frame {
    let image = if image.width().max(image.height()) > MAX_DISPLAY {
        image.resize(
            MAX_DISPLAY,
            MAX_DISPLAY,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    image::Frame::new(image.into_rgba8())
}
fn image_error(e: image::ImageError) -> ImageError {
    if matches!(e, image::ImageError::Limits(_)) {
        ImageError::Limit
    } else {
        ImageError::Corrupt
    }
}
fn dimensions(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Corrupt);
    }
    if width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) * 4 > MAX_STILL_DECODED
    {
        return Err(ImageError::Limit);
    }
    Ok(())
}
fn collect_frames(frames: image::Frames<'_>) -> Result<Vec<image::Frame>, ImageError> {
    let mut decoded = Vec::new();
    let mut total = 0u64;
    for frame in frames {
        let frame = frame.map_err(image_error)?;
        total += frame.buffer().len() as u64;
        if total > MAX_DECODED || decoded.len() >= MAX_FRAMES {
            return Err(ImageError::Limit);
        }
        decoded.push(frame);
    }
    if decoded.is_empty() {
        return Err(ImageError::Corrupt);
    }
    Ok(decoded)
}
/// Pure bounded decode, also used by offline samples and codec tests.
pub fn decode_viewer_image(bytes: Vec<u8>, filename: &str) -> Result<ViewerImage, ImageError> {
    if bytes.len() > MAX_BYTES {
        return Err(ImageError::Limit);
    }
    let format = image::guess_format(&bytes).map_err(image_error)?;
    let reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
    let (width, height) = reader.into_dimensions().map_err(image_error)?;
    dimensions(width, height)?;
    let frames = match format {
        image::ImageFormat::Gif => {
            let mut decoder =
                image::codecs::gif::GifDecoder::new(Cursor::new(&bytes)).map_err(image_error)?;
            decoder.set_limits(limits()).map_err(image_error)?;
            collect_frames(decoder.into_frames())?
        }
        image::ImageFormat::WebP => {
            let mut decoder =
                image::codecs::webp::WebPDecoder::new(Cursor::new(&bytes)).map_err(image_error)?;
            decoder.set_limits(limits()).map_err(image_error)?;
            if decoder.has_animation() {
                collect_frames(decoder.into_frames())?
            } else {
                decoder
                    .set_limits(limits_with(MAX_STILL_DECODED))
                    .map_err(image_error)?;
                vec![display_frame(
                    image::DynamicImage::from_decoder(decoder).map_err(image_error)?,
                )]
            }
        }
        _ => {
            let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
            reader.limits(limits_with(MAX_STILL_DECODED));
            vec![display_frame(reader.decode().map_err(image_error)?)]
        }
    };
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(frames[0].buffer().clone())
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(image_error)?;
    Ok(ViewerImage {
        filename: sanitize_filename(filename),
        width,
        height,
        frames,
        clipboard_png: Arc::new(png.into_inner()),
        original: Arc::new(bytes),
    })
}
impl Conversation {
    pub async fn viewer_image(
        &self,
        id: &str,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<ViewerImage, ImageError> {
        if *cancel.borrow() {
            return Err(ImageError::Cancelled);
        }
        let (filename, size) = {
            let items = self.timeline.items().await;
            let content = items
                .iter()
                .find(|item| item.unique_id().0 == id)
                .and_then(|i| i.as_event())
                .and_then(|e| e.content().as_message())
                .ok_or(ImageError::Unavailable)?;
            let MessageType::Image(image) = content.msgtype() else {
                return Err(ImageError::Unavailable);
            };
            (
                image.filename().to_owned(),
                image.info.as_ref().and_then(|i| i.size).map(u64::from),
            )
        };
        if size.is_some_and(|size| size > MAX_BYTES as u64) {
            return Err(ImageError::Limit);
        }
        let source = self
            .media_source(id)
            .await
            .map_err(|_| ImageError::Unavailable)?;
        let client = self.timeline.room().client();
        let media = client.media();
        let request = MediaRequestParameters {
            source,
            format: MediaFormat::File,
        };
        let bytes = tokio::select! {
            _ = cancel.changed() => return Err(ImageError::Cancelled),
            result = tokio::time::timeout(std::time::Duration::from_secs(30),media.get_media_content(&request,false)) => result.map_err(|_| ImageError::Download)?.map_err(|_| ImageError::Download)?,
        };
        decode_viewer_bytes(bytes, filename, cancel).await
    }
}
/// One process-wide decoder permit is shared by timeline and gallery images.
pub(crate) async fn decode_viewer_bytes(
    bytes: Vec<u8>,
    filename: String,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<ViewerImage, ImageError> {
    if *cancel.borrow() {
        return Err(ImageError::Cancelled);
    }
    let permit = tokio::select! {
        _ = cancel.changed() => return Err(ImageError::Cancelled),
        permit = DECODER.clone().acquire_owned() => permit.map_err(|_| ImageError::Cancelled)?,
    };
    let decode = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        decode_viewer_image(bytes, &filename)
    });
    tokio::select! {
        _ = cancel.changed() => Err(ImageError::Cancelled),
        result = decode => result.map_err(|_| ImageError::Corrupt)?,
    }
}

/// First frame only, sharing the viewer/timeline decoder semaphore.
pub(crate) async fn decode_gallery_thumbnail(
    bytes: Vec<u8>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<Arc<Vec<u8>>, ImageError> {
    if *cancel.borrow() {
        return Err(ImageError::Cancelled);
    }
    let permit = tokio::select! {
        _ = cancel.changed() => return Err(ImageError::Cancelled),
        permit = DECODER.clone().acquire_owned() => permit.map_err(|_| ImageError::Cancelled)?,
    };
    let decode = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        thumbnail_png(bytes).map(Arc::new)
    });
    tokio::select! {
        _ = cancel.changed() => Err(ImageError::Cancelled),
        result = decode => result.map_err(|_| ImageError::Corrupt)?,
    }
}
fn thumbnail_png(bytes: Vec<u8>) -> Result<Vec<u8>, ImageError> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(ImageError::Limit);
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ImageError::Corrupt)?;
    reader.limits(limits_with(64 * 1024 * 1024));
    let image = reader.decode().map_err(image_error)?.thumbnail(168, 168);
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(image_error)?;
    Ok(out.into_inner())
}

#[cfg(test)]
pub(crate) fn animated_webp_fixture() -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(tag);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        if !data.len().is_multiple_of(2) {
            out.push(0);
        }
    }
    let mut body = b"WEBP".to_vec();
    chunk(&mut body, b"VP8X", &[2, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
    chunk(&mut body, b"ANIM", &[0, 0, 0, 0, 0, 0]);
    for color in [[255, 0, 0, 255], [0, 255, 0, 255]] {
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .encode(&color.repeat(4), 2, 2, image::ExtendedColorType::Rgba8)
            .unwrap();
        let mut frame = vec![0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 80, 0, 0, 2];
        frame.extend_from_slice(&encoded[12..]);
        chunk(&mut body, b"ANMF", &frame);
    }
    let mut data = b"RIFF".to_vec();
    data.extend_from_slice(&(body.len() as u32).to_le_bytes());
    data.extend(body);
    data
}
#[cfg(test)]
pub(crate) fn animated_gif_fixture() -> Vec<u8> {
    let mut data = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
        encoder
            .encode_frames((0..2).map(|_| {
                image::Frame::from_parts(
                    image::RgbaImage::new(2, 2),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(80, 1),
                )
            }))
            .unwrap();
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gallery_thumbnails_bound_encoded_input_decode_and_output() {
        assert!(matches!(
            thumbnail_png(vec![0; 16 * 1024 * 1024 + 1]),
            Err(ImageError::Limit)
        ));
        assert!(matches!(
            thumbnail_png(vec![1, 2, 3]),
            Err(ImageError::Corrupt)
        ));
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(512, 256)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let bytes = thumbnail_png(png.into_inner()).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (168, 84));
    }
    #[test]
    fn animated_webp_keeps_composited_frames_and_delays() {
        let image = decode_viewer_image(animated_webp_fixture(), "sample.webp").unwrap();
        assert_eq!(image.frames.len(), 2);
        assert_eq!(image.frames[0].delay().numer_denom_ms(), (80, 1));
        assert_eq!(image.frames[0].buffer().get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(image.frames[1].buffer().get_pixel(0, 0).0, [0, 255, 0, 255]);
    }
    #[test]
    fn large_photos_open_scaled_for_display_with_original_dimensions() {
        // 6000 × 5000 is over the animation budget but a normal camera photo.
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(6000, 5000)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let image = decode_viewer_image(bytes.into_inner(), "photo.png").unwrap();
        assert_eq!((image.width, image.height), (6000, 5000));
        let frame = image.frames[0].buffer();
        assert!(frame.width().max(frame.height()) <= MAX_DISPLAY);
    }
    #[test]
    fn oversized_and_corrupt_images_have_distinct_typed_failures() {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(9000, 1)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert!(matches!(
            decode_viewer_image(bytes.into_inner(), "large.png"),
            Err(ImageError::Limit)
        ));
        assert!(matches!(
            decode_viewer_image(vec![1, 2, 3], "bad.png"),
            Err(ImageError::Corrupt)
        ));
    }
    #[test]
    fn animated_gif_keeps_frames_and_delays_and_bounds_total_allocation() {
        let mut data = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
            let frames = (0..2).map(|_| {
                image::Frame::from_parts(
                    image::RgbaImage::new(2, 2),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(80, 1),
                )
            });
            encoder.encode_frames(frames).unwrap();
        }
        let image = decode_viewer_image(data, "../../sample.gif").unwrap();
        assert_eq!(image.frames.len(), 2);
        assert_eq!(image.filename, "sample.gif");
        assert_eq!(image.frames[0].delay().numer_denom_ms(), (80, 1));
        let mut many = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut many);
            encoder
                .encode_frames((0..241).map(|_| image::Frame::new(image::RgbaImage::new(2, 2))))
                .unwrap();
        }
        assert!(matches!(
            decode_viewer_image(many, "many.gif"),
            Err(ImageError::Limit)
        ));
    }
    #[tokio::test]
    async fn cancelled_decoder_waiters_do_not_enqueue_blocking_work() {
        let permit = DECODER.clone().acquire_owned().await.unwrap();
        let (sender, receiver) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(decode_viewer_bytes(
            Vec::new(),
            "cancelled.png".into(),
            receiver.clone(),
        ));
        let thumbnail = tokio::spawn(decode_gallery_thumbnail(Vec::new(), receiver));
        tokio::task::yield_now().await;
        sender.send_replace(true);
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(ImageError::Cancelled)));
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), thumbnail)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(ImageError::Cancelled)));
        drop(permit);
    }
}
