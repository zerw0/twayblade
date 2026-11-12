//! Explicit physical-pixel timeline previews. Full-size viewer is independent.
use super::{Conversation, ImageError};
use matrix_sdk::media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings};
use std::sync::Arc;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PreviewSize {
    #[default]
    Default,
    Large,
}
impl PreviewSize {
    /// Long-edge pixels, independent of display scale or layout points.
    pub const fn pixels(self) -> u32 {
        match self {
            Self::Default => 512,
            Self::Large => 1280,
        }
    }
}
#[derive(Clone)]
pub struct TimelinePreview {
    pub png: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}
impl TimelinePreview {
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}
impl Conversation {
    /// No persistent media cache. Prefer a homeserver thumbnail; if unavailable
    /// (including encrypted originals), decode the original under the same permit
    /// and limits as other previews. Stable timeline IDs are never replaced.
    pub async fn attachment_preview(
        &self,
        id: &str,
        size: PreviewSize,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<TimelinePreview, ImageError> {
        if *cancel.borrow() {
            return Err(ImageError::Cancelled);
        }
        let source = self
            .media_source(id)
            .await
            .map_err(|_| ImageError::Unavailable)?;
        let client = self.timeline.room().client();
        let media = client.media();
        let request = MediaRequestParameters {
            source: source.clone(),
            format: MediaFormat::Thumbnail(MediaThumbnailSettings::new(
                size.pixels().into(),
                size.pixels().into(),
            )),
        };
        let thumbnail = tokio::select! {
            _ = cancel.changed() => return Err(ImageError::Cancelled),
            result = tokio::time::timeout(std::time::Duration::from_secs(15), media.get_media_content(&request, false)) => result.ok().and_then(Result::ok),
        };
        if let Some(bytes) = thumbnail {
            match super::viewer::decode_timeline_preview(bytes, size, cancel.clone()).await {
                Ok(preview) => return Ok(preview),
                Err(ImageError::Cancelled) => return Err(ImageError::Cancelled),
                Err(_) => {}
            }
        }
        let request = MediaRequestParameters {
            source,
            format: MediaFormat::File,
        };
        let bytes = tokio::select! {
            _ = cancel.changed() => return Err(ImageError::Cancelled),
            result = tokio::time::timeout(std::time::Duration::from_secs(30), media.get_media_content(&request, false)) => result.map_err(|_| ImageError::Download)?.map_err(|_| ImageError::Download)?,
        };
        super::viewer::decode_timeline_preview(bytes, size, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn sizes_preserve_aspect_ratio_and_obey_explicit_pixel_caps() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2048, 1024)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let (_cancel, cancel) = tokio::sync::watch::channel(false);
        for (size, width, height) in [
            (PreviewSize::Default, 512, 256),
            (PreviewSize::Large, 1280, 640),
        ] {
            let preview = super::super::viewer::decode_timeline_preview(
                bytes.get_ref().clone(),
                size,
                cancel.clone(),
            )
            .await
            .unwrap();
            assert_eq!((preview.width, preview.height), (width, height));
            let png = image::load_from_memory(&preview.png).unwrap();
            assert_eq!((png.width(), png.height()), (width, height));
        }
        let (sender, cancel) = tokio::sync::watch::channel(true);
        assert!(matches!(
            super::super::viewer::decode_timeline_preview(Vec::new(), PreviewSize::Large, cancel)
                .await,
            Err(ImageError::Cancelled)
        ));
        drop(sender);
    }
}
