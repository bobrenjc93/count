# Build stage
FROM rust:1.75-slim as builder

WORKDIR /usr/src/app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy source code
COPY src ./src
COPY benches ./benches
COPY examples ./examples
COPY tests ./tests

# Build the actual application
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -s /bin/false count

# Create data directory
RUN mkdir -p /data && chown count:count /data

# Copy the binaries
COPY --from=builder /usr/src/app/target/release/count-cli /usr/local/bin/count-cli
COPY --from=builder /usr/src/app/target/release/count-server /usr/local/bin/count-server

# Set the user
USER count

# Set the working directory
WORKDIR /data

# Expose port for cluster communication
EXPOSE 8080

# Run the server by default
CMD ["count-server"]