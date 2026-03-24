//! Strategy abstraction trait.
//!
//! Each concrete trading strategy (StatArb, Market Making, …) must implement
//! this trait. To ensure backtest parity and prevent I/O blocking, the strategy
//! is a pure mathematical construct. It reacts to events and returns orders.
//! It must never hold network bindings (`ExchangeClient`).

use anyhow::Result;

use crate::{
    core::events::MarketEvent,
    core::models::Order,
};

/// The pure, mathematics-only interface every strategy module must satisfy.
pub trait Strategy: Send + Sync + 'static {
    /// Human-readable strategy name used in logs and metrics.
    fn name(&self) -> &str;

    /// Called once before the event loop begins.
    /// Pure state initialization.
    fn on_start(&mut self) -> Result<()>;

    /// Pure mathematical function: takes an event, returns optional Orders.
    /// Completely uncoupled from network async routines.
    fn on_event(&mut self, event: &MarketEvent) -> Option<Vec<Order>>;

    /// Called when stopping.
    fn on_stop(&mut self) -> Result<()>;
}
