//! Telemetry — structured logging (tracing) + Prometheus metrics exporter.

use anyhow::Result;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

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

/// FinOps Alerting: Asynchronous HTTP Webhook client for Slack/Discord.
///
/// Designed with a strict 2s timeout and standalone reqwest connection pool
/// to prevent stalling the main algorithmic event loop during network degradation.
#[derive(Clone)]
pub struct AlertNotifier {
    client: reqwest::Client,
    webhook_url: String,
}

impl AlertNotifier {
    /// Creates a new notifier binding with a rigorous internal timeout.
    pub fn new(webhook_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .expect("Failed to build Notifier HTTP client");
            
        Self { client, webhook_url }
    }

    /// Dispatches a critical severity alert asynchronously.
    /// This method is Fire-and-Forget.
    pub fn send_alert(&self, title: &str, description: &str) {
        let client = self.client.clone();
        let url = self.webhook_url.clone();
        
        let payload = format!(
            r#"{{"content": "**[OPS FATAL] {}**\n{}"}}"#,
            title, description
        );

        tokio::spawn(async move {
            let res = client.post(&url)
                .header("Content-Type", "application/json")
                .body(payload)
                .send()
                .await;
                
            if let Err(e) = res {
                tracing::error!("Failed to dispatch Slack/Discord Webhook: {}", e);
            }
        });
    }
}
