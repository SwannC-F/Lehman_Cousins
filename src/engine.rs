//! Trading Engine orchestrator.
//!
//! The [`Engine`] owns all top-level subsystems and coordinates their
//! lifecycle. Strategies, exchange clients, and the risk manager are wired
//! together here via Tokio channels.
//!
//! No trading logic lives in this file — it is pure infrastructure wiring.

use anyhow::Result;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::{
    config::AppConfig,
    core::events::MarketEvent,
    exchange_clients::{traits::ExchangeClient, websocket_client::WebSocketFeedClient},
    risk_manager::manager::RiskManager,
    strategies::traits::Strategy,
};

/// Channel capacity for the market-event broadcast bus.
const EVENT_BUS_CAPACITY: usize = 4_096;

/// The top-level engine that owns and drives all subsystems.
pub struct Engine {
    config: AppConfig,
    // Future fields added by feature branches:
    // strategies: Vec<Box<dyn Strategy>>,
    // exchange_client: Box<dyn ExchangeClient>,
    // risk_manager: RiskManager,
}

impl Engine {
    /// Construct the engine and initialise all subsystems.
    ///
    /// This is the correct place to:
    /// - connect to the database pool
    /// - instantiate exchange clients
    /// - inject the risk manager
    /// - register strategies
    pub async fn new(config: AppConfig) -> Result<Self> {
        info!("Initialising engine subsystems…");

        // TODO: initialise DB pool (sqlx::PgPool::connect)
        // TODO: instantiate WebSocketFeedClient
        // TODO: create RiskManager from config.risk
        // TODO: load strategies from config / plugin system

        Ok(Self { config })
    }

    /// Run the engine until a shutdown signal is received.
    ///
    /// Internally this:
    /// 1. Starts the market-data feed ingestion task
    /// 2. Starts the strategy evaluation loop
    /// 3. Starts the order execution worker
    /// 4. Waits for SIGTERM / Ctrl-C and gracefully shuts down each task
    pub async fn run(self) -> Result<()> {
        info!("Engine running — awaiting shutdown signal (Ctrl-C / SIGTERM)");

        // ── Broadcast channel: feed → strategies ──────────────────────────────
        let (_tx, _rx) = broadcast::channel::<MarketEvent>(EVENT_BUS_CAPACITY);

        // TODO: spawn feed ingestion task
        // TODO: spawn strategy loop task(s)
        // TODO: spawn order execution task

        // ── Graceful shutdown ─────────────────────────────────────────────────
        tokio::signal::ctrl_c().await?;
        warn!("Shutdown signal received — stopping subsystems…");

        // TODO: send cancellation tokens to all spawned tasks and await them

        Ok(())
    }
}
