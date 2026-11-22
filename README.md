# KnowMe OpenAI Proxy

A Rust service that connects OpenAI-compatible clients to **ChatGPT Plus and Pro subscriptions** or **OpenAI API keys**. The embedded KnowMe console provides chat and settings for the Codex and OpenCode SDK drivers.

The service provides these interfaces.

| Surface | Description |
|---------|-------------|
| **HTTP proxy** | `POST /v1/chat/completions` — OpenAI-compatible, works with any client |
| **OpenCode 2.x provider** | Native V2 plugin with a dedicated Codex SDK subscription route |
| **Embedded console** | Chat, settings, client provisioning and A2UI 1.0 at `/ui/` |
| **MCP server** | stdio + Streamable HTTP — for Claude Code, Codex CLI, Gemini CLI |
| **ACP server** | stdio for external agents in Zed and JetBrains |
| **AG-UI endpoint** | `POST /ag-ui/stream` streams assistant text in lifecycle events |
| **A2A agent** | `GET /.well-known/agent.json` — A2A-discoverable for multi-agent orchestration |

---

## Chat and settings

Open `http://127.0.0.1:8181/ui/` after starting the proxy on a loopback listener. Paired mode uses the short-lived access code from the private `pairing-token` file identified in the startup log. A private workstation can opt into automatic local browser sessions with `PROXY_CONSOLE_LOCAL_AUTO_SESSION=1`. Shared deployments use individual principal tokens. Owners can inspect credentials, request renewal, choose a chat driver and configure local clients. [Console deployment guide](docs/console.md)

The Codex and OpenCode SDK drivers run in scoped server-side workers. The browser receives ordered text and validated A2UI 1.0 activity snapshots. A tool approval, cancellation or surface action belongs to the authenticated conversation that created it.

On macOS, an existing KnowMe LaunchAgent can be updated with `python3 scripts/deploy-macos-workers.py --full`. The command builds the committed proxy and console, installs the locked runtime, and prints the path to a verified rollback receipt after the service passes its health check. [LaunchAgent deployment and rollback](docs/console.md#update-the-macos-launchagent)

## OpenCode 2.x plugin

The repository configuration loads the package in `plugin/` with the V2 `plugins` setting. Set `PROXY_CONSOLE_URL` to the service origin and provide a scoped service token through `PROXY_CONSOLE_TOKEN` or an owner-only `PROXY_CONSOLE_TOKEN_FILE`. The plugin gets its model inventory from the service and sends subscription requests to `/api/v1/drivers/codex/v1`. OpenCode host, plugin and native SDK 2.3.0 are the release baseline. [OpenCode setup](docs/opencode-setup.md)

## Documentation

Read the [documentation index](docs/index.md) or build the Docusaurus site from the same Markdown guides.

```sh
npm ci --prefix website
npm run build --prefix website
npm run start --prefix website
```

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

`GET /v1/models` lists the visible models from the active managed Codex account's native picker when the service can read it. The list follows that account's available models and can change after login. API-key routes and unavailable native discovery use the [built-in catalogue](src/model_catalog.rs). Context and output limits are returned when the proxy has metadata for the model.

The console lists models for the selected driver. Codex uses the authorized account's picker, and OpenCode uses its configured provider. The OpenCode V2 subscription plugin gets its list from the service's authenticated Codex driver endpoint. Use the returned model IDs for the account and path you intend to use. [OpenCode model discovery](docs/opencode-setup.md#credentials-and-models)

---

## Prerequisites

| Tool | Purpose | Install |
|------|---------|---------|
| Rust stable | Compile the proxy | [rustup](https://rustup.rs/) |
| Node 26.5.0 and npm 11.16.0 | Build the embedded console | [Frontend build guide](frontend/README.md) |
| Native Codex CLI 0.169.0 | Managed ChatGPT login and Codex SDK worker | [Console runtime setup](docs/console.md#install-the-native-drivers) |
| Bun 1.4.4 | Run the native workers and build the OpenCode plugin | [Console runtime setup](docs/console.md#install-the-native-drivers) |
| OpenCode 2.3.0 | Host the optional external OpenCode V2 plugin | [OpenCode setup](docs/opencode-setup.md) |
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
cargo build --release --locked
./target/release/openai-proxy serve --host 127.0.0.1 --port 8181
```

This starts the HTTP proxy and embedded settings. For SDK chat, configure the [native worker runtime](docs/console.md#install-the-native-drivers) and an approved workspace before starting the service.

### Docker

Build the default `sdk-runtime` image for both native SDK drivers, or select the `runtime` target for a smaller service with embedded browser assets. Supply credentials at runtime and mount the complete dedicated Codex home. [Container configuration](docker-compose.yaml) · [Build stages](Dockerfile)

## Development

```bash
# Development server with debug logging
RUST_LOG=openai_proxy=debug cargo run

# Full tower/axum HTTP traces
RUST_LOG=openai_proxy=debug,tower_http=debug cargo run

# Build the release binary
cargo build --release --locked

# Build with SurrealDB memory
cargo build --release --locked --features memory

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
bun install --frozen-lockfile
bun run typecheck      # TypeScript type check (no emit)
bun run build          # compile to dist/
```

---

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions (streaming + non-streaming) |
| `GET` | `/v1/models` | Available models for the active backend profile |
| `GET` | `/health` | `{"status":"ok"}` |
| `POST` | `/ag-ui/stream` | Five lifecycle event types for a Responses-backed text stream |
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
│   ├── acp.rs             # ACP stdio server
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
├── opencode.json          # OpenCode V2 plugin configuration
├── hooks.example.toml     # Webhook hooks config template
├── CLAUDE.md              # Guidance for Claude Code agents working in this repo
└── Cargo.toml
```

---

## License

MIT
