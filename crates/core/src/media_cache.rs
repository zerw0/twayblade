//! On-device media cache (#174). Images and thumbnails are kept in the
//! account's SDK media store, which encrypts content with the store key from
//! OS secret storage; no decrypted files are written. A size limit bounds it
//! and entries unused for a while expire. With the cache off, media stays in
//! memory only, as before.
use crate::auth::Account;
use matrix_sdk::media::{MediaRequestParameters, MediaRetentionPolicy};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Cached media not opened for this long is removed on the next cleanup.
const EXPIRY: Duration = Duration::from_secs(60 * 60 * 24 * 30);
/// How often the SDK checks the limits while the app runs.
const CLEANUP: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, thiserror::Error)]
pub enum MediaCacheError {
    #[error("media_cache_unavailable")]
    Unavailable,
}

/// The retention policy for a size limit in bytes (`None` = cache off).
pub(crate) fn policy(limit: Option<u64>) -> MediaRetentionPolicy {
    MediaRetentionPolicy::new()
        .with_max_cache_size(Some(limit.unwrap_or(0)))
        .with_last_access_expiry(Some(EXPIRY))
        .with_cleanup_frequency(Some(CLEANUP))
}

impl Account {
    /// Keep media on this device up to `limit` bytes, or not at all (`None`).
    /// Lowering the limit trims the cache at once.
    pub async fn set_media_cache_limit(&self, limit: Option<u64>) -> Result<(), MediaCacheError> {
        self.media_cache.store(limit.is_some(), Ordering::Relaxed);
        let media = self.client.media();
        media
            .set_media_retention_policy(policy(limit))
            .await
            .map_err(|_| MediaCacheError::Unavailable)?;
        media
            .clean()
            .await
            .map_err(|_| MediaCacheError::Unavailable)
    }

    /// Whether media downloads may be kept in the encrypted store.
    pub(crate) fn caches_media(&self) -> bool {
        self.media_cache.load(Ordering::Relaxed)
    }

    /// Space the media store takes on disk, in bytes, when the store knows.
    pub async fn media_cache_size(&self) -> Result<Option<u64>, MediaCacheError> {
        let store = self
            .client
            .media_store()
            .lock()
            .await
            .map_err(|_| MediaCacheError::Unavailable)?;
        store
            .get_size()
            .await
            .map(|size| size.map(|size| size as u64))
            .map_err(|_| MediaCacheError::Unavailable)
    }

    /// Removes every cached image and thumbnail and compacts the store. The
    /// limit stays as it was.
    pub async fn clear_media_cache(&self) -> Result<(), MediaCacheError> {
        let media = self.client.media();
        let current = media
            .media_retention_policy()
            .await
            .map_err(|_| MediaCacheError::Unavailable)?;
        let cleared = async {
            media
                .set_media_retention_policy(policy(None))
                .await
                .map_err(|_| MediaCacheError::Unavailable)?;
            media
                .clean()
                .await
                .map_err(|_| MediaCacheError::Unavailable)
        }
        .await;
        let restored = media
            .set_media_retention_policy(current)
            .await
            .map_err(|_| MediaCacheError::Unavailable);
        cleared?;
        restored?;
        let store = self
            .client
            .media_store()
            .lock()
            .await
            .map_err(|_| MediaCacheError::Unavailable)?;
        store
            .optimize()
            .await
            .map_err(|_| MediaCacheError::Unavailable)
    }
}

/// Content already in the cache, without any network request.
pub(crate) async fn cached(
    client: &matrix_sdk::Client,
    enabled: bool,
    request: &MediaRequestParameters,
) -> Option<Vec<u8>> {
    if !enabled {
        return None;
    }
    let store = client.media_store().lock().await.ok()?;
    store.get_media_content(request).await.ok().flatten()
}

/// Cached content, or a download that is kept in the cache when it is on.
/// With `network` false only the cache is consulted.
pub(crate) async fn fetch(
    client: &matrix_sdk::Client,
    enabled: bool,
    network: bool,
    request: &MediaRequestParameters,
) -> Option<matrix_sdk::Result<Vec<u8>>> {
    if !network {
        return cached(client, enabled, request).await.map(Ok);
    }
    Some(client.media().get_media_content(request, enabled).await)
}

#[cfg(test)]
mod tests {
    use super::policy;

    #[test]
    fn off_keeps_nothing_and_a_limit_bounds_the_cache() {
        let off = policy(None);
        assert!(off.exceeds_max_cache_size(1));
        let limited = policy(Some(500 * 1024 * 1024));
        assert!(!limited.exceeds_max_cache_size(100 * 1024 * 1024));
        assert!(limited.exceeds_max_cache_size(600 * 1024 * 1024));
        // A single file cannot take the whole cache.
        assert!(
            limited
                .computed_max_file_size()
                .is_some_and(|max| max < 500 * 1024 * 1024)
        );
        assert!(limited.last_access_expiry.is_some());
    }
}
