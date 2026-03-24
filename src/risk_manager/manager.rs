//! Risk Manager.
//!
//! Acts as a pre-flight gate for every order a strategy wishes to submit.
//! All risk checks must pass before the order is forwarded to the exchange.
//!
//! The manager tracks running state (e.g. open order count, daily P&L) and
//! triggers a circuit-breaker when thresholds are breached.
//!
//! No strategy-specific logic lives here — only cross-cutting risk rules.

use anyhow::{bail, Result};
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{error, warn};

use crate::{
    config::RiskConfig,
    core::models::{Order, Side},
    risk_manager::{checks, inventory::PositionTracker},
};

/// Shared state managed by the risk manager.
pub struct RiskManager {
    config: RiskConfig,
    open_order_count: AtomicU32,
    position_tracker: PositionTracker,
    // Additional state (daily P&L, peak equity) to be added here.
}

impl RiskManager {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            open_order_count: AtomicU32::new(0),
            position_tracker: PositionTracker::new(),
        }
    }

    /// Validate a proposed order against all active risk rules.
    ///
    /// Returns `Ok(())` if the order may proceed, or an error describing
    /// which check failed.
    pub fn validate_order(&self, order: &Order) -> Result<()> {
        checks::check_open_order_count(
            self.open_order_count.load(Ordering::Relaxed) as usize,
            self.config.max_open_orders,
        )?;

        if let Some(price) = order.price {
            checks::check_notional(
                price,
                order.quantity,
                Decimal::from_f64_retain(self.config.max_position_usd)
                    .unwrap_or(Decimal::MAX),
            )?;
        }

        // TODO: checks::check_rate_limit(...)
        // TODO: checks::check_daily_loss(...)

        Ok(())
    }

    /// Notify the manager that a new order was accepted by the exchange.
    pub fn on_order_opened(&self) {
        self.open_order_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Notify the manager of an execution to update internal inventory.
    pub fn on_execution_report(&self, report: &crate::core::models::ExecutionReport) {
        self.position_tracker.apply_execution(report);
    }

    /// Notify the manager that an order reached a terminal state.
    pub fn on_order_closed(&self) {
        self.open_order_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// Trigger a full circuit-breaker halt (logs + future: broadcasts halt signal).
    pub fn trigger_halt(&self, reason: &str) {
        error!(reason, "🚨 CIRCUIT BREAKER TRIGGERED — halting all trading");
        // TODO: send halt signal on a dedicated channel so all strategies stop.
    }
}
