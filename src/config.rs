//! Application configuration.
//!
//! Reads from environment variables (via [`dotenvy`]) and validates all
//! required fields at startup. Fail-fast: a missing or invalid variable
//! kills the process before any network connection is established.

use anyhow::{Context, Result};
use std::env;

// ---------------------------------------------------------------------------
// Sub-configs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct ExchangeConfig {
    pub api_key: String,
    pub api_secret: String,
    pub rest_url: String,
    pub ws_url: String,
}

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub max_drawdown_pct: f64,
    pub max_position_usd: f64,
    pub max_open_orders: usize,
    pub daily_loss_limit_usd: f64,
    pub order_rate_limit_per_sec: u32,
}

#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub reconnect_delay_ms: u64,
    pub max_reconnect_attempts: u32,
    pub ping_interval_seconds: u64,
    pub message_buffer_size: usize,
}

// ---------------------------------------------------------------------------
// Root config
// ---------------------------------------------------------------------------

/// Top-level application configuration assembled from environment variables.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app_env: String,
    pub log_level: String,
    pub log_format: String,
    pub metrics_host: String,
    pub metrics_port: u16,
    pub database: DatabaseConfig,
    pub exchange: ExchangeConfig,
    pub risk: RiskConfig,
    pub websocket: WebSocketConfig,
}

impl AppConfig {
    /// Build config from environment. Panics early if required vars are absent.
    pub fn from_env() -> Result<Self> {
        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "development".into());

        // Select testnet vs prod exchange credentials based on APP_ENV
        let prefix = if app_env == "production" { "PROD" } else { "TESTNET" };

        Ok(Self {
            app_env: app_env.clone(),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into()),
            log_format: env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".into()),
            metrics_host: env::var("METRICS_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            metrics_port: env::var("METRICS_PORT")
                .unwrap_or_else(|_| "9090".into())
                .parse()
                .context("METRICS_PORT must be a valid u16")?,

            database: DatabaseConfig {
                url: env::var("DATABASE_URL").context("DATABASE_URL is required")?,
                max_connections: env_u32("DATABASE_MAX_CONNECTIONS", 10),
                min_connections: env_u32("DATABASE_MIN_CONNECTIONS", 2),
                timeout_seconds: env_u64("DATABASE_TIMEOUT_SECONDS", 30),
            },

            exchange: ExchangeConfig {
                api_key: env::var(format!("EXCHANGE_{prefix}_API_KEY"))
                    .context("Exchange API key is required")?,
                api_secret: env::var(format!("EXCHANGE_{prefix}_API_SECRET"))
                    .context("Exchange API secret is required")?,
                rest_url: env::var(format!("EXCHANGE_{prefix}_REST_URL"))
                    .context("Exchange REST URL is required")?,
                ws_url: env::var(format!("EXCHANGE_{prefix}_WS_URL"))
                    .context("Exchange WS URL is required")?,
            },

            risk: RiskConfig {
                max_drawdown_pct: env_f64("RISK_MAX_DRAWDOWN_PCT", 5.0),
                max_position_usd: env_f64("RISK_MAX_POSITION_USD", 50_000.0),
                max_open_orders: env_usize("RISK_MAX_OPEN_ORDERS", 20),
                daily_loss_limit_usd: env_f64("RISK_DAILY_LOSS_LIMIT_USD", 10_000.0),
                order_rate_limit_per_sec: env_u32("RISK_ORDER_RATE_LIMIT_PER_SEC", 10),
            },

            websocket: WebSocketConfig {
                reconnect_delay_ms: env_u64("WS_RECONNECT_DELAY_MS", 2_000),
                max_reconnect_attempts: env_u32("WS_MAX_RECONNECT_ATTEMPTS", 10),
                ping_interval_seconds: env_u64("WS_PING_INTERVAL_SECONDS", 20),
                message_buffer_size: env_usize("WS_MESSAGE_BUFFER_SIZE", 1_024),
            },
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
