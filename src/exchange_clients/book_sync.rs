//! Order book snapshot+delta synchronization engine.
//!
//! [`BookSynchronizer`] owns the full sync lifecycle for a single trading pair.
//! It enforces the strict state machine defined in [`crate::core::feed_state`]
//! and guards against the three critical failure modes:
//!
//! | Failure mode | Guard | Recovery |
//! |---|---|---|
//! | Sequence gap | `snapshot_seq < oldest_delta_seq` | abort → `Pending` |
//! | Buffer overflow | `buffer.len() >= MAX_BUFFER_CAPACITY` | abort → `Pending` |
//! | Stale snapshot | `generation` counter mismatch | discard silently |
//!
//! ## State machine transitions
//!
//! ```text
//! Pending
//!   │ on_ws_connected()
//!   ▼
//! Buffering  ──────── push_delta() ──► buffer grows (NOT applied to book)
//!   │                                  if len >= MAX_BUFFER_CAPACITY → abort → Pending
//!   │ on_snapshot_received(snapshot, generation)
//!   │   if generation mismatch         → discard silently
//!   │   if snapshot_seq < oldest_delta → SyncError::SequenceGap → Pending
//!   ▼
//! Syncing(snapshot_seq)
//!   │ drain: drop deltas with seq <= snapshot_seq
//!   │        apply deltas  with seq >  snapshot_seq
//!   ▼
//! Live  ──── push_delta() ──► applied directly to book
//!   │
//!   └──── on_ws_disconnected() ──► Pending (book cleared, generation++)
//! ```

use std::collections::VecDeque;

use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::core::{
    feed_state::{FeedState, SyncError},
    models::OrderBookUpdate,
    orderbook::OrderBook,
};

/// Hard cap on the delta buffer size.
///
/// At 1 000 deltas/s this gives an 2-second window for the REST snapshot.
/// If this is exceeded the book is too stale to recover; we abort and re-sync.
pub const MAX_BUFFER_CAPACITY: usize = 2_048;

/// Owns the order book and the sync state machine for one trading symbol.
///
/// Call sites must use the public methods (`on_ws_connected`, `push_delta`,
/// `on_snapshot_received`, `on_ws_disconnected`) to drive state transitions.
/// Direct field mutation is intentionally prevented.
pub struct BookSynchronizer {
    symbol: String,
    /// The managed order book — only written to by this struct.
    book: OrderBook,
    /// Current feed state (also broadcast via `state_tx`).
    state: FeedState,
    /// Delta buffer — populated during `Buffering`; drained during `Syncing`.
    buffer: VecDeque<OrderBookUpdate>,
    /// Monotonic counter incremented on every disconnect / reset.
    /// A spawned REST task captures the value at launch and checks it on
    /// completion; a mismatch means the WS died while the request was running.
    generation: u64,
    /// Watch channel — allows the engine/risk-manager to observe feed health
    /// without acquiring a lock on the synchronizer itself.
    state_tx: watch::Sender<FeedState>,
}

impl BookSynchronizer {
    /// Create a new synchronizer in [`FeedState::Pending`].
    pub fn new(symbol: impl Into<String>) -> (Self, watch::Receiver<FeedState>) {
        let symbol = symbol.into();
        let (state_tx, state_rx) = watch::channel(FeedState::Pending);
        let sync = Self {
            book: OrderBook::new(&symbol),
            symbol,
            state: FeedState::Pending,
            buffer: VecDeque::with_capacity(64),
            generation: 0,
            state_tx,
        };
        (sync, state_rx)
    }

    // ── Public queries ────────────────────────────────────────────────────────

    /// Returns the current generation counter.
    ///
    /// Capture this value *before* launching a REST snapshot task and pass it
    /// back to [`Self::on_snapshot_received`] to detect stale responses.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns `true` when the book is live and fit for strategy consumption.
    pub fn is_live(&self) -> bool {
        self.state.is_live()
    }

    /// Read-only access to the managed order book.
    pub fn book(&self) -> &OrderBook {
        &self.book
    }

