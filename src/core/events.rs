//! Internal event bus types.
//!
//! [`MarketEvent`] is broadcast over a Tokio broadcast channel from the
//! feed ingestion layer to all registered strategies. No logic lives here.

use crate::core::models::{ExecutionReport, OrderBookUpdate, Trade};

/// All market events that flow through the internal broadcast bus.
#[derive(Debug, Clone)]
pub enum MarketEvent {
    /// A new trade was executed on the exchange.
    Trade(Trade),

    /// An order-book snapshot or incremental diff was received.
    OrderBook(OrderBookUpdate),

    /// The exchange connection was (re)established.
    Connected { exchange: String },

    /// The exchange connection was lost.
    Disconnected { exchange: String, reason: String },

    /// A private execution report for a managed order.
    ExecutionReport(ExecutionReport),
}
