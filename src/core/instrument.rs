//! Instrument metadata for exact quantitative rounding.

use crate::core::models::{Order, SymbolId};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Fundamental market constraints for a traded asset.
#[derive(Debug, Clone)]
pub struct Instrument {
    pub symbol_id: SymbolId,
    pub symbol: String,
    pub tick_size: Decimal,
    pub lot_size: Decimal,
    pub min_notional: Decimal,
}

impl Instrument {
    /// Ronds a price mathematically down to the nearest multiple of `tick_size`
    /// safely using pure Decimal division, truncation, and multiplication.
    pub fn round_price_down(&self, price: Decimal) -> Decimal {
        if self.tick_size.is_zero() {
            return price;
        }
        (price / self.tick_size).trunc() * self.tick_size
    }

    /// Ronds a quantity mathematically down to the nearest multiple of `lot_size`.
    pub fn round_qty_down(&self, qty: Decimal) -> Decimal {
        if self.lot_size.is_zero() {
            return qty;
        }
        (qty / self.lot_size).trunc() * self.lot_size
    }

    /// Prepares an order by mutating its price and quantity to survive exchange precision limits.
    pub fn format_order(&self, order: &mut Order) {
        order.quantity = self.round_qty_down(order.quantity);
        if let Some(price) = order.price {
            order.price = Some(self.round_price_down(price));
        }
    }
}

/// A global dictionary of tradable instruments.
pub struct InstrumentManager {
    instruments: HashMap<SymbolId, Instrument>,
}

impl InstrumentManager {
    pub fn new() -> Self {
        Self {
            instruments: HashMap::new(),
        }
    }

    pub fn insert(&mut self, instrument: Instrument) {
        self.instruments.insert(instrument.symbol_id, instrument);
    }

    pub fn get(&self, symbol_id: SymbolId) -> Option<&Instrument> {
        self.instruments.get(&symbol_id)
    }
}

impl Default for InstrumentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_rounding_math() {
        let instr = Instrument {
            symbol_id: 1,
            symbol: "BTC-USDT".into(),
            tick_size: dec!(0.1),
            lot_size: dec!(0.001),
            min_notional: dec!(10.0),
        };

        // Price Rounding: 65432.126 -> 65432.1
        assert_eq!(instr.round_price_down(dec!(65432.126)), dec!(65432.1));
        
        // Qty Rounding: 1.234567 -> 1.234
        assert_eq!(instr.round_qty_down(dec!(1.234567)), dec!(1.234));
    }
}