    // ── State machine inputs ──────────────────────────────────────────────────

    /// Called when the WebSocket connection is established.
    ///
    /// Valid from: `Pending` only.
    /// Transitions to: `Buffering`.
    pub fn on_ws_connected(&mut self) -> Result<(), SyncError> {
        match &self.state {
            FeedState::Pending => {
                info!(symbol = %self.symbol, "WS connected → Buffering");
                self.set_state(FeedState::Buffering);
                Ok(())
            }
            other => Err(SyncError::InvalidTransition {
                from: other.to_string(),
                to: "Buffering".into(),
            }),
        }
    }

    /// Push an incoming WebSocket delta.
    ///
    /// - `Buffering/Syncing`: appended to the internal buffer (or applied after drain)
    /// - `Live`: applied directly to the book
    /// - `Pending`: silently discarded
    ///
    /// Returns `Err(SyncError::BufferOverflow)` and resets to `Pending` if the
    /// buffer exceeds [`MAX_BUFFER_CAPACITY`].
    pub fn push_delta(&mut self, update: OrderBookUpdate) -> Result<(), SyncError> {
        match &self.state {
            FeedState::Pending => {
                debug!(symbol = %self.symbol, seq = update.sequence, "Delta discarded (Pending)");
                Ok(())
            }

            FeedState::Buffering | FeedState::Syncing { .. } => {
                if self.buffer.len() >= MAX_BUFFER_CAPACITY {
                    warn!(
                        symbol   = %self.symbol,
                        capacity = MAX_BUFFER_CAPACITY,
                        "Delta buffer overflow — aborting sync"
                    );
                    self.reset("buffer overflow");
                    return Err(SyncError::BufferOverflow { capacity: MAX_BUFFER_CAPACITY });
                }
                self.buffer.push_back(update);
                Ok(())
            }

            FeedState::Live => {
                self.book.apply(&update);
                Ok(())
            }
        }
    }

    /// Called when the REST snapshot response arrives.
    ///
    /// **Faille 3 guard**: `generation` must match the value captured when the
    /// REST task was spawned. A mismatch means the WS disconnected mid-flight.
    ///
    /// **Faille 1 guard**: if `snapshot.sequence < oldest_buffered_delta_seq`,
    /// the gap is irrecoverable. Resets to `Pending`.
    pub fn on_snapshot_received(
        &mut self,
        snapshot: OrderBookUpdate,
        generation: u64,
    ) -> Result<(), SyncError> {
        // ── Guard: stale snapshot (race condition) ────────────────────────────
        if generation != self.generation {
            warn!(
                symbol   = %self.symbol,
                expected = self.generation,
                got      = generation,
                "Stale REST snapshot discarded (generation mismatch)"
            );
            return Err(SyncError::StaleSnapshot {
                expected: self.generation,
                got: generation,
            });
        }

        // Only valid from Buffering
        if !matches!(self.state, FeedState::Buffering) {
            return Err(SyncError::InvalidTransition {
                from: self.state.to_string(),
                to: "Syncing".into(),
            });
        }

        let snapshot_seq = snapshot.sequence;

        // ── Guard: sequence gap ───────────────────────────────────────────────
        if let Some(oldest) = self.buffer.front() {
            if snapshot_seq < oldest.sequence {
                warn!(
                    symbol           = %self.symbol,
                    snapshot_seq,
                    oldest_delta_seq = oldest.sequence,
                    "Sequence gap detected — snapshot is too old"
                );
                let oldest_delta_seq = oldest.sequence;
                self.reset("sequence gap");
                return Err(SyncError::SequenceGap { snapshot_seq, oldest_delta_seq });
            }
        }

        // Apply snapshot to the book as the authoritative baseline
        self.book.apply(&snapshot);
        self.set_state(FeedState::Syncing { snapshot_seq });

        // ── Drain the delta buffer ────────────────────────────────────────────
        // Drop deltas already covered by the snapshot; apply the rest in order.
        let mut applied = 0usize;
        let mut dropped = 0usize;

        while let Some(delta) = self.buffer.pop_front() {
            if delta.sequence <= snapshot_seq {
                dropped += 1;
            } else {
                self.book.apply(&delta);
                applied += 1;
            }
        }

        info!(
            symbol   = %self.symbol,
            snapshot_seq,
            dropped,
            applied,
            "Buffer drained → Live"
        );
        self.set_state(FeedState::Live);
        Ok(())
    }

