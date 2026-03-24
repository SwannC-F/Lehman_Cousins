//! Token Bucket Rate Limiter.
//!
//! Enforces exchange API rate limits (e.g., 5 orders per second) using a
//! classic token bucket algorithm.
//!
//! A token represents permission to make 1 API request. The bucket refills
//! automatically over time up to a maximum burst capacity. If the bucket is
//! empty, requests are rejected with a clear `RateLimitExceeded` error,
//! preventing the exchange from issuing IP bans or penalty lockouts.

use parking_lot::Mutex;
use std::time::{Duration, Instant};

/// Rate-limiting error. Not a network failure; signals that the strategy
/// is firing too fast and should back off to avoid an exchange ban.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Rate limit exceeded — try again in {retry_after:?}")]
pub struct RateLimitError {
    pub retry_after: Duration,
}

/// Token bucket state. Protected by a `Mutex` because it is updated on every
/// outgoing order. The lock is held for < 1 microsecond (simple math only).
pub struct TokenBucket {
    capacity: f64,
    tokens: Mutex<f64>,
    fill_rate_per_sec: f64,
    last_update: Mutex<Instant>,
}

impl TokenBucket {
    /// Create a new bucket.
    ///
    /// - `capacity`: The maximum burst size (e.g., 10 orders).
    /// - `fill_rate_per_sec`: How many tokens are added per second (e.g., 5.0).
    pub fn new(capacity: f64, fill_rate_per_sec: f64) -> Self {
        Self {
            capacity,
            tokens: Mutex::new(capacity), // Bucket starts full
            fill_rate_per_sec,
            last_update: Mutex::new(Instant::now()),
        }
    }

    /// Attempt to consume 1 token.
    ///
    /// Returns `Ok(())` if a token was consumed.
    /// Returns `Err(RateLimitError)` if the bucket is empty, including the
    /// exact `Duration` to wait before a token will become available.
    pub fn consume(&self) -> Result<(), RateLimitError> {
        let mut tokens_guard = self.tokens.lock();
        let mut last_update_guard = self.last_update.lock();

        let now = Instant::now();
        let elapsed = now.duration_since(*last_update_guard);

        // Refill bucket based on elapsed time (up to capacity)
        let new_tokens = elapsed.as_secs_f64() * self.fill_rate_per_sec;
        *tokens_guard = (*tokens_guard + new_tokens).min(self.capacity);
        *last_update_guard = now;

        if *tokens_guard >= 1.0 {
            *tokens_guard -= 1.0;
            Ok(())
        } else {
            let deficit = 1.0 - *tokens_guard;
            let retry_secs = deficit / self.fill_rate_per_sec;
            Err(RateLimitError {
                retry_after: Duration::from_secs_f64(retry_secs),
            })
        }
    }

    /// Current number of available tokens (for metrics/logging).
    pub fn available_tokens(&self) -> f64 {
        // We do a mock refill calculation to return an accurate instant value
        // without actually mutating the state.
        let tokens = *self.tokens.lock();
        let last_update = *self.last_update.lock();

        let elapsed = Instant::now().duration_since(last_update);
        let new_tokens = elapsed.as_secs_f64() * self.fill_rate_per_sec;
        (tokens + new_tokens).min(self.capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn burst_consumption() {
        let bucket = TokenBucket::new(3.0, 1.0);
        assert!(bucket.consume().is_ok());
        assert!(bucket.consume().is_ok());
        assert!(bucket.consume().is_ok());
        // 4th consume fails immediately (3 capacity)
        assert!(bucket.consume().is_err());
    }

    #[test]
    fn refill_over_time() {
        // Capacity 1, refill 10 per sec (i.e. 1 token every 100ms)
        let bucket = TokenBucket::new(1.0, 10.0);
        assert!(bucket.consume().is_ok());
        assert!(bucket.consume().is_err()); // empty

        sleep(Duration::from_millis(150)); // wait > 100ms

        assert!(bucket.consume().is_ok()); // refilled
        assert!(bucket.consume().is_err()); // empty again
    }

    #[test]
    fn retry_after_duration_is_accurate() {
        // Refill 1 token per second
        let bucket = TokenBucket::new(1.0, 1.0);
        assert!(bucket.consume().is_ok());

        let err = bucket.consume().unwrap_err();
        // Should take ~1 second to refill 1 token
        assert!(err.retry_after.as_secs_f64() > 0.9);
        assert!(err.retry_after.as_secs_f64() <= 1.0);
    }
}
