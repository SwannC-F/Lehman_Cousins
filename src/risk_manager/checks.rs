//! Stateless risk check functions.
//!
//! Each function encodes one atomic risk rule and returns `Err` with a
//! descriptive message when the rule is violated. The [`RiskManager`] calls
//! these in sequence; new rules are added here without touching the manager.

use anyhow::{bail, Result};
use rust_decimal::Decimal;

/// Ensure the open order count does not exceed the configured cap.
pub fn check_open_order_count(current: usize, max: usize) -> Result<()> {
    if current >= max {
        bail!(
            "Risk check failed: open order count {current} >= max {max}"
        );
    }
    Ok(())
}

/// Ensure the notional value of a single order does not exceed the position cap.
pub fn check_notional(price: Decimal, quantity: Decimal, max_usd: Decimal) -> Result<()> {
    let notional = price * quantity;
    if notional > max_usd {
        bail!(
            "Risk check failed: notional {notional} > max_position_usd {max_usd}"
        );
    }
    Ok(())
}

/// Ensure a price quote is within a sane range of the reference price.
///
/// `tolerance_bps`: allowed deviation in basis points (1 bps = 0.01 %).
pub fn check_price_sanity(
    quote_price: Decimal,
    reference_price: Decimal,
    tolerance_bps: u32,
) -> Result<()> {
    if reference_price.is_zero() {
        bail!("Risk check failed: reference price is zero");
    }
    let deviation_bps =
        ((quote_price - reference_price).abs() / reference_price) * Decimal::from(10_000u32);
    let max_bps = Decimal::from(tolerance_bps);
    if deviation_bps > max_bps {
        bail!(
            "Risk check failed: price deviation {deviation_bps} bps > tolerance {tolerance_bps} bps"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn notional_check_passes_below_limit() {
        assert!(check_notional(dec!(30_000), dec!(1), dec!(50_000)).is_ok());
    }

    #[test]
    fn notional_check_fails_above_limit() {
        assert!(check_notional(dec!(30_000), dec!(2), dec!(50_000)).is_err());
    }

    #[test]
    fn open_order_count_check() {
        assert!(check_open_order_count(5, 20).is_ok());
        assert!(check_open_order_count(20, 20).is_err());
    }
}