    /// Called when the WebSocket connection is lost (any state).
    ///
    /// Increments the generation counter (invalidates any in-flight REST task),
    /// clears the buffer, and resets the book to empty.
    pub fn on_ws_disconnected(&mut self) {
        warn!(symbol = %self.symbol, generation = self.generation + 1, "WS disconnected → Pending");
        self.reset("ws disconnected");
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn set_state(&mut self, new: FeedState) {
        debug!(symbol = %self.symbol, from = %self.state, to = %new, "State transition");
        self.state = new.clone();
        // Broadcast — receivers (engine, risk_manager) can react without polling.
        // Ignore send errors: they only occur when all receivers are dropped.
        let _ = self.state_tx.send(new);
    }

    /// Hard reset: back to `Pending`, buffer cleared, book reset, generation++.
    fn reset(&mut self, reason: &str) {
        debug!(symbol = %self.symbol, reason, "Resetting sync state");
        self.buffer.clear();
        self.book = OrderBook::new(&self.symbol);
        self.generation = self.generation.wrapping_add(1);
        self.set_state(FeedState::Pending);
    }
}

// =============================================================================
// Tests — 5 happy path + 3 failure modes
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;
    use crate::core::models::PriceLevel;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn update(seq: u64) -> OrderBookUpdate {
        OrderBookUpdate {
            symbol:    "BTC-USDT".into(),
            bids:      vec![PriceLevel { price: dec!(29_000), quantity: dec!(1) }],
            asks:      vec![PriceLevel { price: dec!(29_010), quantity: dec!(1) }],
            sequence:  seq,
            timestamp: Utc::now(),
        }
    }

