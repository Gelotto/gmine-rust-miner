# Build stage
FROM rust:1.89-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    protobuf-compiler \
    libprotobuf-dev \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /usr/src/gmine

# Copy everything - simple approach that works
COPY . .

# Build the miner in release mode
RUN cargo build --release --bin simple_miner

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 miner

# Create directories for logs and state
RUN mkdir -p /home/miner/.gmine && \
    chown -R miner:miner /home/miner/.gmine

# Copy binary from builder
COPY --from=builder /usr/src/gmine/target/release/simple_miner /usr/local/bin/gmine

# Switch to non-root user
USER miner
WORKDIR /home/miner

# Environment variables for configuration
ENV RUST_LOG=info

# Volume for persistent state
VOLUME ["/home/miner/.gmine"]

# Default command (can be overridden)
ENTRYPOINT ["gmine"]
CMD ["mine", "--use-rust-signer"]