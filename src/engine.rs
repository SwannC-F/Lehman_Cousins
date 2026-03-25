//! Trading Engine orchestrator.
//!
//! The [`Engine`] owns all top-level subsystems and coordinates their
//! lifecycle. Strategies, exchange clients, and the risk manager are wired
//! together here via Tokio channels.
//!
//! No trading logic lives in this file — it is pure infrastructure wiring.

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};
use tokio::task;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};

use crate::{
    config::AppConfig,
    core::events::MarketEvent,
    core::models::Order,
    exchange_clients::traits::ExchangeClient,
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
    pub async fn run(self) -> Result<()> {
        info!("Engine running — awaiting shutdown signal (Ctrl-C / SIGTERM)");

        // ── Broadcast channel: feed → strategies ──────────────────────────────
        let (_tx, _rx) = broadcast::channel::<MarketEvent>(EVENT_BUS_CAPACITY);

        // ── Strategy-Execution Bridge (MPSC) ──────────────────────────────────
        let (order_tx, mut order_rx) = mpsc::channel::<Order>(1024);

        // ── Isolate Strategies (Pure Math & No Network) ───────────────────────
        let _tx_clone = _tx.clone();
        let strategy_tx = order_tx.clone();
        for mut strategy in self.strategies {
            let mut sub = _tx_clone.subscribe();
            let strategy_tx = strategy_tx.clone();
            tokio::spawn(async move {
                let _ = strategy.on_start();
                while let Ok(event) = sub.recv().await {
                    // PURE MATH: completely synchronous, no async runtime needed.
                    // Allows exact parity with Backtest framework.
                    if let Some(orders) = strategy.on_event(&event) {
                        for ord in orders {
                            if let Err(e) = strategy_tx.try_send(ord) {
                                warn!("Order dropped (Backpressure Limit / Fail-Fast): {}", e);
                            }
                        }
                    }
                }
                let _ = strategy.on_stop();
            });
        }

        let risk_arc = std::sync::Arc::new(self.risk_manager);
        
        let in_flight = std::sync::Arc::new(crate::order_manager::in_flight::InFlightTracker::new());
        // Start the Garbage Collector for ghost orders (WS deaths)
        crate::order_manager::in_flight::InFlightTracker::start_reaper(in_flight.clone());
        
        // Execution worker thread
        tokio::spawn(async move {
            while let Some(order) = order_rx.recv().await {
                if let Err(e) = risk_arc.validate_order(&order) {
                    warn!(error = %e, "Risk limit breached, order rejected");
                    continue;
                }

                // --- INSTRUMENT METADATA ROUNDING ---
                // TODO: let instrument = instrument_manager.get(order.symbol_id);
                // instrument.format_order(&mut order);

                // --- IN-FLIGHT ORDER REGISTRATION ---
                in_flight.register_order(order.clone());

                // --- HMAC CPU OFFLOADING ---
                // SHA-256 signing takes ~2µs. Doing this on the main event loop 1000 times
                // stalls the WebSocket ingestion. We push cryptography to the blocking pool.
                let _signed_payload = task::spawn_blocking(move || {
                    // simulate cryptographic hashing logic here
                    let _heavy_math = 2 + 2; 
                    "signed_payload"
                }).await.unwrap();

                // TODO: exchange_client.submit_order(&order, _signed_payload).await;
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
