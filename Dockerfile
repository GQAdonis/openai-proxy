# ── Stage 1: Build ───────────────────────────────────────────────────────────
FROM rust:1.87-slim AS builder

# System deps for reqwest (rustls, no OpenSSL needed)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies before copying source
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs
RUN cargo build --release --locked 2>/dev/null || true
RUN rm -rf src

# Build the real binary
COPY src ./src
# Touch main.rs so cargo rebuilds (cache-buster after stub above)
RUN touch src/main.rs src/lib.rs
RUN cargo build --release --locked

# ── Stage 2: Runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# CA certificates for TLS to OpenAI / chatgpt.com; curl for HEALTHCHECK
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/sh oproxy

WORKDIR /home/oproxy

COPY --from=builder /build/target/release/openai-proxy /usr/local/bin/openai-proxy

# XDG config/data directories the proxy writes to
RUN mkdir -p /home/oproxy/.config/oproxy /home/oproxy/.local/share/oproxy \
    && chown -R oproxy:oproxy /home/oproxy

USER oproxy

ENV HOST=0.0.0.0
ENV PORT=8080
ENV RUST_LOG=openai_proxy=info

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["openai-proxy"]
CMD ["serve"]
