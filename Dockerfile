# =============================================================================
# Dakera MCP Server — Two-stage Docker build
# =============================================================================
# Lightweight stdio-based MCP binary. No RocksDB, no embedding models.
#
# The binary is linked statically against musl and uses rustls (no OpenSSL),
# so the runtime image is distroless/static: no shell, no package manager and
# no OS libraries — nothing for container vulnerability scanners to flag
# beyond the CA bundle.
#
# Build:
#   docker build -t dakera-mcp:latest .
#
# Run (stdio mode for MCP clients):
#   docker run -i --rm \
#     -e DAKERA_API_URL=http://host.docker.internal:3000 \
#     -e DAKERA_API_KEY=your-key \
#     dakera-mcp:latest
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Builder (Alpine = native musl target on both amd64 and arm64)
# ---------------------------------------------------------------------------
# Base images are pinned by the digest of their multi-arch index (amd64 + arm64);
# Dependabot (docker ecosystem) or a manual `docker buildx imagetools inspect
# <image:tag>` refreshes them.
FROM rust:1.95.0-alpine@sha256:606fd313a0f49743ee2a7bd49a0914bab7deedb12791f3a846a34a4711db7ed2 AS builder

# musl-dev: C runtime/headers for crates with C code (ring)
RUN apk add --no-cache musl-dev

# Override release profile for faster Docker builds
ENV CARGO_PROFILE_RELEASE_LTO=false
ENV CARGO_PROFILE_RELEASE_OPT_LEVEL=2
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16

WORKDIR /app

# Copy manifests first for dependency layer caching
COPY Cargo.toml ./
COPY Cargo.lock* ./

# Create stub main + lib so cargo can fetch and compile dependencies
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs

# Compile dependencies (cached until Cargo.toml/lock change)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked 2>&1 || true

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
    cargo build --release --locked --bin dakera-mcp && \
    cp /app/target/release/dakera-mcp /usr/local/bin/dakera-mcp

# ---------------------------------------------------------------------------
# Stage 2: Runtime — distroless static, runs as non-root (uid 65532)
# ---------------------------------------------------------------------------
FROM gcr.io/distroless/static-debian13:nonroot@sha256:d62067879de890a1def9da0e7766b7d20810a5e5555cef8506b752437d7a3e88

# Required annotation for MCP registry OCI package validation
LABEL io.modelcontextprotocol.server.name="io.github.Dakera-AI/dakera-mcp"

# Root-owned and not writable by the runtime user.
COPY --from=builder --chown=0:0 --chmod=0555 /usr/local/bin/dakera-mcp /usr/local/bin/dakera-mcp

# The distroless `nonroot` user, by number so that Kubernetes `runAsNonRoot`
# can verify it without resolving a name.
USER 65532:65532

# A stdio server has no port or endpoint to probe; the MCP client that spawned
# it sees it exit. Declared so that no inherited check applies.
HEALTHCHECK NONE

# stdio-based protocol — no ports to expose
ENTRYPOINT ["/usr/local/bin/dakera-mcp"]
