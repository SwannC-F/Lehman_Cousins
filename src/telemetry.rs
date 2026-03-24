//! Telemetry — structured logging (tracing) + Prometheus metrics exporter.

use anyhow::Result;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tracing_subscriber::{fmt, EnvFilter};

/// Initialise the global tracing subscriber.
///
/// Supports two output formats controlled by `log_format`:
/// - `"json"` — machine-readable, ideal for production / log aggregators
/// - anything else — human-readable "pretty" format for development
pub fn init(log_level: &str, log_format: &str) -> Result<()> {
    let filter = EnvFilter::new(log_level);

    match log_format {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_current_span(true)
                .with_span_list(true)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(true)
                .pretty()
                .init();
        }
    }

    Ok(())
}

/// Bind a Prometheus scrape endpoint on `host:port`.
/// The exporter runs as a background Tokio task; this function returns as soon
/// as the socket is successfully bound.
pub async fn start_metrics_exporter(host: &str, port: u16) -> Result<()> {
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .map_err(|e| anyhow::anyhow!("Failed to start metrics exporter: {e}"))?;

    tracing::info!(addr = %addr, "📊 Prometheus metrics exporter listening");
    Ok(())
}
