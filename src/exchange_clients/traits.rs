//! Exchange client abstraction trait.
//!
//! All exchange integrations (Binance, OKX, Bybit, …) must implement this
//! trait so they are interchangeable from the engine's perspective.

use anyhow::Result;
use async_trait::async_trait;

use crate::core::events::MarketEvent;
use crate::core::models::{Order, OrderBookUpdate, OrderStatus, Side, OrderType, SymbolId};
use rust_decimal::Decimal;

/// Unified interface every exchange client must satisfy.
#[async_trait]
pub trait ExchangeClient: Send + Sync + 'static {
    /// Submit a new order to the exchange.
    async fn submit_order(&self, order: &Order) -> Result<()>;

    /// Fetch real-world open positions to hydrate the inventory at boot (Fail-Fast).
    async fn fetch_positions(&self) -> Result<Vec<(SymbolId, Decimal)>>;

    /// Cancel all open orders for emergency shutdown.
    async fn cancel_all_orders(&self) -> Result<()>;

    /// Human-readable exchange identifier (e.g. `"binance"`, `"okx"`).
    fn name(&self) -> &str;

    // ── Market data ──────────────────────────────────────────────────────────

    /// Fetch a **full L2 order book snapshot** for `symbol` via REST.
    ///
    /// This is called by the [`BookSynchronizer`] as the "heavy" REST step
    /// during the `Buffering → Syncing` transition. The returned update's
    /// `sequence` field MUST match the exchange's own sequence numbering so
    /// the synchronizer can reconcile it with buffered WebSocket deltas.
    ///
    /// Do NOT call this on the WebSocket hot path — it is a blocking network
    /// call and will stall the feed. It runs inside a dedicated Tokio task.
    async fn fetch_order_book_snapshot(
        &self,
        symbol: &str,
        depth: u32,
    ) -> Result<OrderBookUpdate>;

    // ── Order management ─────────────────────────────────────────────────────

    /// Submit a new order. Returns the order with `exchange_id` populated.
    async fn place_order(
        &self,
        symbol: &str,
        side: Side,
        order_type: OrderType,
        quantity: Decimal,
        price: Option<Decimal>,
    ) -> Result<Order>;

    /// Cancel an open order by its exchange-assigned identifier.
    async fn cancel_order(&self, symbol: &str, exchange_id: &str) -> Result<()>;

    /// Fetch the current status of an order.
    async fn get_order_status(&self, symbol: &str, exchange_id: &str) -> Result<OrderStatus>;

    // ── Account ──────────────────────────────────────────────────────────────

    /// Fetch the available balance for a given asset (e.g. `"USDT"`).
    async fn get_balance(&self, asset: &str) -> Result<Decimal>;
}
