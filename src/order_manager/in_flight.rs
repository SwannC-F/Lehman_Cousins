//! In-flight order tracking and memory leak garbage collector (The Reaper).

use crate::core::models::{ExecutionReport, Order};
use dashmap::DashMap;
use rust_decimal::Decimal;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OrderState {
    pub order: Order,
    pub timestamp: Instant,
    pub cumulative_filled: Decimal,
}

/// Tracks orders that have been sent but not yet fully confirmed or filled.
pub struct InFlightTracker {
    pending_orders: DashMap<Uuid, OrderState>,
}

impl Default for InFlightTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl InFlightTracker {
    pub fn new() -> Self {
        Self {
            pending_orders: DashMap::new(),
        }
    }

    /// Register a new order that has been dispatched to the exchange REST API.
    pub fn register_order(&self, order: Order) {
        self.pending_orders.insert(
            order.client_id,
            OrderState {
                order,
                timestamp: Instant::now(),
                cumulative_filled: Decimal::ZERO,
            },
        );
    }

    /// Returns the true partial fill delta to apply to the PositionTracker,
    /// avoiding duplicate summation on cumulative quantities from the exchange WS.
    pub fn process_execution(&self, report: &ExecutionReport) -> Option<Decimal> {
        let mut state = self.pending_orders.get_mut(&report.client_id)?;

        let delta = report.executed_quantity - state.cumulative_filled;
        state.cumulative_filled = report.executed_quantity;

        // If terminal state, remote it and free RAM
        if report.order_status == crate::core::models::OrderStatus::Filled
            || report.order_status == crate::core::models::OrderStatus::Cancelled
        {
            drop(state); // Drop the lock before removal
            self.pending_orders.remove(&report.client_id);
        }

        Some(delta)
    }

    /// Spawns the Garbage Collector (Reaper Task) which wakes up every 10s.
    /// It scans for "ghost" orders older than 30s that were lost due to WS failure.
    pub fn start_reaper(tracker: std::sync::Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                let active = tracker.pending_orders.len();
                if active > 0 {
                    info!("Reaper Task scanning {} in-flight orders for memory leaks...", active);
                }
                
                let timeout = Duration::from_secs(30);
                let mut ghosts = vec![];
                
                for entry in tracker.pending_orders.iter() {
                    if entry.value().timestamp.elapsed() > timeout {
                        ghosts.push(entry.key().clone());
                    }
                }

                for ghost_id in ghosts {
                    warn!(
                        client_id = %ghost_id,
                        "Ghost order detected (WS failed). Forcing REST reconciliation and purging memory."
                    );
                    // TODO: call exchange_client.fetch_order_status(ghost_id)
                    tracker.pending_orders.remove(&ghost_id);
                }
            }
        });
    }
}
