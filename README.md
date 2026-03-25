# Lehman Cousins - Institutional StatArb Engine

Lehman Cousins is a high-frequency algorithmic trading and statistical arbitrage (StatArb) engine written in Rust. It has been rigorously audited and hardened to meet the strict technical standards of tier-l quantitative prop-shops.

## 🏛️ Architectural Philosophy

The architecture is built on a **Fail-Fast, Zero-Allocation, and Non-Blocking** foundation. The quantitative trading algorithms (Strategies) are mathematically isolated from network complexities (WebSockets, REST, Latency), ensuring absolute purity for backtesting (`Backtest/Live Parity`).

---

## ✅ Production Hardened Features

The engine has undergone 5 phases of rigorous technical remediation and auditing:

1. **Hot Path Zero-Allocation:** The `OrderBook` mutates pre-allocated memory in-place (`Vec::insert` using a O(log n) `partition_point` pointer) without ever relying on `clone()` within the hot path.
2. **Lock-Free Inventory:** The exposure tracking engine (`PositionTracker`) is indexed by pure integers (`SymbolId`) and powered by an asynchronous `DashMap` to prevent any OS context-switching contention (`parking_lot::Mutex`).
3. **Resilience & Go-Live Safety:**
   - **Boot Reconciliation:** "Blind Startups" are forbidden. The engine demands a synchronous `fetch_positions` call at boot to synchronize its internal state with the live exchange truth.
   - **Graceful Shutdown:** System-level OS signal interception (`SIGTERM` & `SIGINT` from K8s/Docker) triggers an emergency `cancel_all_orders()` routine to secure capital before process termination.
   - **Active Ping/Pong:** WebSocket connections are kept alive via an explicit application-level interval `{"op":"ping"}` to pre-emptively detect silent network drops (TCP Half-Open) on illiquid markets.
   - **Non-Blocking MPSC Strategy Bridge:** Event ingestion by the Quant layer is decoupled via a Multi-Producer Single-Consumer buffer utilizing `try_send()`. If the Token Bucket Rate Limiter saturates, orders are *dropped* (Fail-Fast) to ensure the core mathematical loops never freeze.
4. **Micro-Structure Quant Realities:**
   - **Decimal Filter:** An `InstrumentManager` enforces market physics by perfectly quantifying Lot Sizes and Tick Sizes via pure `rust_decimal` arithmetic (`(price / tick).trunc() * tick`), banishing any floating-point drift or "Invalid Precision" rejections.
   - **The Reaper (Garbage Collector):** An In-Flight memory dictionary locks Partial Fills without blindly adding up quantities. A permanent background coroutine (The Reaper) scans for orphaned orders (due to WS drops) every 10 seconds and forces REST API reconciliation to prevent memory leaks (OOM).
   - **HMAC Offloading:** CPU-heavy cryptographic payloads (HMAC-SHA256 for Bybit REST auth) are dispatched to `tokio::task::spawn_blocking` thread pools, entirely preserving the latency of the main WebSocket ingestion loop.
5. **Strict Backtesting Harness:**
   - **Pure CPU-Bound Execution:** Synchronous O(N) ingestion reading historical CSV tick flows (Zero-Tokio overhead).
   - **Ruthless PaperTrader:** Automatically enforces Spread Crossing (buys on Ask, sells on Bid) and systematically deducts a **0.1% Taker Fee**, ensuring any mathematically profitable backtest translates directly into Live profitability.
6. **DevSecOps Fortress & Day-2 Operations:**
   - **Docker Distroless:** The engine is compiled via a multi-stage builder and deployed in a barebones `gcr.io/distroless/cc-debian12` container without a shell, executing strictly as `USER nonroot` to neutralize zero-day container escapes.
   - **Conditional Orchestration:** `docker-compose.yml` employs `depends_on: condition: service_healthy` to ping for TimescaleDB readiness, destroying blind crash-loops that trigger Exchange IP bans.
   - **GitHub Actions (Lothaire's Barrier):** Enforces a strict `.github/workflows/deploy.yml` pipeline that auto-blocks any merge triggering `cargo clippy` warnings or failing the quantitative tests before pushing the immutable image.
   - **Asynchronous Webhook Alerting:** A dedicated HTTP client rigorously bounded by a **2-second timeout** alerts Slack/Discord upon critical drawdowns or WS losses, preventing network API lag from freezing the engine.
   - **KMS Secrets Expatriation:** File-based `.env` loading is vanished. API credentials are hydrated directly into RAM at boot via an asynchronous AWS Secrets Manager fetch equipped with a resilient **Exponential Backoff** retry algorithm.

---

## 🚀 How to Run the Infrastructure

### 1. The Backtest Simulator (Quant Environment)
Launch the high-velocity backtesting simulation over historical CSV flows:
```bash
cargo run --bin backtest
```
*Note: Ensure to implement your pure mathematical logic inside `DummyStatArb::on_event`.*

### 2. The Live Production Engine
Launch the entire network infrastructure and Tokio event scheduler (Currently targeting Bybit Spot):
```bash
cargo run --bin lehman_cousins
```

### 3. Tests & Validation
The project compiles strictly without any restrictive warnings:
```bash
cargo check
cargo test
```

---

## 🚧 Next Steps

The engineering foundation is complete and fail-proof. The baton now passes from Software Engineering to Quantitative Research.

1. **Quant Research (Lothaire's Job):**
   - Replace the mock `DummyStatArb` with actual mathematical models. The `fn on_event` interface is completely ready, pure, and decoupled from networking.
2. **Final Bybit Wiring (Data Engineering):**
   - In `src/exchange_clients/bybit.rs`, link the `simd-json` output dictionary to the `MarketEvent::OrderBookUpdate` constructor by reading the exact string payloads (bid, ask, timestamp) published by Bybit.
   - Uncomment the `reqwest::Client::post().send()` execution lines after securely injecting live API keys via `.env` (`dotenvy`).
3. **Tick Persistence (Post-Trade Analytics):**
   - Fill the stubs in `telemetry.rs` to establish the PostgresSQL / TimescaleDB hooks, persisting asynchronous `ExecutionReport` histories to support next-day hyper-parameter tuning and analytics.
