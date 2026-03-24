# =============================================================================
# Lehman_Cousins — Multi-stage Dockerfile
# Stage 1 : builder  — compile the release binary with full Rust toolchain
# Stage 2 : runtime  — minimal Debian slim image, no compiler, no sources
# =============================================================================

# ── Stage 1 : Builder ─────────────────────────────────────────────────────────
FROM rust:1.78-slim-bookworm AS builder

WORKDIR /usr/src/lehman_cousins

# Install system dependencies required by crates (SSL, pkg-config for sqlx, etc.)
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config           \
        libssl-dev           \
        libpq-dev            \
    && rm -rf /var/lib/apt/lists/*

# ── Dependency caching layer ──────────────────────────────────────────────────
# Copy manifests first so Docker cache is reused when only src changes.
COPY Cargo.toml Cargo.lock ./

# Create a dummy main so `cargo build` can resolve the full dependency tree.
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs && \
    echo '' > src/lib.rs && \
    cargo build --release --locked && \
    rm -rf src

# ── Full build ────────────────────────────────────────────────────────────────
COPY src ./src
COPY migrations ./migrations

# Touch main.rs to force recompile of the application binary.
RUN touch src/main.rs && \
    cargo build --release --locked

# =============================================================================
# ── Stage 2 : Runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Metadata labels
LABEL org.opencontainers.image.title="lehman_cousins"
LABEL org.opencontainers.image.description="Algorithmic trading engine — StatArb & Market Making"
LABEL org.opencontainers.image.version="0.1.0"

# Install minimal runtime libraries (SSL, CA certs, libpq for sqlx)
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates  \
        libssl3          \
        libpq5           \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for security
RUN useradd --uid 10001 --no-create-home --shell /sbin/nologin trader
USER trader

WORKDIR /app

# Copy only the compiled binary from builder
COPY --from=builder /usr/src/lehman_cousins/target/release/lehman_cousins ./lehman_cousins

# Prometheus metrics port
EXPOSE 9090

# Health-check — verifies the process is alive
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD pgrep lehman_cousins || exit 1

ENTRYPOINT ["./lehman_cousins"]
