# openai-proxy — AGENTS.md

## opencode plugin (native)

| Task | Command |
|------|---------|
| Install deps | `bun install` (or `npm install`) in `plugin/` |
| Build | `bun run src/index.ts` (dev) or `bun build src/index.ts --outdir dist --format esm --target bun` |
| Typecheck | `bun run typecheck` (tsc --noEmit) |

**Plugin lifecycle hooks** (`plugin/src/index.ts`):
- `config` — injects `codex` provider + model definitions into live opencode config. Guard: skips if `config.provider.codex` already set (don't clobber user override).
- `auth` — registers `"codex"` provider in `/connect`. Loader reads `~/.codex/auth.json` (or opencode's stored auth). Two methods: OAuth (delegates to `codex login` subprocess) and API key.
- `shell.env` — injects `CODEX_AUTH_PATH`, `CODEX_PROXY_URL`, `CODEX_DEFAULT_MODEL` into every spawned shell.
- `event` — on `session.created`, pings `/health` via fetch (1.5s timeout), shows TUI warning toast if proxy unreachable.

**Auth flow**: `spawnCodexLogin()` in `plugin/src/codex-login.ts` runs `codex login` as subprocess, reads resulting `~/.codex/auth.json`, returns token in opencode OAuth success shape. Token expiry: 55 min (conservative from ~1h TTL).

**Plugin path** in `opencode.json`: `"file:./plugin"`. Load globally via `~/.config/opencode/opencode.json` or project-locally via repo-root `opencode.json`.

**Plugin provider ID**: `"codex"`. Models surfaced: `gpt-5.3-codex`, `codex-mini`, `gpt-4o` (128K context, 16K output).

## CLI setup commands (generate opencode/MCP config)

```bash
# Level 2 static provider config
openai-proxy setup opencode [--global|--project] [--port N] [--force]

# MCP server registration for opencode or Claude Code
openai-proxy setup mcp [--opencode|--claude] [--port N]
```

`setup_opencode()` in `src/cli/setup.rs` detects ChatGPT subscription vs API key from `~/.codex/auth.json` and emits different model lists per credential type. Default model: `openai-proxy/gpt-5.5`. Writes to `~/.config/opencode/opencode.json` (global) or `.opencode/opencode.json` (project). Use `--force` to overwrite existing entry.

## Project-level opencode config

`opencode.json` at repo root declares:
- `"plugin": ["file:./plugin"]` — loads the TypeScript plugin
- `"model": "codex/gpt-5.3-codex"` — default model
- `"provider.codex"` — `@ai-sdk/openai-compatible` pointing at `http://localhost:8080/v1` with models: gpt-5.3-codex, codex-mini, gpt-4o

`.opencode/` directory contains:
- **10 opsx-* commands** — `/opsx-new`, `/opsx-apply`, `/opsx-archive`, `/opsx-continue`, `/opsx-explore`, `/opsx-ff`, `/opsx-sync`, `/opsx-verify`, `/opsx-onboard`, `/opsx-bulk-archive` — experimental OpenSpec artifact workflow
- **10 openspec-* skills** — implement the OPSX workflow steps

## MCP server (opencode integration)

```bash
# stdio transport (for opencode MCP config)
openai-proxy serve --mcp-stdio

# Streamable HTTP transport
openai-proxy serve --mcp-http-port 8081
# Endpoint: POST http://localhost:8081/mcp
```

Four MCP tools: `chat_completion`, `list_models`, `check_auth`, `set_model`. Local MCP tool schemas loaded from `[[tool]]` TOML config via `mcp_client.rs` and injected into `req.tools`.

## AG-UI endpoint

`POST /ag-ui/stream` — 5-event SSE protocol (`RUN_STARTED` → `TEXT_MESSAGE_START` → `TEXT_MESSAGE_CONTENT` → `TEXT_MESSAGE_END` → `RUN_FINISHED`). Wire uses `SCREAMING_SNAKE_CASE` enum variants.

## Auth credentials

`~/.codex/auth.json` written by `codex login`. Access token takes priority over API key. Token expires ~1h — user must re-authenticate. The plugin's `shell.env` hook injects `CODEX_AUTH_PATH` so the Rust binary finds auth without separate config.

## Build invariants

- `cargo build` (no features) must produce zero warnings
- `cargo build --features memory` adds ~35MB (SurrealDB + HNSW)
- `serde_yml = "0.0"` (not 0.9) — maintained fork
- Edition 2024, LTO + codegen-units=1 in release

## Key dev commands

```bash
cargo clippy -- -D warnings
cargo test --lib                          # unit only
cargo test --test integration non_streaming_max_tokens_respected -- --nocapture  # single integration test
RUST_LOG=openai_proxy=debug cargo run    # dev server
```

Integration tests require live `~/.codex/auth.json` or `OPENAI_API_KEY`. They hit real backends — expensive and rate-limited.

## Architecture notes

- `AppState` is `Clone`, passed via `State<AppState>` extractor
- `BackendProfile` (ChatGptCodex / OpenAiResponses / OpenAiChatCompletions) selected at startup from auth — never changes at runtime
- SSE event translation: Responses API `response.output_text.delta` → OpenAI `chat.completion.chunk`
- Skills system: keyword + domain-boost scoring, injected as system message prefix
- Memory RAG (feature-gated): SurrealDB HNSW, 500ms timeout on search
- Webhook hooks: fire-and-forget AG-UI-compatible JSON, 5s timeout
- `req.tools` in `ChatCompletionRequest` is `Option<serde_json::Value>` (raw JSON), not typed — MCP passthrough merges JSON arrays
