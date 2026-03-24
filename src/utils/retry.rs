//! Async retry with exponential back-off.
//!
//! Generic helper used throughout the codebase to add resilience to
//! flaky network calls (WebSocket reconnects, REST retries, DB queries).

use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

/// Execute `op` up to `max_attempts` times, doubling the delay each time.
///
/// Returns the result of the first successful attempt, or the last error.
///
/// # Example
/// ```rust,no_run
/// # use lehman_cousins_core::utils::retry::retry_exponential;
/// # async fn example() -> anyhow::Result<()> {
/// let result = retry_exponential(3, 500, || async {
///     // some fallible async operation
///     Ok::<_, anyhow::Error>(42)
/// }).await?;
/// # Ok(())
/// # }
/// ```
pub async fn retry_exponential<F, Fut, T>(
    max_attempts: u32,
    base_delay_ms: u64,
    mut op: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_err = None;

    for attempt in 0..max_attempts {
        match op().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                let delay = base_delay_ms * 2u64.saturating_pow(attempt);
                warn!(
                    attempt = attempt + 1,
                    max_attempts,
                    delay_ms = delay,
                    error = %e,
                    "Retrying after error"
                );
                last_err = Some(e);
                sleep(Duration::from_millis(delay)).await;
            }
        }
    }

    Err(last_err.expect("max_attempts must be > 0"))
}
