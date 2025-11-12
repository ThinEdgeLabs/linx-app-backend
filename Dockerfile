FROM rust:1.86-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    build-essential \
    libpq-dev \
    git \
    curl \
    ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

# Create directory structure for workspace
RUN mkdir -p crates examples

COPY crates/ ./crates/
COPY examples/linx-indexer ./examples/linx-indexer

# Build binaries to cache dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp /app/target/release/bento /app/bento

# Create final lightweight image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libpq5 \
    ca-certificates \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries from workspace target directory
COPY --from=builder /app/bento /app/

# Default command will be overridden in docker-compose.yml
CMD ["./bento"]