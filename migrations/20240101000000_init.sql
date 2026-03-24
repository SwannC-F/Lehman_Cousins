-- =============================================================================
-- Migration 0001: TimescaleDB hypertable for tick ingestion
-- =============================================================================
-- WHY TimescaleDB instead of plain PostgreSQL:
--   • Appends are routed to the active chunk → no global index contention
--   • Columnar compression (enable_columnstore) gives 10-20× storage reduction
--   • Parallel chunk scans for time-range queries (no full table scan)
--   • Native continuous aggregates replace manual OHLCV roll-ups
--
-- REQUIREMENT: TimescaleDB extension must be installed before running.
--   AWS RDS: enable "timescaledb" in the parameter group + reboot.
--   Docker:  use image timescale/timescaledb:latest-pg16
--   Manual:  CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;
-- =============================================================================

-- Enable the extension (idempotent)
CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;

-- ---------------------------------------------------------------------------
-- 1. Base tick table
--    Use NUMERIC(28,10) for decimal-safe price/qty (matches rust_decimal).
--    Do NOT add a global B-tree index on (symbol, received_at) — TimescaleDB
--    chunks already partition by time; a local index per chunk is far cheaper.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ticks (
    received_at  TIMESTAMPTZ     NOT NULL,
    symbol       TEXT            NOT NULL,
    price        NUMERIC(28, 10) NOT NULL,
    quantity     NUMERIC(28, 10) NOT NULL,
    side         TEXT            NOT NULL CHECK (side IN ('BUY', 'SELL')),
    exchange     TEXT            NOT NULL
    -- No UUID primary key: time-series workloads never look up by PK.
    -- If you need deduplication, add a UNIQUE on (exchange, received_at, symbol, side).
);

-- ---------------------------------------------------------------------------
-- 2. Convert to hypertable partitioned by time (1-hour chunks)
--    chunk_time_interval: tune based on expected ingestion rate.
--    At 10k ticks/s, 1h ≈ 36M rows/chunk — adjust to '10 minutes' if needed.
-- ---------------------------------------------------------------------------
SELECT create_hypertable(
    'ticks',
    'received_at',
    chunk_time_interval => INTERVAL '1 hour',
    if_not_exists       => TRUE
);

-- ---------------------------------------------------------------------------
-- 3. Space partitioning by symbol (optional, uncomment for > 5 symbols)
--    Distributes chunks across tablespaces to parallelise disk I/O.
-- ---------------------------------------------------------------------------
-- SELECT add_dimension('ticks', 'symbol', number_partitions => 8);

-- ---------------------------------------------------------------------------
-- 4. Local chunk index — TimescaleDB creates one per chunk automatically
--    when you define it on the hypertable. Much cheaper than a global index.
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_ticks_symbol ON ticks (symbol, received_at DESC);

-- ---------------------------------------------------------------------------
-- 5. Continuous aggregate: 1-minute OHLCV candles (materialised, refreshed async)
--    Strategies can query tick_ohlcv_1m without touching the raw table.
-- ---------------------------------------------------------------------------
CREATE MATERIALIZED VIEW IF NOT EXISTS tick_ohlcv_1m
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('1 minute', received_at) AS bucket,
    symbol,
    exchange,
    first(price, received_at)            AS open,
    max(price)                           AS high,
    min(price)                           AS low,
    last(price, received_at)             AS close,
    sum(quantity)                        AS volume
FROM ticks
GROUP BY bucket, symbol, exchange
WITH NO DATA;

-- Refresh policy: keep the last 24 h of candles materialised and up-to-date.
SELECT add_continuous_aggregate_policy(
    'tick_ohlcv_1m',
    start_offset => INTERVAL '24 hours',
    end_offset   => INTERVAL '1 minute',
    schedule_interval => INTERVAL '1 minute',
    if_not_exists => TRUE
);

-- ---------------------------------------------------------------------------
-- 6. Compression policy (enable after data exists, or set start_after to future)
--    Compresses chunks older than 1 hour — columnar storage, 10-20× smaller.
-- ---------------------------------------------------------------------------
ALTER TABLE ticks SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'symbol, exchange',
    timescaledb.compress_orderby   = 'received_at DESC'
);

SELECT add_compression_policy(
    'ticks',
    compress_after => INTERVAL '1 hour',
    if_not_exists  => TRUE
);
