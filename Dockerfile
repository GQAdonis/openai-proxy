FROM node:26.5.0-bookworm-slim AS console_deps
RUN npm install --global npm@11.16.0
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --include=dev --no-audit --no-fund

FROM rust:1.95-bookworm AS builder
COPY --from=console_deps /usr/local/bin/node /usr/local/bin/node
COPY --from=console_deps /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY frontend ./frontend
COPY --from=console_deps /build/frontend/node_modules ./frontend/node_modules
COPY src ./src
COPY data ./data
COPY plugin ./plugin
COPY workers ./workers
RUN cargo build --release --locked

FROM oven/bun:1.4.4 AS sdk_deps
WORKDIR /opt/proxy/workers
COPY workers/package.json workers/bun.lock ./
RUN bun install --frozen-lockfile --production
COPY workers/ ./
WORKDIR /opt/proxy/plugin
COPY plugin/package.json plugin/bun.lock ./
RUN bun install --frozen-lockfile --production
COPY plugin/src/ ./src/
# The SDK lock resolves a platform-specific Codex CLI package.
RUN set -eu; cli="$(find /opt/proxy/workers/node_modules -type f -path '*/vendor/*' -name codex -perm /111 | head -1)"; \
    test -n "$cli"; ln -s "$cli" /opt/proxy/codex; chmod 755 /opt/proxy/workers/codex-exec

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
    && install -d -m 0700 -o oproxy -g oproxy /home/oproxy/.codex /home/oproxy/workspace /home/oproxy/.local/share/oproxy/console \
    && chown -R oproxy:oproxy /home/oproxy

USER oproxy

ENV CODEX_HOME=/home/oproxy/.codex
ENV PROXY_CONSOLE_WORKSPACE=/home/oproxy/workspace
ENV HOST=0.0.0.0
ENV PORT=8181
ENV RUST_LOG=openai_proxy=info

EXPOSE 8181

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8181/health || exit 1

ENTRYPOINT ["openai-proxy"]
CMD ["serve"]


# Full image. The runtime target above provides the embedded web/API and stdio service alone.
FROM runtime AS sdk-runtime
COPY --from=sdk_deps /usr/local/bin/bun /usr/local/bin/bun
COPY --from=sdk_deps /opt/proxy /opt/proxy
ENV PROXY_SDK_BUN=/usr/local/bin/bun \
    PROXY_SDK_WORKER=/opt/proxy/workers/main.mjs \
    PROXY_CODEX_BINARY=/opt/proxy/codex
