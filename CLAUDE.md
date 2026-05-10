# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Purpose

`openai-proxy` is a Rust axum 0.8 proxy that bridges any OpenAI Chat Completions client to multiple backends: the ChatGPT Codex Responses API (ChatGPT Plus/Pro subscription), the OpenAI Responses API, and the OpenAI Chat Completions API. It is distributed as a CLI binary, an MCP server, an ACP server, an AG-UI streaming endpoint, an agentskills.io skill (`SKILL.md`), and a native opencode plugin (`plugin/`).

Authentication source determines backend automatically: `~/.codex/auth.json` with `access_token` → `chatgpt.com/backend-api/codex/responses`; with `api_key` or `OPENAI_API_KEY` → `api.openai.com/v1/responses` (or `v1/chat/completions` when `CODEX_WIRE_API=chat`). The proxy understands both the flat `{ "access_token": ... }` format and the nested `{ "tokens": { "access_token": ... } }` format written by Codex CLI ≥ v1.x.

## Commands

```bash
# Build (default — no SurrealDB)
cargo build

# Build with memory feature (adds ~35MB; SurrealDB/HNSW)
cargo build --features memory

# Release binary (~8MB stripped, no runtime deps)
cargo build --release

# Run all integration tests (requires ~/.codex/auth.json or OPENAI_API_KEY)
cargo test --test integration -- --nocapture

# Run a single test by name
cargo test --test integration non_streaming_max_tokens_respected -- --nocapture

# Unit tests only (no network)
cargo test --lib

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt

# Dev server with debug logging
RUST_LOG=openai_proxy=debug cargo run

# Dev server with full HTTP traces
RUST_LOG=openai_proxy=debug,tower_http=debug cargo run
```

## Architecture

### Request flow

```
client POST /v1/chat/completions
  → proxy::chat_completions()
      → inject_skills()          — prepend relevant SKILL.md content as system message
      → inject_memory()          — RAG search via SurrealDB HNSW; prepend context (#[cfg(feature="memory")])
      → resolve_model()          — canonicalize model alias; validate against BackendProfile
      → BackendProfile dispatch:
            ChatGptCodex / OpenAiResponses  → stream_responses() / non_stream_responses()
            OpenAiChatCompletions           → stream_chat_completions() / non_stream_chat_completions()
      → SSE forwarded back to client as OpenAI chat.completion.chunk events
```

### AppState (src/lib.rs)

Shared across all axum handlers via `State<AppState>`. Key fields:

| Field | Type | Purpose |
|-------|------|---------|
| `backend_profile` | `BackendProfile` | Which backend and wire format to use |
| `skills` | `Arc<Vec<SkillManifest>>` | SKILL.md manifests loaded at startup |
| `mcp_tools` | `Arc<Vec<McpToolSchema>>` | MCP tool schemas injected into `req.tools` |
| `memory` | `DynMemory` | Trait object — `NoopMemoryStore` by default |
| `memory_store` | `Option<Arc<MemoryStore>>` | Concrete type for REST handlers (`#[cfg(feature="memory")]`) |
| `hooks` | `Arc<dyn ProxyHooks>` | Webhook event delivery; defaults to `NullHooks` |

### BackendProfile (src/codex.rs)

Three variants control both the upstream URL and request shape:

- `ChatGptCodex` — strips `temperature`, `top_p`, `max_output_tokens`; forces `stream=true`, `store=false`; models: gpt-5.5, gpt-5.4, gpt-5.4-mini, gpt-5.4-nano, gpt-5.3-codex, gpt-5.3-chat, gpt-5.2-chat
- `OpenAiResponses` — Responses API format; passes `max_output_tokens`, tools; adds gpt-5.5-pro vs ChatGptCodex
- `OpenAiChatCompletions` — Chat Completions wire format; uses `messages[]`, `max_completion_tokens`; same model set as OpenAiResponses

Profile selected at startup based on auth credentials and `CODEX_WIRE_API`. Never changes at runtime.

### Key modules

| Module | Responsibility |
|--------|---------------|
| `codex.rs` | Auth loading, `BackendProfile`, model catalogue (`resolve_model`), request converters, Responses API SSE event types |
| `proxy.rs` | Axum handlers — `chat_completions()`, streaming/non-streaming paths for both wire formats, `inject_skills()`, `inject_memory()` |
| `openai.rs` | `ChatCompletionRequest`, `ChatCompletionResponse` — the public-facing OpenAI wire types |
| `models.rs` | `GET /v1/models` — profile-aware model list with `context_length`/`max_output_tokens` |
| `mcp.rs` | MCP JSON-RPC 2.0 server — stdio (`run_stdio`) + Streamable HTTP (`run_http`) transports |
| `acp.rs` | ACP v0.11 stdio server — incremental streaming via `bytes_stream()` |
| `agui.rs` | AG-UI endpoint — `POST /ag-ui/stream`; 5-event protocol (`RUN_STARTED` → `RUN_FINISHED`) |
| `config.rs` | `ProxyConfig` TOML loader; XDG paths; `apply_env()` merge; `expand_tilde()` |
| `cli/` | Clap subcommands: `setup opencode/mcp/config`, `skills list/validate/test`, `config show/path` |
| `skills.rs` | SKILL.md YAML frontmatter parser; `load_skills()`, `select_skills()` (keyword + domain-boost) |
| `mcp_client.rs` | `[[tool]]` TOML loader for MCP passthrough schemas injected into `req.tools` |
| `memory.rs` | `MemoryBackend` trait + `NoopMemoryStore` (always); `MemoryStore` SurrealDB + HNSW behind `#[cfg(feature="memory")]`; REST handlers |
| `hooks.rs` | `ProxyHooks` trait, `WebhookHooks`, `NullHooks` — lifecycle event delivery |
| `a2a.rs` | A2A Agent Card at `GET /.well-known/agent.json` (opt-in via `--a2a`/`modes.a2a`) |
| `error.rs` | `ProxyError` → axum HTTP response; includes `ModelNotAvailable` variant |

