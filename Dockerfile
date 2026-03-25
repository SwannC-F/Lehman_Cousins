# Stage 1: Builder
# Uses slim Debian Bookworm standard Rust image
FROM rust:1.75-slim-bookworm AS builder

# Ensure required C compilation dependencies for standard packages and musl/static libs if needed
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler curl

WORKDIR /app
COPY . .

# Build the release binary. We prioritize heavy LLVM optimization (opt-level=3, LTO=fat in Cargo.toml).
# We exclusively target the main production application "lehman_cousins".
RUN cargo build --release --bin lehman_cousins

# Stage 2: Distroless Runner
# No Shell. No Package Manager. Minimal attack surface.
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# The user is strictly forced to nonroot implicitly provided by Google's Distroless
USER nonroot

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/lehman_cousins /app/lehman_cousins

# Port exposition for Prometheus metrics scrape
EXPOSE 9000

ENTRYPOINT ["/app/lehman_cousins"]
