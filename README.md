# Lehman_Cousins : Automated Statistical Arbitrage Engine

![Rust](https://img.shields.io/badge/Rust-1.86%2B-orange?style=flat-square&logo=rust)
![PostgreSQL](https://img.shields.io/badge/TimescaleDB-16-blue?style=flat-square&logo=postgresql)
![Docker](https://img.shields.io/badge/Docker-Ready-2496ED?style=flat-square&logo=docker)

## Overview
**Lehman_Cousins** is a proprietary algorithmic trading infrastructure designed to capture alpha through statistical arbitrage and market-making strategies on fragmented cryptocurrency markets (Bybit, Gate.io).

Engineered in strict **Rust (Edition 2021)**, the system is built with a deeptech "Quant Engineering" architecture, bypassing standard web stacks to focus on deterministic latency, zero-copy parsing, and absolute execution safety in high-volatility environments.

## Core Architecture & Deeptech Stack

### 1. Ingestion & Storage
- **TimescaleDB for Ticks**: Replaced standard PostgreSQL for time-series ingestion. Utilises hypertables for O(1) active-chunk insertion and Continuous Aggregates for native OHLCV views. Columnar compression enabled for massive storage reduction.
- **Zero-Copy JSON Parsing**: Integrates `simd-json` on the WebSocket hot-paths. Parses incoming network frames in-place by mutating the network buffer, eliminating heap allocations for JSON field strings.

### 2. Low-Latency Data Structures
- **Arena-Allocated Order Book**: Discarded `BTreeMap` in favor of a flat, pre-allocated `Vec`-based slab. Price levels are maintained via `partition_point` (binary search), ensuring raw cache locality, `O(1)` best bid/ask lookups, and zero heap allocations during market bursts.

### 3. Execution Safety & Concurrency
- **Strict State Machine Feed**: Lock-free synchronization engine (`FeedState::Pending → Buffering → Syncing → Live`). Features strict sequence gap detection, bounded buffer overflows (anti-OOM), and generation-counter monotonic guards against REST snapshot race conditions.
- **Order Manager Gatekeeper**:
  - *Token Bucket Rate Limiter*: Enforces exchange REST API rate limits precisely (`Mutex<f64>` fast path) to prevent IP bans during algorithmic bursts.
  - *Atomic Nonce Generator*: Uses `std::sync::atomic::AtomicU64` to guarantee strictly increasing, collision-free cryptographic signatures for orders, even under multi-threaded asynchronous concurrency.

## Project Scope & Roles
* **Lothaire (Quantitative Research):** Alpha generation, signal processing, quantitative modeling, and historical backtesting.
* **Swann (Lead Architect & Quant Dev):** Rust system design, low-latency data structures, market connectivity, and execution security.

## Deployment
The infrastructure is containerised for deployment on collocated AWS EC2 instances, managed via Docker Compose.

```bash
docker-compose up -d  # Boots TimescaleDB and the core engine
```