### Router (src/lib.rs → build_app)

```
POST /v1/chat/completions     → proxy::chat_completions
GET  /v1/models               → models::list_models
POST /ag-ui/stream            → agui::agui_stream
GET  /health                  → inline handler

# A2A (opt-in)
GET  /.well-known/agent.json  → a2a::agent_card_handler

# Memory REST (--features memory only)
POST   /v1/memory/documents   → memory::handlers::create_document
GET    /v1/memory/documents   → memory::handlers::list_documents
DELETE /v1/memory/documents/:id → memory::handlers::delete_document
GET    /v1/memory/search      → memory::handlers::search_documents
```

### Config precedence (lowest → highest)

Built-in defaults → `$XDG_CONFIG_HOME/oproxy/config.toml` → `--config <path>` → env vars → CLI flags

### opencode integration levels

1. **Native plugin** (`plugin/`) — TypeScript; published as `@prometheus-ags/opencode-codex-proxy` on npm; hooks `config`, `auth`, `shell.env`, `event`; recommended. Provider ID: `codex`.
2. **Static config** (`opencode.json` at repo root) — `@ai-sdk/openai-compatible` provider with provider ID `codex`; drop-in for any project
3. **Generic OpenAI client** — `OPENAI_BASE_URL=http://localhost:8080/v1`, any non-empty API key

`openai-proxy setup opencode` generates a correct `opencode.json` (detects ChatGPT OAuth vs API key; uses `{env:VAR}` syntax for opencode's interpolation format).

## Rust Patterns Required

All code in this repository must follow these patterns:

### Error handling
Use `thiserror` for `ProxyError` (library boundary); use `anyhow` + `?` in `main.rs` and CLI handlers. Never `unwrap()` outside tests.

### Feature-gated code
Memory feature is the only compile-time flag. Use `#[cfg(feature = "memory")]` precisely — do not leak concrete SurrealDB types into non-gated code. The `MemoryBackend` trait and `DynMemory` type alias are always available; `MemoryStore` is not.

### AppState extensions
`AppState` is `Clone` and passed via `State<AppState>` extractor. New shared state goes in `AppState`; new per-request context goes in handler parameters. Keep `AppState` fields `Arc<T>` for large data, raw for cheap-to-clone scalars.

### MCP tool injection
`req.tools` in `ChatCompletionRequest` is `Option<serde_json::Value>` (raw JSON array), not a typed `Vec<Tool>`. MCP passthrough merges JSON arrays — do not introduce typed structs at this boundary.

### SSE streaming
Responses API events use dot-notation: `response.output_text.delta`, `response.completed`. Chat Completions use `chat.completion.chunk`. Both are parsed in `codex.rs` under `ResponseStreamEvent`. ACP uses incremental `bytes_stream()` — never buffer the full response body.

### Skills selection
`select_skills(req, manifests, max)` in `skills.rs` uses keyword matching against the last user message plus a domain-boost scoring step. Skills are injected as a system message prefix. The inject path is in `proxy.rs::inject_skills()`.

## Key invariants

- `serde_yml = "0.0"` (not `0.9`) — maintained fork of deprecated `serde_yaml`; version scheme is `0.0.x`
- Integration tests require live credentials (`~/.codex/auth.json` or `OPENAI_API_KEY`); they hit real backends
- `load_real_auth()` in `src/lib.rs` is used by all integration tests — it panics cleanly if credentials are absent
- `cargo build` (no features) must always produce zero warnings; the memory feature may add cfg-attr suppression for non-feature builds
- The AG-UI `AguiEvent` enum uses `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` — wire values are `RUN_STARTED`, `TEXT_MESSAGE_CONTENT`, etc.
- opencode provider config uses `{env:VAR}` (not `${VAR}`) for runtime interpolation; the MCP key is `"mcp"` with `"type": "local"` (not `"mcpServers"` / `"type": "stdio"`)
- `codex-mini` and `gpt-4o-mini` are legacy aliases that resolve to `gpt-5.4-mini` in `resolve_model()`; the alias mapping lives in `src/codex.rs`
- opencode plugin provider ID is `codex` (not `openai-proxy`) — any config or test referencing the old name needs updating
