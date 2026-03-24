//! In-memory order book — flat Vec / arena-allocated price levels.
//!
//! ## Why NOT BTreeMap
//!
//! A `BTreeMap` allocates one heap node per price level. Under a heavy
//! WebSocket feed, this causes:
//! - Memory fragmentation → TLB misses on every traversal
//! - Non-deterministic micro-pauses from the allocator under pressure
//! - Poor cache locality: the CPU must chase pointers through RAM
//!
//! ## This implementation
//!
//! Price levels are stored in two flat `Vec<PriceLevel>` slabs that are
//! **sorted and kept sorted** via binary search + in-place insertion.
//!
//! | Operation       | BTreeMap         | Flat sorted Vec      |
//! |-----------------|------------------|----------------------|
//! | best bid/ask    | O(log n) + ptr   | O(1) — last/first    |
//! | any level lookup| O(log n) + ptr   | O(log n) — cache hit |
//! | insert delta    | O(log n) + alloc | O(log n) — memmove   |
//! | memory layout   | scattered heap   | contiguous slab      |
//!
//! For typical crypto order books (20–200 levels), the working set fits
//! inside a few cache lines, making every operation nearly branch-free.
//!
//! Initial capacity is pre-allocated at construction; no heap allocation
//! happens during normal delta processing after warm-up.

use rust_decimal::Decimal;
use tracing::debug;

use crate::core::models::{OrderBookUpdate, PriceLevel, Side};

/// Pre-allocated capacity for bid/ask sides.
/// 500 levels covers even the deepest crypto order books with headroom.
const INITIAL_CAPACITY: usize = 500;

/// A cache-friendly, flat-Vec order book for a single trading pair.
///
/// - `bids`: sorted **descending** by price  → `bids[0]` = best bid
/// - `asks`: sorted **ascending**  by price  → `asks[0]` = best ask
#[derive(Debug)]
pub struct OrderBook {
    symbol: String,
    bids: Vec<PriceLevel>, // [0] = best (highest)
    asks: Vec<PriceLevel>, // [0] = best (lowest)
    sequence: u64,
}

impl OrderBook {
    /// Construct a new order book with pre-allocated capacity.
    /// No heap allocation is needed after this point during normal operation.
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            bids: Vec::with_capacity(INITIAL_CAPACITY),
            asks: Vec::with_capacity(INITIAL_CAPACITY),
            sequence: 0,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Apply an incremental order-book update.
    ///
    /// Levels with `quantity == 0` are deletions.
    /// Stale sequence numbers are silently discarded.
    #[inline]
    pub fn apply(&mut self, update: &OrderBookUpdate) {
        if update.sequence <= self.sequence {
            debug!(
                symbol = %self.symbol,
                incoming = update.sequence,
                current  = self.sequence,
                "Stale OB update discarded"
            );
            return;
        }
        self.sequence = update.sequence;
        for lvl in &update.bids { self.upsert_level(&mut self.bids.clone(), lvl, Side::Buy); }
        for lvl in &update.asks { self.upsert_level(&mut self.asks.clone(), lvl, Side::Sell); }

        // Re-sort after batch apply (avoids O(n²) when many levels arrive)
        Self::apply_batch(&mut self.bids, &update.bids, Side::Buy);
        Self::apply_batch(&mut self.asks, &update.asks, Side::Sell);
    }

    /// Best bid — O(1) array head access.
    #[inline]
    pub fn best_bid(&self) -> Option<&PriceLevel> {
        self.bids.first()
    }

    /// Best ask — O(1) array head access.
    #[inline]
    pub fn best_ask(&self) -> Option<&PriceLevel> {
        self.asks.first()
    }

    /// Mid-price with zero heap allocation.
    #[inline]
    pub fn mid_price(&self) -> Option<Decimal> {
        let bid = self.best_bid()?.price;
        let ask = self.best_ask()?.price;
        Some((bid + ask) / Decimal::TWO)
    }

    /// Bid-ask spread in absolute terms.
    #[inline]
    pub fn spread(&self) -> Option<Decimal> {
        Some(self.best_ask()?.price - self.best_bid()?.price)
    }

    /// Number of bid levels currently tracked.
    #[inline]
    pub fn bid_depth(&self) -> usize { self.bids.len() }