    fn synced_book() -> BookSynchronizer {
        // Helper: returns a synchronizer in Live state with snapshot at seq=100
        // and two applied deltas (seq=101, seq=102).
        let (mut sync, _rx) = BookSynchronizer::new("BTC-USDT");
        sync.on_ws_connected().unwrap();
        // Buffer deltas 95-102
        for seq in 95..=102 {
            sync.push_delta(update(seq)).unwrap();
        }
        let gen = sync.generation();
        sync.on_snapshot_received(update(100), gen).unwrap();
        sync
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    /// 1. on_ws_connected() transitions Pending → Buffering
    #[test]
    fn pending_blocks_on_ws_open() {
        let (mut sync, rx) = BookSynchronizer::new("BTC-USDT");
        assert_eq!(sync.state, FeedState::Pending);
        assert!(!rx.borrow().is_live());

        sync.on_ws_connected().unwrap();
        assert_eq!(sync.state, FeedState::Buffering);
        assert!(!rx.borrow().is_live());
    }

    /// 2. Deltas pushed during Buffering are buffered, not applied to the book.
    #[test]
    fn buffering_does_not_apply_deltas() {
        let (mut sync, _rx) = BookSynchronizer::new("BTC-USDT");
        sync.on_ws_connected().unwrap();

        sync.push_delta(update(1)).unwrap();
        sync.push_delta(update(2)).unwrap();

        // Book must remain at sequence 0 (nothing applied yet)
        assert_eq!(sync.book().sequence(), 0);
        assert_eq!(sync.buffer.len(), 2);
    }

    /// 3. Snapshot drains stale deltas and applies only those ahead of it.
    #[test]
    fn syncing_drains_stale_deltas() {
        let (mut sync, _rx) = BookSynchronizer::new("BTC-USDT");
        sync.on_ws_connected().unwrap();

        // Buffer deltas 95-105
        for seq in 95..=105 {
            sync.push_delta(update(seq)).unwrap();
        }

        let gen = sync.generation();
        sync.on_snapshot_received(update(100), gen).unwrap();

        // Book must be at sequence 105 (snapshot 100 + deltas 101..=105)
        assert_eq!(sync.book().sequence(), 105);
        assert_eq!(sync.buffer.len(), 0); // buffer fully drained
    }

    /// 4. State becomes Live after a clean drain.
    #[test]
    fn live_after_clean_buffer() {
        let sync = synced_book();
        assert!(sync.is_live());
        assert_eq!(sync.state, FeedState::Live);
        // Watch channel must also reflect Live
    }

    /// 5. on_ws_disconnected() from any state resets to Pending and clears the book.
    #[test]
    fn disconnect_resets_to_pending() {
        let (mut sync, rx) = BookSynchronizer::new("BTC-USDT");
        sync.on_ws_connected().unwrap();

        let gen_before = sync.generation();
        sync.on_ws_disconnected();

        assert_eq!(sync.state, FeedState::Pending);
        assert!(!rx.borrow().is_live());
        assert_eq!(sync.buffer.len(), 0);
        assert_eq!(sync.book().sequence(), 0);
        // Generation must have incremented
        assert_eq!(sync.generation(), gen_before + 1);
    }

    // ── Failure modes ─────────────────────────────────────────────────────────

    /// 6. Sequence gap: snapshot_seq < oldest buffered delta → abort to Pending.
    #[test]
    fn sequence_gap_detected_aborts_to_pending() {
        let (mut sync, _rx) = BookSynchronizer::new("BTC-USDT");
        sync.on_ws_connected().unwrap();

        // Only deltas from seq=150 onwards have arrived
        for seq in 150..=155 {
            sync.push_delta(update(seq)).unwrap();
        }

        // Snapshot at seq=100 is OLDER than the oldest buffered delta (150)
        let gen = sync.generation();
        let result = sync.on_snapshot_received(update(100), gen);

        assert!(matches!(result, Err(SyncError::SequenceGap {
            snapshot_seq: 100,
            oldest_delta_seq: 150,
        })));
        assert_eq!(sync.state, FeedState::Pending);
        assert_eq!(sync.buffer.len(), 0); // buffer cleared
    }

    /// 7. Buffer overflow: push more than MAX_BUFFER_CAPACITY → abort to Pending.
    #[test]
    fn buffer_overflow_aborts_to_pending() {
        let (mut sync, _rx) = BookSynchronizer::new("BTC-USDT");
        sync.on_ws_connected().unwrap();

        // Fill the buffer to capacity
        for seq in 0..MAX_BUFFER_CAPACITY as u64 {
            sync.push_delta(update(seq)).unwrap();
        }
        assert_eq!(sync.buffer.len(), MAX_BUFFER_CAPACITY);

        // The (MAX_BUFFER_CAPACITY + 1)-th delta must trip the circuit breaker
        let result = sync.push_delta(update(MAX_BUFFER_CAPACITY as u64));

        assert!(matches!(result, Err(SyncError::BufferOverflow { .. })));
        assert_eq!(sync.state, FeedState::Pending);
        assert_eq!(sync.buffer.len(), 0); // buffer cleared by reset()
    }

    /// 8. Stale snapshot: WS disconnects while REST is in-flight → discard.
    #[test]
    fn stale_snapshot_discarded_after_disconnect() {
        let (mut sync, _rx) = BookSynchronizer::new("BTC-USDT");
        sync.on_ws_connected().unwrap();
        sync.push_delta(update(1)).unwrap();

        // Capture generation BEFORE disconnect (simulates the REST task spawn)
        let stale_gen = sync.generation();

        // WS disconnects while REST is in-flight
        sync.on_ws_disconnected();
        assert_eq!(sync.generation(), stale_gen + 1);

        // REST response arrives — must be silently discarded
        let result = sync.on_snapshot_received(update(1), stale_gen);

        assert!(matches!(result, Err(SyncError::StaleSnapshot { .. })));
        // State must remain Pending (not transition to Syncing/Live)
        assert_eq!(sync.state, FeedState::Pending);
    }
}
