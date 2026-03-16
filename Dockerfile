# =============================================================================
# Dakera MCP Server — Two-stage Docker build
# =============================================================================
# Lightweight stdio-based MCP binary. No RocksDB, no embedding models.
#
# Build:
#   docker compose --profile mcp build dakera-mcp
#   # or standalone:
#   docker build -t dakera-mcp:latest -f crates/mcp/Dockerfile .
#
# Run (stdio mode for MCP clients):
#   docker run -i --rm \
#     -e DAKERA_API_URL=http://host.docker.internal:3000 \
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

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/mcp/Cargo.toml crates/mcp/Cargo.toml
COPY crates/common/Cargo.toml crates/common/Cargo.toml
COPY crates/storage/Cargo.toml crates/storage/Cargo.toml
COPY crates/engine/Cargo.toml crates/engine/Cargo.toml
COPY crates/inference/Cargo.toml crates/inference/Cargo.toml
COPY crates/api/Cargo.toml crates/api/Cargo.toml
COPY crates/client/Cargo.toml crates/client/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml
COPY crates/dashboard/Cargo.toml crates/dashboard/Cargo.toml

# Create stub files so cargo can resolve the workspace
RUN mkdir -p crates/mcp/src && echo 'fn main() {}' > crates/mcp/src/main.rs && \
    mkdir -p crates/common/src && echo '' > crates/common/src/lib.rs && \
    mkdir -p crates/storage/src && echo '' > crates/storage/src/lib.rs && \
    mkdir -p crates/engine/src && echo '' > crates/engine/src/lib.rs && \
    mkdir -p crates/inference/src && echo '' > crates/inference/src/lib.rs && \
    mkdir -p crates/api/src && echo 'fn main() {}' > crates/api/src/main.rs && \
    mkdir -p crates/client/src && echo '' > crates/client/src/lib.rs && \
    mkdir -p crates/cli/src && echo 'fn main() {}' > crates/cli/src/main.rs && \
    mkdir -p crates/dashboard/src && echo '' > crates/dashboard/src/lib.rs

# Compile dependencies with stubs (cached until Cargo.toml/lock change)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    CARGO_BUILD_JOBS=2 cargo build --release --bin dakera-mcp 2>&1 || true

# ---------------------------------------------------------------------------
# Layer 2: Real source compilation (only mcp + common recompile)
# ---------------------------------------------------------------------------

# Copy real source
COPY crates/common/ crates/common/
COPY crates/mcp/ crates/mcp/

# Touch source files to ensure cargo detects them as newer than stubs
RUN find crates/mcp crates/common -name "*.rs" -exec touch {} +

# Build release binary with real source
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    CARGO_BUILD_JOBS=2 cargo build --release --bin dakera-mcp && \
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
