//! Trading Engine orchestrator.
//!
//! The [`Engine`] owns all top-level subsystems and coordinates their
//! lifecycle. Strategies, exchange clients, and the risk manager are wired
//! together here via Tokio channels.
//!
//! No trading logic lives in this file — it is pure infrastructure wiring.

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};

use crate::{
    config::AppConfig,
    core::events::MarketEvent,
    core::models::Order,
    exchange_clients::{traits::ExchangeClient, websocket_client::WebSocketFeedClient},
    risk_manager::manager::RiskManager,
    strategies::traits::Strategy,
};

/// Channel capacity for the market-event broadcast bus.
const EVENT_BUS_CAPACITY: usize = 4_096;

/// The top-level engine that owns and drives all subsystems.
pub struct Engine {
    config: AppConfig,
    strategies: Vec<Box<dyn Strategy>>,
    exchange_client: Option<Box<dyn ExchangeClient>>,
    risk_manager: RiskManager,
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
        let risk_manager = RiskManager::new(config.risk.clone());

        // --- BOOT RECONCILIATION ---
        // TODO: Once exchange_client is concrete, block on boot:
        // let open_positions = exchange_client.fetch_positions().await?;
        // for (sym, qty) in open_positions {
        //     risk_manager.position_tracker().set_position(sym, qty);
        // }
        // info!("Inventory reconciled from exchange.");

        Ok(Self {
            config,
            strategies: vec![],
            exchange_client: None,
            risk_manager,
        })
    }

    /// Run the engine until a shutdown signal is received.
    ///
    /// Internally this:
    /// 1. Starts the market-data feed ingestion task
    /// 2. Starts the strategy evaluation loop
    /// 3. Starts the order execution worker
    /// 4. Waits for SIGTERM / Ctrl-C and gracefully shuts down each task
    pub async fn run(mut self) -> Result<()> {
        info!("Engine running — awaiting shutdown signal (Ctrl-C / SIGTERM)");

        // ── Broadcast channel: feed → strategies ──────────────────────────────
        let (_tx, _rx) = broadcast::channel::<MarketEvent>(EVENT_BUS_CAPACITY);

        // ── Strategy-Execution Bridge (MPSC) ──────────────────────────────────
        let (order_tx, mut order_rx) = mpsc::channel::<Order>(1024);

        // TODO: Pass order_tx to strategies. Strategies must use try_send:
        // if let Err(e) = order_tx.try_send(order) {
        //     warn!("Order dropped, execution channel full (Backpressure limit)");
        // }

        let risk_arc = std::sync::Arc::new(self.risk_manager);
        
        // Execution worker thread
        tokio::spawn(async move {
            while let Some(order) = order_rx.recv().await {
                if let Err(e) = risk_arc.validate_order(&order) {
                    warn!(error = %e, "Risk limit breached, order rejected");
                    continue;
                }
                // TODO: exchange_client.submit_order(&order).await;
            }
        });

        // ── Graceful shutdown interception (SIGINT + SIGTERM) ─────────────────
        let mut sigterm = signal(SignalKind::terminate())?;
        
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                warn!("SIGINT (Ctrl-C) received — stopping subsystems…");
            }
            _ = sigterm.recv() => {
                warn!("SIGTERM received — stopping subsystems…");
            }
        }

        // ── The Shield: Cancel All Orders ─────────────────────────────────────
        if let Some(client) = self.exchange_client {
            info!("Executing REST Cancel All open orders...");
            if let Err(e) = client.cancel_all_orders().await {
                 error!(error = %e, "Failed to cancel orders gracefully on shutdown!");
            }
        }

        Ok(())
    }
}
