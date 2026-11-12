//! Observable SDK preparation; no protocol state is implemented here.
use super::*;
use std::{future::Future, time::Instant};
use tracing::{Instrument, Span};

struct Step {
    span: Span,
    started: Instant,
    complete: bool,
}
impl Drop for Step {
    fn drop(&mut self) {
        self.span
            .record("elapsed_ms", self.started.elapsed().as_millis() as u64);
        self.span.record("cancelled", !self.complete);
    }
}
/// Closing spans distinguish completion from cancellation; SDK child spans
/// (`read`, `write`, `spin_lock`, sqlite operations) identify store waits.
pub(super) async fn step<T>(span: Span, future: impl Future<Output = T>) -> T {
    let mut guard = Step {
        span: span.clone(),
        started: Instant::now(),
        complete: false,
    };
    let value = future.instrument(span).await;
    guard.complete = true;
    value
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum OpenError {
    Failed,
    Retry(Option<std::time::Duration>),
}
impl OpenError {
    fn status(status: u16, retry_after: Option<std::time::Duration>) -> Self {
        if status == 429 || (500..600).contains(&status) {
            Self::Retry(retry_after)
        } else {
            Self::Failed
        }
    }
    fn sdk(error: matrix_sdk::Error) -> Self {
        use matrix_sdk::ruma::api::error::{ErrorKind, RetryAfter};
        if let Some(api) = error.as_client_api_error() {
            let hint = match api.error_kind() {
                Some(ErrorKind::LimitExceeded(data)) => data.retry_after.map(|hint| match hint {
                    RetryAfter::Delay(delay) => delay,
                    RetryAfter::DateTime(time) => time
                        .duration_since(std::time::SystemTime::now())
                        .unwrap_or_default(),
                }),
                _ => None,
            };
            return Self::status(api.status_code.as_u16(), hint);
        }
        match error {
            matrix_sdk::Error::Http(error) => match *error {
                matrix_sdk::HttpError::Reqwest(_) => Self::Retry(None),
                _ => Self::Failed,
            },
            matrix_sdk::Error::CrossProcessLockError(_)
            | matrix_sdk::Error::ConcurrentRequestFailed => Self::Retry(None),
            _ => Self::Failed,
        }
    }
    fn cache(error: matrix_sdk::event_cache::EventCacheError) -> Self {
        match error {
            matrix_sdk::event_cache::EventCacheError::LockingStorage(_) => Self::Retry(None),
            _ => Self::Failed,
        }
    }
    fn builder(error: matrix_sdk_ui::timeline::Error) -> Self {
        match error {
            matrix_sdk_ui::timeline::Error::Sdk(error) => Self::sdk(error),
            matrix_sdk_ui::timeline::Error::EventCacheError(error) => Self::cache(error),
            _ => Self::Failed,
        }
    }
}

pub(super) fn retry_delay(
    attempt: u32,
    server_hint: Option<std::time::Duration>,
) -> std::time::Duration {
    super::open_retry_delay(attempt).max(server_hint.unwrap_or_default())
}

/// Prepare the same public SDK dependencies the builder will use. In
/// particular, don't let Builder's `.ok().unwrap_or_default()` hide a failed
/// encryption-state request as an unencrypted timeline. Transient failures retry;
/// permanent errors retain Failed. Cancellation drops only SDK RAII guards.
pub(super) async fn timeline(
    room: &matrix_sdk::Room,
    focus: TimelineFocus,
) -> Result<Arc<Timeline>, OpenError> {
    // SDK futures are large. Box each stage at the call boundary so nesting
    // tracing/preparation does not overflow the normal test/worker stack.
    let client = room.client();
    let cache = client.event_cache();
    cache.subscribe().map_err(OpenError::cache)?;
    let (_cache, _cache_drop) = step(
        tracing::info_span!(
            "event_cache_for_room",
            elapsed_ms = tracing::field::Empty,
            cancelled = tracing::field::Empty
        ),
        Box::pin(cache.room(room.room_id())),
    )
    .await
    .map_err(OpenError::cache)?;
    step(
        tracing::info_span!(
            "encryption_state_fetch",
            elapsed_ms = tracing::field::Empty,
            cancelled = tracing::field::Empty
        ),
        Box::pin(room.latest_encryption_state()),
    )
    .await
    .map_err(OpenError::sdk)?;
    step(
        tracing::info_span!(
            "timeline_build",
            elapsed_ms = tracing::field::Empty,
            cancelled = tracing::field::Empty
        ),
        Box::pin(
            room.timeline_builder()
                .with_focus(focus)
                .track_read_marker_and_receipts(TimelineReadReceiptTracking::MessageLikeEvents)
                .build(),
        ),
    )
    .await
    .map(Arc::new)
    .map_err(OpenError::builder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn only_transient_http_statuses_retry_and_server_delays_are_not_shortened() {
        for status in [400, 401, 403, 404] {
            assert_eq!(OpenError::status(status, None), OpenError::Failed);
        }
        for status in [429, 500, 502, 503, 504] {
            assert_eq!(OpenError::status(status, None), OpenError::Retry(None));
        }
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(60))),
            Duration::from_secs(60)
        );
        assert_eq!(
            retry_delay(u32::MAX, Some(Duration::ZERO)),
            Duration::from_secs(30)
        );
        assert_eq!(
            retry_delay(2, Some(Duration::from_secs(1))),
            Duration::from_secs(4)
        );
    }
}
