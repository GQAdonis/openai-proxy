# openai-proxy

A fast, extensible OpenAI-compatible proxy written in Rust that bridges any OpenAI Chat Completions client to **ChatGPT Plus/Pro subscriptions** and **OpenAI API keys** — with full agent support built in.

The service provides these interfaces.

| Surface | Description |
|---------|-------------|
| **HTTP proxy** | `POST /v1/chat/completions` — OpenAI-compatible, works with any client |
| **OpenCode 2.x provider** | Native V2 plugin with a dedicated Codex SDK subscription route |
| **Embedded console** | Chat, settings, client provisioning and A2UI 1.0 at `/ui/` |
| **MCP server** | stdio + Streamable HTTP — for Claude Code, Codex CLI, Gemini CLI |
| **ACP server** | stdio — for Zed and JetBrains via Agent Client Protocol v0.11 |
| **AG-UI endpoint** | `POST /ag-ui/stream` — structured SSE for CopilotKit and AG-UI frontends |
| **A2A agent** | `GET /.well-known/agent.json` — A2A-discoverable for multi-agent orchestration |

---

## Chat and settings

Start the proxy, open `http://localhost:8181/ui/`, and enter the short-lived pairing token from the private file named in the startup log. Pairing consumes the token and creates an HTTP-only session. Owners can inspect credential status, reload the native store, request managed refresh, choose a chat driver and configure local clients. See the [frontend build guide](frontend/README.md) and [console deployment guide](docs/console.md).

The Codex and OpenCode SDK drivers run in scoped server-side workers. The browser receives ordered text and validated A2UI 1.0 activity snapshots. A tool approval, cancellation or surface action belongs to the authenticated conversation that created it.

## OpenCode 2.x plugin

The repository configuration loads the package in `plugin/` with the V2 `plugins` setting. Set `PROXY_CONSOLE_URL` to the service origin and provide a scoped service token through `PROXY_CONSOLE_TOKEN`. The plugin gets its model inventory from the service and sends subscription requests to `/api/v1/drivers/codex/v1`. OpenCode host, plugin and native SDK 2.3.0 are the initial compatibility target. See [OpenCode setup](docs/opencode-setup.md).

## How the proxy works

```
opencode / any OpenAI client
    │
    │  POST /v1/chat/completions  (OpenAI Chat Completions format)
    ▼
┌─────────────────────────────────────────────────────┐
│                  openai-proxy (Rust)                │
│                                                     │
│  1. Resolve model alias → canonical model ID        │
│  2. Inject loaded SKILL.md context (optional)       │
│  3. Inject memory RAG context (--features memory)   │
│  4. Inject MCP tool schemas (optional)              │
│  5. Convert to backend wire format per profile      │
│  6. Fire lifecycle hooks (optional)                 │
│  7. Stream SSE back as OpenAI chat.completion.chunk │
└─────────────────────────────────────────────────────┘
    │
    ├─► chatgpt.com/backend-api/codex/responses  (ChatGPT subscription)
    ├─► api.openai.com/v1/responses              (OpenAI API key, Responses API)
    └─► api.openai.com/v1/chat/completions       (OpenAI API key, Chat Completions)
```

### Backend selection

Each request uses the credential generation, account and backend profile selected together by the identity provider.

| Credentials found | Backend selected |
|-------------------|-----------------|
| `~/.codex/auth.json` with `access_token` + `account_id` | `chatgpt.com/backend-api/codex/responses` (ChatGPT Plus/Pro) |
| `~/.codex/auth.json` with `api_key` or `OPENAI_API_KEY` | `api.openai.com/v1/responses` (default) |
| `OPENAI_API_KEY` + `CODEX_WIRE_API=chat` | `api.openai.com/v1/chat/completions` |

The three backends have different parameter allowlists. The proxy shapes each request correctly:

| Parameter | ChatGPT sub | OpenAI Responses | OpenAI Chat |
|-----------|:-----------:|:----------------:|:-----------:|
| `temperature` | omit | pass | pass |
| `max_output_tokens` | omit | pass | → `max_completion_tokens` |
| `tools` / `tool_choice` | pass | pass | pass |
| `stream` | force `true` | optional | optional |
| `store` | force `false` | optional | n/a |

---

## Model catalog

`GET /v1/models` reads the active managed Codex account's visible model picker. New model IDs appear as Codex offers them. The proxy uses the built-in catalogue for API-key routes and when it cannot read the Codex picker.

| Model ID | Context | Output | Notes |
|----------|--------:|-------:|-------|
| `gpt-5.5` | 1,000,000 | 32,768 | Default model |
| `gpt-5.5-pro` | 1,000,000 | 32,768 | API key only |
| `gpt-5.4` | 400,000 | 32,768 | |
| `gpt-5.4-mini` | 200,000 | 16,384 | `codex-mini` aliases here |
| `gpt-5.4-nano` | 128,000 | 8,192 | |
| `gpt-5.3-codex` | 400,000 | 32,768 | |
| `gpt-5.3-chat` | 128,000 | 16,384 | |
| `gpt-5.2-chat` | 128,000 | 16,384 | |

---

## Prerequisites

