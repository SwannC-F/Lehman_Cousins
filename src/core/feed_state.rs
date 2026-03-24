//! Order book feed state machine.
//!
//! Defines the strict lifecycle every exchange feed connection must follow
//! before its data is trusted by the trading engine.
//!
//! ```text
//! Pending ──on_ws_connected()──► Buffering ──on_snapshot_received()──► Syncing ──(drain)──► Live
//!   ▲                                │                                                         │
//!   └──────────── on_ws_disconnected() / buffer overflow / sequence gap ────────────────────────┘
//! ```
//!
//! The current state is broadcast over a [`tokio::sync::watch`] channel so
//! consumers (engine, risk manager) can observe feed health without polling.

use std::fmt;

/// The five observable states of an exchange feed connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedState {
    /// Initial state and post-disconnect recovery state.
    /// All order submissions are blocked while in this state.
    Pending,

    /// WebSocket is connected. Incoming deltas are buffered but NOT applied.
    /// A REST snapshot request is in-flight.
    Buffering,

    /// REST snapshot has been received. Draining the delta buffer:
    /// deltas with `sequence <= snapshot_seq` are discarded.
    /// Remaining deltas are applied to the snapshot in order.
    Syncing { snapshot_seq: u64 },

    /// Book is fully synchronised. Strategies may trade.
    Live,
}

impl FeedState {
    /// Returns `true` only when the book can be trusted for strategy use.
    #[inline]
    pub fn is_live(&self) -> bool {
        matches!(self, FeedState::Live)
    }

    /// Returns `true` if the feed is actively collecting deltas (not halted).
    #[inline]
    pub fn is_connected(&self) -> bool {
        matches!(self, FeedState::Buffering | FeedState::Syncing { .. } | FeedState::Live)
    }
}

impl fmt::Display for FeedState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeedState::Pending              => write!(f, "Pending"),
            FeedState::Buffering            => write!(f, "Buffering"),
            FeedState::Syncing { snapshot_seq } =>
                write!(f, "Syncing(snapshot_seq={snapshot_seq})"),
            FeedState::Live                 => write!(f, "Live"),
        }
    }
}

/// Errors that cause the state machine to abort and reset to [`FeedState::Pending`].
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The REST snapshot is older than the oldest buffered delta.
    /// The gap between snapshot and delta stream is irrecoverable.
    #[error("Sequence gap: snapshot_seq={snapshot_seq} < oldest_delta_seq={oldest_delta_seq}")]
    SequenceGap {
        snapshot_seq:     u64,
        oldest_delta_seq: u64,
    },

    /// The delta buffer exceeded [`MAX_BUFFER_CAPACITY`].
    /// The snapshot is irrelevant; the book is too stale to recover.
    #[error("Delta buffer overflow (capacity={capacity})")]
    BufferOverflow { capacity: usize },

    /// A REST snapshot arrived but the generation no longer matches.
    /// The WS connection was reset while the REST call was in-flight.
    #[error("Stale REST snapshot (generation mismatch: expected={expected}, got={got})")]
    StaleSnapshot { expected: u64, got: u64 },

    /// Invalid state transition attempted.
    #[error("Invalid transition: cannot go from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}
