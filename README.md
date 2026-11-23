# Lehman Cousins

A Rust skeleton for a statistical-arbitrage trading engine on crypto exchanges, starting with Bybit spot. It separates the strategy code from everything network-related, so the same strategy can run in a backtest or live.

Personal project, March 2026.

## Architecture

```
exchange_clients/   WebSocket + REST clients, Bybit adapter, order-book sync
core/               order book, instruments (tick and lot sizes), market events
strategies/         Strategy trait: on_event(MarketEvent) -> orders
order_manager/      in-flight orders, nonces, token-bucket rate limiter
risk_manager/       position tracking and pre-trade checks
engine.rs           Tokio event loop wiring feeds, strategy, risk and orders
bin/backtest.rs     synchronous replay of historical CSV ticks with a paper trader
telemetry.rs        alerting and persistence hooks
```

Design choices:
- **The strategy never touches the network.** It receives market events and returns orders. The same code runs in the backtest and in the live engine.
- **Exact prices.** Prices and quantities are `rust_decimal` values, rounded to each instrument's tick and lot size.
- **Backpressure.** Events reach the strategy through a bounded channel. When the rate limiter is saturated, orders are dropped instead of blocking the loop.
- **A conservative paper trader.** Buys fill at the ask, sells at the bid, and a 0.1 % taker fee is charged.
- **Safe startup and shutdown.** At boot the engine reconciles its positions with the exchange. On SIGTERM or SIGINT it cancels all open orders.
- **Deployment.** The Docker image uses a multi-stage build on distroless and runs as a non-root user. Docker Compose adds TimescaleDB, and a GitHub Actions workflow runs clippy and the tests.

## Status

This is an engineering skeleton, and it has never traded.
- The strategy is a placeholder (`DummyStatArb`).
- Bybit order-book messages are parsed but not yet mapped to events.
- Sending orders is commented out.
- Some risk checks and the telemetry are stubs.

These gaps are marked `TODO` in the code.

## Run

```bash
cargo run --bin backtest        # replay historical ticks with the paper trader
cargo test
```

## Stack

Rust · Tokio · rust_decimal · DashMap · simd-json · reqwest · Docker · TimescaleDB · GitHub Actions
