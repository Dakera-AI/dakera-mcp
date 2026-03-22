# =============================================================================
# Dakera MCP Server — Two-stage Docker build
# =============================================================================
# Lightweight stdio-based MCP binary. No RocksDB, no embedding models.
#
# Build:
#   docker build -t dakera-mcp:latest .
#
# Run (stdio mode for MCP clients):
#   docker run -i --rm \
#     -e DAKERA_API_URL=http://host.docker.internal:3300 \
#     -e DAKERA_API_KEY=your-key \
#     dakera-mcp:latest
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Builder
# ---------------------------------------------------------------------------
FROM rust:1.92-bookworm AS builder

RUN apt-get update && apt-get install -y \
    libssl-dev \
    pkg-config \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Set SSL cert environment variables for cargo
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs
ENV CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt
ENV OPENSSL_NO_VENDOR=1

# Override release profile for faster Docker builds
ENV CARGO_PROFILE_RELEASE_LTO=false
ENV CARGO_PROFILE_RELEASE_OPT_LEVEL=2
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16

WORKDIR /app

# Copy manifests first for dependency layer caching
COPY Cargo.toml ./
COPY Cargo.lock* ./

# Create stub main so cargo can fetch and compile dependencies
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs

# Compile dependencies (cached until Cargo.toml/lock change)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release 2>&1 || true

# ---------------------------------------------------------------------------
# Layer 2: Real source compilation
# ---------------------------------------------------------------------------

# Copy real source
COPY src/ src/

# Touch source files to ensure cargo detects them as newer than stubs
RUN find src -name "*.rs" -exec touch {} +

# Build release binary with real source
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin dakera-mcp && \
    cp /app/target/release/dakera-mcp /usr/local/bin/dakera-mcp

# ---------------------------------------------------------------------------
# Stage 2: Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd --create-home --shell /bin/false dakera
USER dakera

COPY --from=builder /usr/local/bin/dakera-mcp /usr/local/bin/dakera-mcp

# stdio-based protocol — no ports to expose
ENTRYPOINT ["dakera-mcp"]
