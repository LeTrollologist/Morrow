# ==============================================================================
# Tungsten Fortress v2 — High-Concurrency Microservice Production Dockerfile
# Multi-Stage Build:
#   Stage 1: Build Tungsten Toolchain (Forge) & Compile Fortress v2 Service
#   Stage 2: Minimal Production Runtime Image (< 80 MB, Non-Root)
# ==============================================================================

# --- Stage 1: Build Environment ---
FROM rust:1.80-slim-bookworm AS builder

# Install LLVM toolchain, Clang, LLD, and build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    lld \
    gcc \
    libc6-dev \
    libsqlite3-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Copy workspace manifests and source code
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY std ./std
COPY examples ./examples

# Build forge CLI compiler in release mode
RUN cargo build --release -p forge

# Compile Fortress v2 HTTP server into native Linux ELF binary
RUN ./target/release/forge build --release --target x86_64-unknown-linux-gnu examples/web_service_v2.tg

# Verify output binary exists and has execution permissions
RUN chmod +x target/release/web_service_v2

# --- Stage 2: Production Minimal Runtime ---
FROM debian:bookworm-slim AS runtime

# Install minimal runtime dependencies (glibc, ca-certificates, curl for healthchecks)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 10001 -U -s /bin/false -m tungsten

WORKDIR /app

# Copy the standalone compiled Tungsten ELF microservice binary
COPY --from=builder --chown=tungsten:tungsten /workspace/target/release/web_service_v2 /app/web_service_v2

# Drop root privileges for defense-in-depth container security
USER tungsten:tungsten

# Expose Fortress HTTP port
EXPOSE 8096

# Healthcheck probe hitting Fortress zero-copy /health endpoint
HEALTHCHECK --interval=5s --timeout=2s --start-period=3s --retries=3 \
    CMD curl -f http://127.0.0.1:8096/health || exit 1

# Launch Fortress v2 High-Concurrency HTTP Engine
ENTRYPOINT ["/app/web_service_v2"]
