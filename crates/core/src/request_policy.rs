//! Interactive SDK requests must return control to the application's retry
//! loop. SDK-owned sync/member/media operations can supply their own configs.
use matrix_sdk::config::RequestConfig;
use std::time::Duration;
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) fn interactive() -> RequestConfig {
    // SDK 0.19.1 honours Retry-After even when it exceeds the attempt timeout.
    // `max_retry_time` sets max *backoff*, not a wall-clock deadline in 0.19.1.
    RequestConfig::new()
        .disable_retry()
        .timeout(REQUEST_TIMEOUT)
}
#[cfg(test)]
mod tests {
    #[test]
    fn request_deadline_leaves_time_for_open_cleanup() {
        assert!(
            super::REQUEST_TIMEOUT + std::time::Duration::from_secs(2)
                < crate::conversation::OPEN_TIMEOUT
        );
    }
}
