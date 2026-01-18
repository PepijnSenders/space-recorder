//! Retry and backoff utilities for fal.ai API operations.
//!
//! This module provides functions for handling transient errors, rate limiting,
//! and exponential backoff with jitter.

use std::time::Duration;

/// Default number of retry attempts for rate-limited requests.
pub const DEFAULT_MAX_RETRIES: u32 = 5;

/// Default number of retry attempts for transient network errors.
pub const DEFAULT_NETWORK_RETRIES: u32 = 3;

/// Base delay for exponential backoff (1 second).
pub const DEFAULT_BACKOFF_BASE: Duration = Duration::from_secs(1);

/// Maximum delay cap for exponential backoff (60 seconds).
pub const DEFAULT_BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Determine if a reqwest error is a transient network error that should be retried.
///
/// Returns true for connection errors, timeouts, and other temporary failures.
/// Returns false for errors that are unlikely to resolve on retry.
pub fn is_transient_network_error(error: &reqwest::Error) -> bool {
    // Connection errors (e.g., connection refused, DNS failures)
    if error.is_connect() {
        return true;
    }

    // Request timeout
    if error.is_timeout() {
        return true;
    }

    // Request failed during body transfer
    if error.is_body() {
        return true;
    }

    // Check for specific status codes that indicate transient issues
    if let Some(status) = error.status() {
        // 502 Bad Gateway, 503 Service Unavailable, 504 Gateway Timeout
        // These are typically temporary server-side issues
        if status.as_u16() == 502 || status.as_u16() == 503 || status.as_u16() == 504 {
            return true;
        }
    }

    false
}

/// Parse the Retry-After header value to get retry delay in seconds.
///
/// Handles both integer seconds format (e.g., "30") and HTTP-date format.
/// Returns None if the header is missing or cannot be parsed.
pub fn parse_retry_after(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Calculate exponential backoff delay with jitter.
///
/// Uses the formula: min(base * 2^attempt + jitter, max_delay)
/// where jitter is a random value between 0 and base.
pub fn calculate_backoff(attempt: u32, base: Duration, max: Duration) -> Duration {
    let exponential = base.saturating_mul(2u32.saturating_pow(attempt));
    // Add some jitter (up to base duration) to prevent thundering herd
    let jitter_ms = (base.as_millis() as u64).min(1000);
    let jitter = Duration::from_millis(jitter_ms / 2);
    exponential.saturating_add(jitter).min(max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_backoff_first_attempt() {
        let delay = calculate_backoff(0, Duration::from_secs(1), Duration::from_secs(60));
        // First attempt: base * 2^0 + jitter = 1s + 0.5s = 1.5s
        assert!(delay >= Duration::from_secs(1));
        assert!(delay <= Duration::from_millis(1500));
    }

    #[test]
    fn test_calculate_backoff_second_attempt() {
        let delay = calculate_backoff(1, Duration::from_secs(1), Duration::from_secs(60));
        // Second attempt: base * 2^1 + jitter = 2s + 0.5s = 2.5s
        assert!(delay >= Duration::from_secs(2));
        assert!(delay <= Duration::from_millis(2500));
    }

    #[test]
    fn test_calculate_backoff_third_attempt() {
        let delay = calculate_backoff(2, Duration::from_secs(1), Duration::from_secs(60));
        // Third attempt: base * 2^2 + jitter = 4s + 0.5s = 4.5s
        assert!(delay >= Duration::from_secs(4));
        assert!(delay <= Duration::from_millis(4500));
    }

    #[test]
    fn test_calculate_backoff_respects_max() {
        let delay = calculate_backoff(10, Duration::from_secs(1), Duration::from_secs(60));
        // Should be capped at max (60s)
        assert!(delay <= Duration::from_secs(60));
    }

    #[test]
    fn test_calculate_backoff_with_small_base() {
        let delay = calculate_backoff(0, Duration::from_millis(100), Duration::from_secs(10));
        // First attempt with 100ms base: 100ms + 50ms jitter = 150ms max
        assert!(delay >= Duration::from_millis(100));
        assert!(delay <= Duration::from_millis(150));
    }

    #[test]
    fn test_default_retry_constants() {
        assert_eq!(DEFAULT_MAX_RETRIES, 5);
        assert_eq!(DEFAULT_BACKOFF_BASE, Duration::from_secs(1));
        assert_eq!(DEFAULT_BACKOFF_MAX, Duration::from_secs(60));
    }

    #[test]
    fn test_default_network_retries_is_3() {
        assert_eq!(DEFAULT_NETWORK_RETRIES, 3);
    }

    #[test]
    fn test_network_retry_uses_exponential_backoff() {
        // Default backoff: 1s base, 60s max
        let first_delay = calculate_backoff(0, DEFAULT_BACKOFF_BASE, DEFAULT_BACKOFF_MAX);
        let second_delay = calculate_backoff(1, DEFAULT_BACKOFF_BASE, DEFAULT_BACKOFF_MAX);
        let third_delay = calculate_backoff(2, DEFAULT_BACKOFF_BASE, DEFAULT_BACKOFF_MAX);

        // Each retry should have increasing delay
        assert!(first_delay >= Duration::from_secs(1));
        assert!(second_delay >= Duration::from_secs(2));
        assert!(third_delay >= Duration::from_secs(4));

        // All should be capped at max
        assert!(first_delay <= DEFAULT_BACKOFF_MAX);
        assert!(second_delay <= DEFAULT_BACKOFF_MAX);
        assert!(third_delay <= DEFAULT_BACKOFF_MAX);
    }
}
