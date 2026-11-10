//! Avatar thumbnails. Fetched through the SDK media cache and re-encoded as a
//! small square PNG, so the UI never decodes untrusted images on its thread.
use std::{
    io::Cursor,
    sync::{Arc, LazyLock},
    time::Duration,
};

use matrix_sdk::{
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{OwnedMxcUri, UInt, events::room::MediaSource},
};

use crate::auth::Account;

/// Bounded so a room with hundreds of members cannot flood the homeserver.
static FETCHES: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(4)));
const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_DIMENSION: u32 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AvatarError {
    #[error("avatar_invalid")]
    Invalid,
    #[error("avatar_download")]
    Download,
    #[error("avatar_decode")]
    Decode,
}

/// Decode with limits, crop to a centred square and encode as PNG.
pub fn square_png(bytes: &[u8], size: u32) -> Result<Vec<u8>, AvatarError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(AvatarError::Decode);
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| AvatarError::Decode)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| AvatarError::Decode)?
        .resize_to_fill(size, size, image::imageops::FilterType::Triangle);
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| AvatarError::Decode)?;
    Ok(png.into_inner())
}

impl Account {
    /// `size` is in physical pixels; request twice the point size for Retina.
    pub async fn avatar_thumbnail(&self, mxc: String, size: u32) -> Result<Vec<u8>, AvatarError> {
        let uri = OwnedMxcUri::from(mxc);
        if !uri.is_valid() {
            return Err(AvatarError::Invalid);
        }
        let _permit = FETCHES
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AvatarError::Download)?;
        let request = MediaRequestParameters {
            source: MediaSource::Plain(uri),
            format: MediaFormat::Thumbnail(MediaThumbnailSettings::new(
                UInt::from(size),
                UInt::from(size),
            )),
        };
        let bytes = tokio::time::timeout(
            Duration::from_secs(20),
            self.client.media().get_media_content(&request, true),
        )
        .await
        .map_err(|_| AvatarError::Download)?
        .map_err(|_| AvatarError::Download)?;
        tokio::task::spawn_blocking(move || square_png(&bytes, size))
            .await
            .map_err(|_| AvatarError::Decode)?
    }

    /// The signed-in user's own avatar URL, from the server profile.
    pub async fn own_avatar_url(&self) -> Option<String> {
        tokio::time::timeout(
            Duration::from_secs(15),
            self.client.account().get_avatar_url(),
        )
        .await
        .ok()?
        .ok()?
        .map(|url| url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatars_are_cropped_square_and_bad_input_is_rejected() {
        let mut source = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(300, 120)
            .write_to(&mut source, image::ImageFormat::Png)
            .unwrap();
        let png = square_png(&source.into_inner(), 64).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 64));
        assert_eq!(square_png(b"not an image", 64), Err(AvatarError::Decode));
    }
}
