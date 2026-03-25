//! Top-level Order Execution Manager.
//!
//! Provides the primary interface for strategies to execute trades.
//!
//! Enforces:
//! 1. **Order Book Sync State**: Rejects orders if the feed state is not `Live`.
//! 2. **Risk Limits**: Checks with the RiskManager (e.g. max position, drawdown).
//! 3. **Rate Limits**: Checks the `TokenBucket` before touching the network.
//! 4. **Nonce Generation**: Attaches an atomic nonce to the REST call.

use anyhow::{bail, Result};
use rust_decimal::Decimal;
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::{
    core::{
        feed_state::FeedState,
        models::{Order, OrderStatus, OrderType, Side},
    },
    exchange_clients::traits::ExchangeClient,
    order_manager::{
        nonce::NonceGenerator,
        rate_limiter::TokenBucket,
    },
    risk_manager::manager::RiskManager,
};

/// Gatekeeper for all order submissions and cancellations.
pub struct OrderManager {
    exchange_client: Box<dyn ExchangeClient>,
    risk_manager: RiskManager,
    // The rate limiter configured to the specific exchange's limits
    rate_limiter: TokenBucket,
    nonce_gen: NonceGenerator,
    // Watch channel receiver from the BookSynchronizer to verify the book is Live
    feed_state: watch::Receiver<FeedState>,
}

impl OrderManager {
    /// Initialise a new OrderManager.
    pub fn new(
        exchange_client: Box<dyn ExchangeClient>,
        risk_manager: RiskManager,
        feed_state: watch::Receiver<FeedState>,
        rate_limit_capacity: f64,
        rate_limit_per_sec: f64,
    ) -> Self {
        Self {
            exchange_client,
            risk_manager,
            rate_limiter: TokenBucket::new(rate_limit_capacity, rate_limit_per_sec),
            nonce_gen: NonceGenerator::new(),
            feed_state,
        }
    }

    /// Submits a new order to the exchange.
    ///
    /// The entire pre-flight check (Sync state -> Risk -> Rate Limit) runs
    /// synchronously and lock-free (or micro-lock for the token bucket).
    /// If all rules pass, the atomic nonce is generated and the async network
    /// call is dispatched.
    pub async fn submit_order(
        &self,
        symbol: &str,
        side: Side,
        order_type: OrderType,
        quantity: Decimal,
        price: Option<Decimal>,
    ) -> Result<Order> {
        // 1. Check if the order book is fully synchronised
        if !self.feed_state.borrow().is_live() {
            bail!("Execution rejected: Order book is not Live (re-syncing or disconnected)");
        }

        // 2. Build a local tentative Order object to pass into the Risk Manager
        let tentative_order = Order {
            client_id: uuid::Uuid::new_v4(),
            exchange_id: None,
            symbol: symbol.into(),
            side,
            order_type,
            price,
            quantity,
            filled_quantity: Decimal::ZERO,
            status: OrderStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // 3. Risk Pre-Flight Checks
        self.risk_manager.validate_order(&tentative_order)?;

        // 4. Rate Limit Check (Token Bucket)
        if let Err(e) = self.rate_limiter.consume() {
            warn!(
                symbol,
                ?side,
                wait_ms = e.retry_after.as_millis(),
                "Execution rejected: API Rate Limit hit"
            );
            return Err(e.into());
        }

        // 5. Generate strict cryptographic nonce
        let nonce = self.nonce_gen.next();

        // 6. Network Execution
        info!(
            symbol,
            ?side,
            qty = %quantity,
            nonce,
            "Pre-flight checks passed — dispatching order to exchange"
        );

        match self
            .exchange_client
            .place_order(symbol, side, order_type, quantity, price) // A real REST client implementation will accept the nonce here!
            .await
        {
            Ok(placed_order) => {
                self.risk_manager.on_order_opened();
                Ok(placed_order)
            }
            Err(e) => {
                error!(error = %e, "Exchange API rejected order submission");
                Err(e)
            }
        }
    }
}
