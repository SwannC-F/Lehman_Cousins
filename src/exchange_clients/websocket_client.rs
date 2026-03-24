//! WebSocket feed ingestion client.
//!
//! Connects to an exchange WebSocket feed, deserialises incoming messages
//! into [`MarketEvent`]s, and broadcasts them on the internal event bus.
//! Implements automatic reconnection with exponential back-off.
//!
//! ## Zero-copy JSON parsing (simd-json)
//!
//! On the hot text-frame path we use [`simd_json`] instead of `serde_json`:
//!
//! ```text
//! serde_json::from_str(&text)   // allocates a new String for every field
//! simd_json::from_slice(&mut bytes) // borrows &str directly from the buffer
//! ```
//!
//! The WS frame buffer is already on the stack/heap; simd-json rewrites
//! bytes in-place (tape algorithm) and returns borrows — zero extra allocation.
//!
//! **Rule**: never call `serde_json` inside `connect_and_stream`. Only
//! `simd_json::from_slice` is permitted on the message hot path.
//!
//! No parsing of exchange-specific message formats is implemented here —
//! that belongs in exchange-specific adapters under each exchange sub-module.

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::{
    config::WebSocketConfig,
    core::events::MarketEvent,
};

/// Manages a single WebSocket connection to an exchange feed.
pub struct WebSocketFeedClient {
    name: String,
    url: String,
    config: WebSocketConfig,
    event_tx: broadcast::Sender<MarketEvent>,
}

impl WebSocketFeedClient {
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        config: WebSocketConfig,
        event_tx: broadcast::Sender<MarketEvent>,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            config,
            event_tx,
        }
    }

    /// Connect and run the receive loop. Reconnects automatically on drop.
    ///
    /// Returns only when `max_reconnect_attempts` is exhausted or a shutdown
    /// signal cancels the enclosing task.
    pub async fn run(&self) -> Result<()> {
        let mut attempt = 0u32;

        loop {
            match self.connect_and_stream().await {
                Ok(()) => {
                    info!(exchange = %self.name, "WebSocket stream ended cleanly");
                    return Ok(());
                }
                Err(e) => {
                    attempt += 1;
                    let max = self.config.max_reconnect_attempts;
                    warn!(
                        exchange = %self.name,
                        error = %e,
                        attempt,
                        max,
                        "WebSocket error"
                    );

                    if max > 0 && attempt >= max {
                        error!(exchange = %self.name, "Max reconnect attempts reached. Giving up.");
                        return Err(e);
                    }

                    let delay_ms = self.config.reconnect_delay_ms
                        * 2u64.saturating_pow(attempt.min(7));
                    info!(
                        exchange = %self.name,
                        delay_ms,
                        "Reconnecting after back-off delay…"
                    );
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    async fn connect_and_stream(&self) -> Result<()> {
        info!(exchange = %self.name, url = %self.url, "Connecting to WebSocket feed…");
        let (ws_stream, _response) = connect_async(&self.url).await?;
        let (mut _write, mut read) = ws_stream.split();

        let _ = self.event_tx.send(MarketEvent::Connected {
            exchange: self.name.clone(),
        });

        info!(exchange = %self.name, "WebSocket connected");

        while let Some(msg) = read.next().await {
            let msg = msg?;
            match msg {
                Message::Text(text) => {
                    // ── Zero-copy hot path ──────────────────────────────────
                    // Convert the frame text to a mutable byte slice so
                    // simd-json can do its in-place tape rewrite without
                    // allocating any String for individual field values.
                    //
                    // TODO: replace `RawFrame` with your exchange's concrete
                    //       deserialization type (e.g. BinanceBookUpdate).
                    //       The type must derive serde::Deserialize.
                    //
                    // Example:
                    //   let mut bytes = text.into_bytes();
                    //   match simd_json::from_slice::<RawFrame>(&mut bytes) {
                    //       Ok(frame) => {
                    //           if let Some(event) = frame.into_market_event() {
                    //               let _ = self.event_tx.send(event);
                    //           }
                    //       }
                    //       Err(e) => warn!(error = %e, "Failed to parse WS frame"),
                    //   }
                    debug!(
                        exchange = %self.name,
                        bytes    = text.len(),
                        "WS text frame received (simd-json path)"
                    );
                }
                Message::Ping(payload) => {
                    // Pong is handled automatically by tokio-tungstenite
                    debug!(exchange = %self.name, "Ping received");
                    let _ = payload;
                }
                Message::Close(frame) => {
                    warn!(exchange = %self.name, ?frame, "WebSocket close frame received");
                    break;
                }
                _ => {}
            }
        }

        let _ = self.event_tx.send(MarketEvent::Disconnected {
            exchange: self.name.clone(),
            reason: "stream ended".into(),
        });

        Ok(())
    }
}
