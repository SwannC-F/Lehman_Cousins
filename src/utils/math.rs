//! Financial math helpers.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Compute basis points spread between two prices.
///
/// Returns `None` if `reference` is zero to avoid division by zero.
pub fn spread_bps(price: Decimal, reference: Decimal) -> Option<Decimal> {
    if reference.is_zero() {
        return None;
    }
    Some(((price - reference).abs() / reference) * dec!(10_000))
}

/// Convert basis points to a decimal factor (e.g. 50 bps → 0.005).
#[inline]
pub fn bps_to_factor(bps: Decimal) -> Decimal {
    bps / dec!(10_000)
}

/// Round `value` to `places` decimal places (half-up).
pub fn round_to(value: Decimal, places: u32) -> Decimal {
    value.round_dp(places)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_bps_calculation() {
        let spread = spread_bps(dec!(100.5), dec!(100)).unwrap();
        assert_eq!(spread, dec!(50)); // 0.5% = 50 bps
    }
}
