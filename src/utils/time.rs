//! Time utilities.

use chrono::{DateTime, Utc};

/// Current UTC timestamp.
#[inline]
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Convert a Unix epoch in milliseconds to a [`DateTime<Utc>`].
#[inline]
pub fn from_epoch_ms(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default()
}

/// Current Unix epoch in milliseconds (for exchange API signatures).
#[inline]
pub fn epoch_ms() -> u64 {
    Utc::now().timestamp_millis() as u64
}
