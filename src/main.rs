//! # Lehman_Cousins — Main entry point
//!
//! Boots the Tokio runtime, loads configuration, initialises all subsystems
//! (feed ingestion, risk manager, order executor) and parks until a graceful
//! shutdown signal (SIGTERM / Ctrl-C) is received.

use anyhow::Result;
use tracing::info;

use lehman_cousins_core::{
    config::AppConfig,
    engine::Engine,
    telemetry,
};

#[tokio::main]
async fn main() -> Result<()> {
    // ── Load environment variables from .env (dev only) ──────────────────────
    dotenvy::dotenv().ok();

    // ── Load structured configuration ────────────────────────────────────────
    let config = AppConfig::from_env()?;

    // ── Initialise tracing / logging ─────────────────────────────────────────
    telemetry::init(&config.log_level, &config.log_format)?;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        env = %config.app_env,
        "🚀 Lehman_Cousins starting"
    );

    // ── Start Prometheus metrics exporter ────────────────────────────────────
    telemetry::start_metrics_exporter(&config.metrics_host, config.metrics_port).await?;

    // ── Build and run the trading engine ─────────────────────────────────────
    let engine = Engine::new(config).await?;
    engine.run().await?;

    info!("Engine stopped. Goodbye.");
    Ok(())
}
