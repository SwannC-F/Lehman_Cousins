//! Global position tracker to maintain true exposure.

use crate::core::models::{ExecutionReport, Side, SymbolId};
use dashmap::DashMap;
use rust_decimal::Decimal;

/// Tracks net exposure (inventory) per symbol based on execution reports.
pub struct PositionTracker {
    // DashMap enables lock-free concurrent reads and sharded lock writes.
    // Using SymbolId (u32) instead of String eliminates allocation overhead.
    positions: DashMap<SymbolId, Decimal>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: DashMap::new(),
        }
    }

    /// Update position based on a new execution report.
    /// Long exposure (Buy) is positive, Short exposure (Sell) is negative.
    pub fn apply_execution(&self, report: &ExecutionReport) {
        if report.executed_quantity.is_zero() {
            return;
        }

        let mut pos = self.positions.entry(report.symbol_id).or_insert(Decimal::ZERO);
        match report.side {
            Side::Buy => *pos += report.executed_quantity,
            Side::Sell => *pos -= report.executed_quantity,
        }
    }

    /// Forcefully set a position (used exclusively during boot reconciliation).
    pub fn set_position(&self, symbol_id: SymbolId, qty: Decimal) {
        self.positions.insert(symbol_id, qty);
    }

    /// Retrieve the net position for a symbol.
    pub fn get_position(&self, symbol_id: SymbolId) -> Decimal {
        self.positions
            .get(&symbol_id)
            .map(|r| *r)
            .unwrap_or(Decimal::ZERO)
    }
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use crate::core::models::OrderStatus;
    use uuid::Uuid;

    #[test]
    fn inventory_updates() {
        let tracker = PositionTracker::new();
        let sym = 42; // SymbolId

        let rep1 = ExecutionReport {
            client_id: Uuid::new_v4(),
            symbol_id: sym,
            symbol: "BTC-USDT".into(),
            order_status: OrderStatus::Filled,
            executed_quantity: dec!(1.5),
            price: dec!(100),
            side: Side::Buy,
        };

        tracker.apply_execution(&rep1);
        assert_eq!(tracker.get_position(sym), dec!(1.5));

        let rep2 = ExecutionReport {
            client_id: Uuid::new_v4(),
            symbol_id: sym,
            symbol: "BTC-USDT".into(),
            order_status: OrderStatus::Filled,
            executed_quantity: dec!(0.5),
            price: dec!(100),
            side: Side::Sell,
        };

        tracker.apply_execution(&rep2);
        assert_eq!(tracker.get_position(sym), dec!(1.0));
    }
}
