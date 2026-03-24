//! Order Manager — execution gatekeeper.
//!
//! Every order submission from a strategy MUST go through this module.
//! It enforces two hard invariants:
//!
//! 1. **Rate limiting** — a token-bucket algorithm matches the exchange's
//!    published API rate limits. Orders that would exceed the limit are either
//!    queued (if within the burst window) or rejected with `RateLimitExceeded`.
//!
//! 2. **Monotonic nonce** — every authenticated REST call receives a strictly
//!    increasing `u64` nonce drawn from an `AtomicU64`. Even if Tokio schedules
//!    two tasks in the same microsecond, they can never share a nonce.
//!
//! The engine should hold a single `Arc<OrderManager>` and route all
//! order submissions through it.

pub mod rate_limiter;
pub mod nonce;
pub mod manager;

pub use manager::OrderManager;
pub use rate_limiter::{TokenBucket, RateLimitError};
pub use nonce::NonceGenerator;
