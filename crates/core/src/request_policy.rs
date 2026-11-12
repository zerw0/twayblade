//! Requests that use the client's default config fail on the first error
//! instead of retrying inside the SDK, so callers see it and decide. SDK 0.19.1
//! otherwise retries transient responses (5xx, 429) for up to 15 minutes,
//! which kept room opening pending with no status. Room opening retries with
//! its own backoff and status. Sync, members, media and the send queue set
//! their own configs and keep their SDK retries and timeouts.
//!
//! The per-attempt timeout stays at the SDK default (30 s): joins over
//! federation, history pagination and search routinely take longer than 10 s.
//! Room opening is bounded separately by `conversation::OPEN_TIMEOUT`.
use matrix_sdk::config::RequestConfig;
pub(crate) fn interactive() -> RequestConfig {
    RequestConfig::new().disable_retry()
}