| Tool | Purpose | Install |
|------|---------|---------|
| Rust 1.87+ | compile the proxy | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Codex CLI | ChatGPT OAuth login | `npm i -g @openai/codex` |
| opencode (optional) | AI coding agent | `npm i -g opencode` |
| Bun (optional) | plugin local build | `curl -fsSL https://bun.sh/install \| bash` |
| Docker (optional) | containerised deployment | [docker.com](https://docker.com) |

A ChatGPT **Plus** or **Pro** subscription is required for the subscription path. An explicitly configured OpenAI API-key connection uses separate per-token billing.

---

## Quick start (without opencode)

### Rust binary

```bash
# Log in (ChatGPT subscription)
codex login

# Or set an API key for the standard billing path
export OPENAI_API_KEY=sk-...

# Install the locked web dependencies, then build and run
npm ci --prefix frontend --include=dev
cargo build --release
./target/release/openai-proxy serve
# Proxy is now at http://localhost:8181
```

### Docker

Build the default `sdk-runtime` image for both native SDK drivers, or select the `runtime` target for a smaller service with embedded browser assets. Supply credentials at runtime and mount the complete dedicated Codex home. [Container configuration](docker-compose.yaml) · [Build stages](Dockerfile)

## Development

```bash
# Development server with debug logging
RUST_LOG=openai_proxy=debug cargo run

# Full tower/axum HTTP traces
RUST_LOG=openai_proxy=debug,tower_http=debug cargo run

# Build release binary (~8MB stripped)
cargo build --release

# Build with SurrealDB memory
cargo build --release --features memory

# Run all integration tests (requires live credentials)
cargo test --test integration -- --nocapture

# Run a single test
cargo test --test integration non_streaming_max_tokens_respected -- --nocapture

# Unit tests only (no network)
cargo test --lib

# Verify health
curl http://localhost:8181/health
curl http://localhost:8181/v1/models
```

### Plugin development

```bash
cd plugin
bun install
bun run typecheck      # TypeScript type check (no emit)
bun run build          # compile to dist/
bun run dev            # run src/index.ts directly via bun
```

---

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions (streaming + non-streaming) |
| `GET` | `/v1/models` | Available models for the active backend profile |
| `GET` | `/health` | `{"status":"ok"}` |
| `POST` | `/ag-ui/stream` | AG-UI 5-event SSE stream |
| `GET` | `/.well-known/agent.json` | A2A Agent Card (opt-in with `--a2a`) |
| `POST` | `/v1/memory/documents` | Store a document (`--features memory`) |
| `GET` | `/v1/memory/documents` | List documents by scope |
| `DELETE` | `/v1/memory/documents/:id` | Delete a document |
| `GET` | `/v1/memory/search` | Semantic search |

---

## Authentication and troubleshooting

Running `codex login` updates the selected credential store. The proxy watches file replacements and periodically reconciles missed notifications. A managed refresh uses the native Codex owner. The connection panel distinguishes a store reload, confirmed renewal, sign-in required and uncertain exchange status. [Credential operation guide](docs/codex-auth.md)

A subscription denial or exhausted allowance ends that request. For an unavailable chat driver, check the versioned worker dependencies, native executable, workspace and source shown in settings. An OpenCode V2 client needs a service bearer scoped to its owner. [OpenCode setup](docs/opencode-setup.md)

## Project layout

```
openai-proxy/
├── src/
│   ├── main.rs            # CLI parsing, config loading, server startup
│   ├── lib.rs             # AppState, build_app() router, load_real_auth()
│   ├── codex.rs           # BackendProfile, auth, model catalogue, request converters
│   ├── proxy.rs           # Axum handlers, streaming paths, skill/memory injection
│   ├── openai.rs          # OpenAI wire types (request/response)
│   ├── models.rs          # GET /v1/models — profile-aware with context limits
│   ├── error.rs           # ProxyError → HTTP response
│   ├── config.rs          # ProxyConfig TOML loader, XDG paths, env merge
│   ├── cli/               # Clap subcommands: setup, skills, config
│   ├── skills.rs          # SKILL.md parser, SkillIndex (IDF-weighted), selector
│   ├── mcp.rs             # MCP JSON-RPC 2.0 server (stdio + HTTP)
│   ├── mcp_client.rs      # MCP tool schema loader for passthrough injection
│   ├── acp.rs             # ACP v0.11 stdio server
│   ├── agui.rs            # AG-UI 5-event SSE endpoint
│   ├── memory.rs          # SurrealDB RAG (feature-gated: --features memory)
│   ├── hooks.rs           # Webhook lifecycle hooks
│   └── a2a.rs             # A2A Agent Card endpoint
├── plugin/                # OpenCode V2 server plugin and delegation integration
├── workers/               # Pinned Codex and OpenCode SDK adapters
├── frontend/              # React 19/Vite 8 chat, settings and validated surfaces
├── build.rs               # Cargo build and embedded asset manifest
├── docs/
│   ├── config.md          # Full configuration reference
│   ├── opencode-setup.md  # opencode integration guide
│   ├── memory.md          # SurrealDB memory system reference
│   ├── ag-ui.md           # AG-UI endpoint reference
│   ├── acp.md             # ACP stdio reference (Zed / JetBrains)
│   ├── a2a.md             # A2A Agent Card reference
│   └── hooks.md           # Webhook hooks reference
├── Dockerfile             # Multi-stage build (builder + debian-slim runtime)
├── docker-compose.yaml    # Compose with auth mount + data volume
├── SKILL.md               # agentskills.io skill (setup + MCP sub-skills)
├── opencode.json          # Static opencode provider config (Level 2 integration)
├── hooks.example.toml     # Webhook hooks config template
├── CLAUDE.md              # Guidance for Claude Code agents working in this repo
└── Cargo.toml
```

---

## License

MIT
