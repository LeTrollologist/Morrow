# ==============================================================================
# Tungsten Genesis (v1.8) — Self-Hosted Production Multi-Stage Dockerfile
# Multi-Stage Build:
#   Stage 1: Pure Self-Hosted Tungsten Toolchain & Application Compilation
#   Stage 2: Minimal Production Runtime Image (< 50 MB, Non-Root)
# ==============================================================================

# --- Stage 1: Build Environment ---
FROM debian:bookworm-slim AS builder

# Install LLVM toolchain, Clang, LLD, and build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    lld \
    gcc \
    libc6-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Copy workspace sources and pre-bootstrapped self-hosted Tungsten compiler
COPY bin/tgc_linux ./bin/tgc_linux
COPY compiler ./compiler
COPY std ./std
COPY examples ./examples

# Grant execution permissions to the self-hosted Tungsten compiler
RUN chmod +x bin/tgc_linux

# Compile microservice into native Linux ELF binary
RUN ./bin/tgc_linux examples/bootstrap_sample.tg --target x86_64-unknown-linux-gnu -o target/app_service

# Verify output binary exists and has execution permissions
RUN chmod +x target/app_service

# --- Stage 2: Production Minimal Runtime ---
FROM debian:bookworm-slim AS runtime

# Install minimal runtime dependencies (glibc, ca-certificates, curl for healthchecks)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 10001 -U -s /bin/false -m tungsten

WORKDIR /app

# Copy the standalone compiled Tungsten ELF binary from builder stage
COPY --from=builder --chown=tungsten:tungsten /workspace/target/app_service /app/app_service

# Drop root privileges for defense-in-depth container security
USER tungsten:tungsten

# Expose service port
EXPOSE 8096

# Launch compiled Tungsten native ELF application
ENTRYPOINT ["/app/app_service"]
