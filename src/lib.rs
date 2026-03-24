//! # Lehman_Cousins Core Library
//!
//! Central re-export hub for all subsystem modules.
//! The binary (`main.rs`) and integration tests both link against this crate.

pub mod config;
pub mod engine;
pub mod telemetry;

pub mod core {
    pub mod events;
    pub mod models;
    pub mod orderbook;
    pub mod instrument;
    pub mod feed_state;
}

pub mod exchange_clients {
    pub mod traits;
    pub mod websocket_client;
    pub mod rest_client;
    pub mod book_sync;
}

pub mod strategies {
    pub mod traits;
}

pub mod risk_manager {
    pub mod manager;
    pub mod checks;
    pub mod inventory;
}

pub mod order_manager {
    pub mod manager;
    pub mod nonce;
    pub mod rate_limiter;
    pub mod in_flight;
}

pub mod utils {
    pub mod time;
    pub mod math;
    pub mod retry;
}
