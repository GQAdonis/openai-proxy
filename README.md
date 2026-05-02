# openai-proxy

A fast, extensible OpenAI-compatible proxy written in Rust that bridges any OpenAI Chat Completions client to **ChatGPT Plus/Pro subscriptions** and **OpenAI API keys** — with full agent support built in.

The proxy surfaces as six delivery mechanisms depending on how you want to use it:

| Surface | Description |
|---------|-------------|
| **HTTP proxy** | `POST /v1/chat/completions` — OpenAI-compatible, works with any client |
| **opencode provider** | Native plugin + static `opencode.json` for the [opencode](https://opencode.ai) AI coding agent |
| **MCP server** | stdio + Streamable HTTP — for Claude Code, Codex CLI, Gemini CLI |
| **ACP server** | stdio — for Zed and JetBrains via Agent Client Protocol v0.11 |
| **AG-UI endpoint** | `POST /ag-ui/stream` — structured SSE for CopilotKit and AG-UI frontends |
| **A2A agent** | `GET /.well-known/agent.json` — A2A-discoverable for multi-agent orchestration |

---

## How it works

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
│  3. Inject memory RAG context (optional, --features memory) │
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

Authentication source determines the backend automatically at startup:

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

### Model mapping

| Input alias | Canonical model | ChatGPT sub | API key |
|-------------|-----------------|:-----------:|:-------:|
| `gpt-5.5` | `gpt-5.5` | ✓ 400K ctx | ✓ 1M ctx |
| `gpt-5.5-pro` | `gpt-5.5-pro` | ✗ | ✓ 1M ctx |
| `gpt-5.4` | `gpt-5.4` | ✓ | ✓ |
| `gpt-5.3-codex`, `gpt-4o`, `gpt-4`, `gpt-3.5-turbo` | `gpt-5.3-codex` | ✓ | ✓ |
| `codex-mini`, `gpt-4o-mini` | `codex-mini` | ✗ | ✓ |
| anything else | `gpt-5.3-codex` | ✓ | ✓ |

---

## Prerequisites

| Tool | Purpose | Install |
|------|---------|---------|
| Rust 1.87+ | compile the proxy | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Codex CLI | ChatGPT OAuth login | `npm i -g @openai/codex` |
| opencode (optional) | AI coding agent | `npm i -g opencode` |
| Bun (optional) | opencode plugin build | `curl -fsSL https://bun.sh/install \| bash` |
| Docker (optional) | containerised deployment | [docker.com](https://docker.com) |

A ChatGPT **Plus** or **Pro** subscription is required for the subscription path. A standard OpenAI API key works as a fallback with per-token billing.

---

## Quick start

### Rust binary

```bash
# 1. Log in (ChatGPT subscription) or set OPENAI_API_KEY for API key mode
codex login

# 2. Build and run
cargo build --release
./target/release/openai-proxy serve

# Proxy is now at http://localhost:8080
```

### Docker

```bash
# Pull credentials into the container via volume mount
docker run --rm \
  -v ~/.codex/auth.json:/run/secrets/auth.json:ro \
  -e CODEX_AUTH_PATH=/run/secrets/auth.json \
  -p 8080:8080 \
  openai-proxy:latest

# Or with docker-compose (see docker-compose.yaml)
docker compose up
```

### opencode auto-configure

```bash
# Build the proxy, then generate a correct opencode.json for your credentials
openai-proxy setup opencode --global --port 8080

# Open opencode — the "openai-proxy" provider appears immediately
opencode
```

---

## Configuration

### Config file

The proxy uses an XDG-standard TOML config file. Generate the default:

```bash
openai-proxy setup config
```

Location: `~/.config/oproxy/config.toml`

```toml
[server]
host = "0.0.0.0"    # Env: HOST
port = 8080         # Env: PORT

[backend]
wire_api = "responses"   # "responses" (default) or "chat". Env: CODEX_WIRE_API

[skills]
dirs = []            # Colon-separated dirs of SKILL.md files. Env: PROXY_SKILLS_DIRS
max_injected = 3     # Max skills to inject per request. Env: PROXY_SKILLS_MAX

[mcp]
# config_path = "~/.config/oproxy/mcp.toml"   # MCP tool schema passthrough

[hooks]
# config_path = "~/.config/oproxy/hooks.toml"  # Webhook lifecycle events

[memory]
enabled = false           # Requires: cargo build --features memory
db_path = ""              # Default: ~/.local/share/oproxy/memory.db
embedding_model = "text-embedding-3-small"

[modes]
a2a = false   # Enable A2A Agent Card at /.well-known/agent.json
```

Full reference: [docs/config.md](docs/config.md)

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8080` | Listen port |
| `CODEX_AUTH_PATH` | `~/.codex/auth.json` | Auth file path override |
| `CODEX_BACKEND_URL` | auto | Override upstream URL |
| `CODEX_WIRE_API` | `responses` | `responses` or `chat` |
| `CODEX_DEFAULT_MODEL` | — | Default model for generic aliases |
| `OPENAI_API_KEY` | — | API key (fallback if no auth.json) |
| `MCP_HTTP_PORT` | — | Start MCP Streamable HTTP server on this port |
| `PROXY_SKILLS_DIRS` | — | Colon-separated skill directories |
| `PROXY_SKILLS_MAX` | `3` | Max skills injected per request |
| `PROXY_HOOKS_CONFIG` | — | Path to hooks.toml |
| `OPROXY_CONFIG` | — | Config file path override |
| `RUST_LOG` | — | Log filter (e.g. `openai_proxy=debug`) |

Copy `.env.example` to `.env` — the proxy loads it automatically via `dotenvy`.

### CLI subcommands

```
openai-proxy serve [OPTIONS]            # Start the HTTP proxy (default when no subcommand)
  --host --port --wire-api --a2a        # Server options
  --mcp-stdio                           # Run as MCP stdio server instead
  --acp-stdio                           # Run as ACP stdio server instead
  --mcp-http-port <PORT>                # Also start MCP HTTP server on this port

openai-proxy setup opencode [--global|--project] [--port N]
                                        # Write opencode.json for current credentials

openai-proxy setup mcp [--opencode|--claude] [--port N]
                                        # Write MCP config entry for opencode or Claude Code

openai-proxy setup config               # Scaffold ~/.config/oproxy/config.toml with defaults

openai-proxy skills list [--dirs ...]   # List all loaded SKILL.md files in tabular form
openai-proxy skills validate <dir>      # Validate SKILL.md frontmatter in a directory
openai-proxy skills test <msg>          # Show which skills would be selected for <msg>

openai-proxy config show                # Print the resolved config (file + env merged)
openai-proxy config path                # Print the XDG config file path and existence
```

---

## opencode Integration

Three levels are available. The `setup opencode` command generates the correct config automatically.

### Level 1 — Native plugin (recommended)

The `plugin/` directory is a TypeScript opencode plugin that hooks into four opencode lifecycle points:

| Hook | Effect |
|------|--------|
| `config` | Injects the provider and all models into opencode's live config — no manual edits needed |
| `auth` | Registers `"openai-proxy"` in `/connect` with OAuth (`codex login`) or API key options |
| `shell.env` | Injects `CODEX_AUTH_PATH` into every shell the agent spawns |
| `event` | On `session.created`, pings `/health` and emits a TUI warning if the proxy is not running |

```bash
cd plugin && bun install   # or npm install
```

Load globally in `~/.config/opencode/opencode.json`:
```json
{ "plugin": ["file:/absolute/path/to/openai-proxy/plugin"] }
```

Or project-locally — the `opencode.json` at the repo root already declares it:
```json
{ "plugin": ["file:./plugin"] }
```

### Level 2 — Static config

The `opencode.json` at the repo root declares the `codex` provider via `@ai-sdk/openai-compatible`. Drop it into any project directory and opencode picks it up. The `apiKey` field is required by the SDK but ignored by the proxy.

### Level 3 — Any OpenAI-compatible client

```bash
# curl
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer anything" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-5.5","messages":[{"role":"user","content":"Hello"}],"stream":true}'

# Python openai SDK
from openai import OpenAI
client = OpenAI(base_url="http://localhost:8080/v1", api_key="anything")
```

Full guide: [docs/opencode-setup.md](docs/opencode-setup.md)

---

## MCP Server

The proxy is a built-in [MCP (Model Context Protocol)](https://modelcontextprotocol.io) server with four tools:

| Tool | Description |
|------|-------------|
| `chat_completion` | Send messages to Codex/GPT via your subscription |
| `list_models` | List available models for the active profile |
| `check_auth` | Verify credentials are loaded correctly |
| `set_model` | Advisory model recommendation for a task type |

### Auto-configure for Claude Code

```bash
openai-proxy setup mcp --claude
```

Writes an entry into `~/.claude.json` (`mcpServers`). Or manually:

```json
{
  "mcpServers": {
    "openai-proxy": {
      "command": "/path/to/openai-proxy",
      "args": ["serve", "--mcp-stdio"]
    }
  }
}
```

### Auto-configure for opencode

```bash
openai-proxy setup mcp --opencode
```

Writes an `"mcp"` entry with `"type": "local"` into `opencode.json`.

### Streamable HTTP (remote / multi-user)

```bash
openai-proxy serve --mcp-http-port 8081
# MCP endpoint: POST http://localhost:8081/mcp
```

---

## ACP Server (Zed / JetBrains)

```bash
openai-proxy serve --acp-stdio
```

Implements ACP v0.11 (JSON-RPC 2.0 over stdio) with true incremental streaming — chunks arrive as bytes, never buffered.

**Zed** (`settings.json`):
```json
{
  "assistant": {
    "version": "2",
    "provider": {
      "name": "openai-proxy",
      "type": "acp",
      "command": "openai-proxy",
      "args": ["serve", "--acp-stdio"]
    }
  }
}
```

Full reference: [docs/acp.md](docs/acp.md)

---

## AG-UI Streaming Endpoint

```
POST /ag-ui/stream
Content-Type: application/json
Accept: text/event-stream
```

Emits five structured lifecycle events compatible with [CopilotKit](https://copilotkit.ai) and any AG-UI-aware frontend:

```
data: {"type":"RUN_STARTED","run_id":"..."}
data: {"type":"TEXT_MESSAGE_START","message_id":"..."}
data: {"type":"TEXT_MESSAGE_CONTENT","message_id":"...","delta":"Hello"}
data: {"type":"TEXT_MESSAGE_END","message_id":"..."}
data: {"type":"RUN_FINISHED","run_id":"..."}
```

Full reference: [docs/ag-ui.md](docs/ag-ui.md)

---

## Memory System (RAG)

Requires: `cargo build --features memory` (adds ~35MB to binary; SurrealDB embedded).

```toml
[memory]
enabled = true
embedding_model = "text-embedding-3-small"
```

Documents are stored with 1536-dimension embeddings in a SurrealDB HNSW index. On every chat request the proxy semantically searches the most relevant documents and prepends them as a system message context.

Pass `X-Memory-Scope: project` (or `session`, `global`) to namespace documents per project.

### REST API

```bash
# Store
curl -X POST http://localhost:8080/v1/memory/documents \
  -d '{"scope":"project","text":"The auth module uses JWT RS256.","metadata":{}}'

# Search
curl "http://localhost:8080/v1/memory/search?q=authentication&scope=project&limit=5"

# List / Delete
curl http://localhost:8080/v1/memory/documents?scope=project
curl -X DELETE http://localhost:8080/v1/memory/documents/<id>
```

Full reference: [docs/memory.md](docs/memory.md)

---

## Skills System

Skills are SKILL.md files with YAML frontmatter. The proxy loads them from configured directories, selects the most relevant ones per request via keyword + domain scoring, and prepends their content as a system message.

```bash
openai-proxy skills list
openai-proxy skills test "how do I refactor this function"
```

Config:
```toml
[skills]
dirs = ["~/.config/oproxy/skills", "~/.claude/skills"]
max_injected = 3
```

---

## Webhook Hooks

The proxy fires AG-UI-compatible JSON payloads to configured HTTP endpoints at key lifecycle points without affecting request latency (fire-and-forget, 5s timeout):

```bash
cp hooks.example.toml hooks.toml
openai-proxy serve --hooks-config hooks.toml
```

Events: `on_request_received`, `on_text_delta`, `on_tool_call_start`, `on_tool_call_args`, `on_tool_result_submitted`, `on_response_complete`, `on_error`.

Full reference: [docs/hooks.md](docs/hooks.md)

---

## A2A Agent Discovery

```bash
openai-proxy serve --a2a
```

Mounts `GET /.well-known/agent.json` — the A2A Agent Card. Multi-agent orchestrators (opencode, LangGraph, etc.) can discover and call this proxy as a named agent in a pipeline.

Full reference: [docs/a2a.md](docs/a2a.md)

---

## Docker

```bash
# Build
docker build -t openai-proxy .

# Run (ChatGPT subscription)
docker run --rm \
  -v ~/.codex/auth.json:/run/secrets/auth.json:ro \
  -e CODEX_AUTH_PATH=/run/secrets/auth.json \
  -p 8080:8080 \
  openai-proxy

# Run (API key)
docker run --rm \
  -e OPENAI_API_KEY=sk-... \
  -p 8080:8080 \
  openai-proxy

# With memory feature (requires --features memory build)
docker compose up
```

The `docker-compose.yaml` mounts `~/.codex/auth.json` and `~/.config/oproxy/` as read-only volumes and persists SurrealDB data in a named volume.

Build with memory:
```dockerfile
RUN cargo build --release --locked --features memory
```

---

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
curl http://localhost:8080/health
curl http://localhost:8080/v1/models
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

## Auth reference

`~/.codex/auth.json` is written by `codex login`. The proxy reads it at startup.

```json
{
  "access_token": "eyJ...",
  "account_id":   "db1fc050-5df3-42c1-be65-9463d9d23f0b",
  "api_key":      "sk-proj-..."
}
```

`access_token` takes priority over `api_key` when both are present.

---

## Troubleshooting

**`Cannot load ~/.codex/auth.json`** — Run `codex login`.

**`401 Unauthorized`** — Token expired (~1 hour TTL). Run `codex login` again or re-auth via `opencode /connect → openai-proxy`.

**`403 Forbidden`** — ChatGPT backend rejected the request headers. Check [openai/codex releases](https://github.com/openai/codex/releases) for updated header requirements.

**`400 model_not_available`** — The requested model is not available on your active backend profile. Check `openai-proxy config show` and [docs/opencode-setup.md](docs/opencode-setup.md#model-selection).

**Plugin not loading in opencode** — Ensure `bun install` has run inside `plugin/` and the path in `opencode.json` is correct (absolute or relative to the opencode working directory).

**`429 Too Many Requests`** — ChatGPT subscription Codex usage limit reached. Check usage at [chatgpt.com/codex](https://chatgpt.com/codex).

---

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
│   ├── skills.rs          # SKILL.md parser, loader, selector
│   ├── mcp.rs             # MCP JSON-RPC 2.0 server (stdio + HTTP)
│   ├── mcp_client.rs      # MCP tool schema loader for passthrough injection
│   ├── acp.rs             # ACP v0.11 stdio server
│   ├── agui.rs            # AG-UI 5-event SSE endpoint
│   ├── memory.rs          # SurrealDB RAG (feature-gated: --features memory)
│   ├── hooks.rs           # Webhook lifecycle hooks
│   └── a2a.rs             # A2A Agent Card endpoint
│
├── plugin/                # opencode native TypeScript plugin
│   └── src/
│       ├── index.ts       # config, auth, shell.env, event hooks
│       ├── auth.ts        # ~/.codex/auth.json reader
│       ├── codex-login.ts # spawns `codex login`, returns tokens
│       └── config.ts      # provider ID, base URL, model list
│
├── docs/
│   ├── config.md          # Full configuration reference
│   ├── opencode-setup.md  # opencode integration guide
│   ├── memory.md          # SurrealDB memory system reference
│   ├── ag-ui.md           # AG-UI endpoint reference
│   ├── acp.md             # ACP stdio reference (Zed / JetBrains)
│   ├── a2a.md             # A2A Agent Card reference
│   └── hooks.md           # Webhook hooks reference
│
├── scripts/
│   ├── start.sh           # Build if needed, then run HTTP proxy
│   └── start-mcp.sh       # Run as MCP stdio server
│
├── Dockerfile             # Multi-stage build (builder + debian-slim runtime)
├── docker-compose.yaml    # Compose with auth mount + data volume
├── SKILL.md               # agentskills.io skill (setup + MCP sub-skills)
├── opencode.json          # Level-2 static opencode provider config
├── hooks.example.toml     # Webhook hooks config template
├── CLAUDE.md              # Guidance for Claude Code agents working in this repo
└── Cargo.toml
```

---

## License

MIT
