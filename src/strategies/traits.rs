//! Strategy abstraction trait.
//!
//! Each concrete trading strategy (StatArb, Market Making, …) must implement
//! this trait. The engine calls `on_event` for every market event it receives.
//! Order submission goes through the injected `ExchangeClient`.
//!
//! No concrete strategy logic is implemented here.

use anyhow::Result;
use async_trait::async_trait;

use crate::{
    core::events::MarketEvent,
    exchange_clients::traits::ExchangeClient,
};

/// The interface every strategy module must satisfy.
#[async_trait]
pub trait Strategy: Send + Sync + 'static {
    /// Human-readable strategy name used in logs and metrics.
    fn name(&self) -> &str;

    /// Called once before the event loop begins.
    /// Use to warm up state, load parameters, or subscribe to feeds.
    async fn on_start(&mut self, client: &dyn ExchangeClient) -> Result<()>;

    /// Called for every [`MarketEvent`] broadcast on the internal bus.
    /// Implementations should be non-blocking; heavy computation must be
    /// dispatched to a blocking thread via `tokio::task::spawn_blocking`.
    async fn on_event(
        &mut self,
        event: &MarketEvent,
        client: &dyn ExchangeClient,
    ) -> Result<()>;

    /// Called when a graceful shutdown is requested.
    /// Cancel open orders and flush any in-flight state here.
    async fn on_stop(&mut self, client: &dyn ExchangeClient) -> Result<()>;
}
