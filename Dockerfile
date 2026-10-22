FROM rust:1.99.1-slim-bookworm AS builder

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
RUN mkdir -p crates

COPY crates/ ./crates/

# Build binaries to cache dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp /app/target/release/cli /app/cli

# Create final lightweight image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libpq5 \
    ca-certificates \
    fontconfig \
    fonts-liberation \
    && fc-cache -f \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries from workspace target directory
COPY --from=builder /app/cli /app/

# Share image backgrounds, read from assets/ relative to the working directory
COPY assets/ /app/assets/

# Default command will be overridden in docker-compose.yml
CMD ["./cli"]