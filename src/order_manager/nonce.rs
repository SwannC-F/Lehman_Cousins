//! Monotonically-increasing nonce generator.
//!
//! ## The problem
//!
//! Authenticated exchange REST APIs require a nonce — a value that must be
//! strictly greater than the previous call's nonce. Most exchanges use the
//! current timestamp in milliseconds or microseconds.
//!
//! In an async Tokio engine, two tasks scheduled in the same millisecond will
//! read the same `SystemTime::now()`, producing identical nonces. The exchange
//! will reject the second request as a **replay attack** (HTTP 400 / -1021).
//!
//! ## This solution
//!
//! [`NonceGenerator`] initialises an `AtomicU64` to the current epoch in
//! **microseconds**, then increments it with `SeqCst` ordering on every call.
//!
//! ```text
//! Task A:  fetch_add(1) → 1_711_276_925_000_001
//! Task B:  fetch_add(1) → 1_711_276_925_000_002   ← always strictly greater
//! ```
//!
//! The result is always >= the wall clock (no backwards drift) because the
//! counter only moves forward. If the wall clock advances past the counter
//! (e.g. after a long pause), the next call re-anchors to the new wall time.
//!
//! **Thread/task safety**: `AtomicU64` with `SeqCst` ordering is lock-free
//! and safe for arbitrary concurrent callers.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lock-free, monotonically-increasing nonce generator.
///
/// Create one instance per exchange connection and wrap it in an `Arc`
/// so all tasks share the same sequence.
pub struct NonceGenerator {
    counter: AtomicU64,
}

impl NonceGenerator {
    /// Create a generator seeded at the current epoch in **microseconds**.
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(Self::epoch_micros()),
        }
    }

    /// Return the next nonce.
    ///
    /// Guarantees:
    /// - Strictly greater than every previously returned value.
    /// - Never repeats, even under concurrent callers.
    /// - Always >= the current wall-clock epoch in microseconds.
    #[inline]
    pub fn next(&self) -> u64 {
        // Attempt to re-anchor to wall clock if it has overtaken the counter.
        // This prevents the counter from drifting far ahead of real time after
        // a burst, while keeping strict monotonicity.
        let wall = Self::epoch_micros();
        let prev = self.counter.fetch_max(wall, Ordering::SeqCst);
        // fetch_max returns the *previous* value; we need to ensure we
        // return a value strictly greater than whatever fetch_max stored.
        self.counter.fetch_add(1, Ordering::SeqCst).max(prev).wrapping_add(1)
    }

    fn epoch_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }
}

impl Default for NonceGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn nonces_are_strictly_increasing() {
        let gen = NonceGenerator::new();
        let mut prev = gen.next();
        for _ in 0..10_000 {
            let n = gen.next();
            assert!(n > prev, "nonce {n} <= previous {prev}");
            prev = n;
        }
    }

    #[test]
    fn concurrent_nonces_are_unique() {
        use std::sync::Arc;
        use std::thread;

        let gen = Arc::new(NonceGenerator::new());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let g = Arc::clone(&gen);
                thread::spawn(move || (0..1_000).map(|_| g.next()).collect::<Vec<_>>())
            })
            .collect();

        let all: Vec<u64> = threads.into_iter().flat_map(|t| t.join().unwrap()).collect();
        let unique: HashSet<u64> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "Duplicate nonces detected!");
    }

    #[test]
    fn nonce_is_anchored_near_wall_clock() {
        let gen = NonceGenerator::new();
        let n = gen.next();
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        // Nonce should be within a 1-second window of now
        assert!(n <= wall + 1_000_000, "Nonce far ahead of wall clock");
        assert!(n >= wall - 1_000_000, "Nonce far behind wall clock");
    }
}