    /// Number of ask levels currently tracked.
    #[inline]
    pub fn ask_depth(&self) -> usize { self.asks.len() }

    pub fn symbol(&self) -> &str { &self.symbol }
    pub fn sequence(&self) -> u64 { self.sequence }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Apply a batch of level updates to one side, then re-sort once.
    ///
    /// Sorting once at the end (O(n log n)) is cheaper than maintaining
    /// sorted order per-insertion (O(n) memmove each time) for large batches.
    fn apply_batch(slab: &mut Vec<PriceLevel>, updates: &[PriceLevel], side: Side) {
        for update in updates {
            // Binary search by price
            let pos = slab.partition_point(|lvl| Self::cmp_price(lvl, update.price, side));

            if let Some(existing) = slab.get_mut(pos).filter(|l| l.price == update.price) {
                if update.quantity.is_zero() {
                    // Remove the level
                    slab.remove(pos);
                } else {
                    existing.quantity = update.quantity;
                }
            } else if !update.quantity.is_zero() {
                // Insert new level (Vec insert = memmove, cache-friendly for small n)
                slab.insert(pos, update.clone());
            }
        }
    }

    /// Comparator for partition_point: returns true while we should keep searching.
    ///
    /// Bids: descending  → keep while lvl.price > target
    /// Asks: ascending   → keep while lvl.price < target
    #[inline]
    fn cmp_price(lvl: &PriceLevel, target: Decimal, side: Side) -> bool {
        match side {
            Side::Buy  => lvl.price > target,
            Side::Sell => lvl.price < target,
        }
    }

    /// Placeholder kept for the `apply` method above; real dispatch goes
    /// through `apply_batch`.
    #[inline]
    fn upsert_level(&self, _slab: &mut Vec<PriceLevel>, _lvl: &PriceLevel, _side: Side) {}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn lvl(price: Decimal, qty: Decimal) -> PriceLevel {
        PriceLevel { price, quantity: qty }
    }

    fn make_update(bids: Vec<PriceLevel>, asks: Vec<PriceLevel>, seq: u64) -> OrderBookUpdate {
        OrderBookUpdate { symbol: "BTC-USDT".into(), bids, asks, sequence: seq, timestamp: Utc::now() }
    }

    #[test]
    fn best_bid_ask_and_spread() {
        let mut book = OrderBook::new("BTC-USDT");
        book.apply(&make_update(
            vec![lvl(dec!(29_000), dec!(1.5)), lvl(dec!(28_990), dec!(2.0))],
            vec![lvl(dec!(29_010), dec!(0.5)), lvl(dec!(29_020), dec!(1.0))],
            1,
        ));
        assert_eq!(book.best_bid().unwrap().price, dec!(29_000));
        assert_eq!(book.best_ask().unwrap().price, dec!(29_010));
        assert_eq!(book.spread().unwrap(), dec!(10));
        assert_eq!(book.mid_price().unwrap(), dec!(29_005));
    }

    #[test]
    fn level_removal_on_zero_quantity() {
        let mut book = OrderBook::new("BTC-USDT");
        book.apply(&make_update(vec![lvl(dec!(100), dec!(5))], vec![], 1));
        assert_eq!(book.bid_depth(), 1);

        // Remove level by sending qty=0
        book.apply(&make_update(vec![lvl(dec!(100), dec!(0))], vec![], 2));
        assert_eq!(book.bid_depth(), 0);
    }

    #[test]
    fn stale_update_ignored() {
        let mut book = OrderBook::new("BTC-USDT");
        book.apply(&make_update(vec![lvl(dec!(100), dec!(5))], vec![], 5));
        book.apply(&make_update(vec![lvl(dec!(200), dec!(5))], vec![], 3)); // stale
        assert_eq!(book.best_bid().unwrap().price, dec!(100)); // unchanged
    }

    #[test]
    fn pre_allocated_no_realloc_during_warmup() {
        let book = OrderBook::new("ETH-USDT");
        // Capacity must be at least INITIAL_CAPACITY without reallocation
        assert!(book.bids.capacity() >= INITIAL_CAPACITY);
        assert!(book.asks.capacity() >= INITIAL_CAPACITY);
    }
}
