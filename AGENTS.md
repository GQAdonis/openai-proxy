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

**Plugin provider ID**: `"codex"`. Models surfaced come from `plugin/src/config.ts::PROXY_MODELS`, refreshed at runtime from the proxy's own `GET /v1/models` (which derives from `src/model_catalog.rs`, the single source of truth). Current catalogue: `gpt-5.6-sol`/`gpt-5.6-terra`/`gpt-5.6-luna` (1.05M context, 128K output), `gpt-5.5`, `gpt-5.5-pro`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.4-nano`, `gpt-5.3-codex`, `gpt-5.3-chat`, `gpt-5.2-chat`, plus legacy aliases (`gpt-5.6`, `codex-mini`, `gpt-4o`, etc.).

## Codex-native plugin

Distinct from the opencode plugin above — this targets the Codex CLI's own native plugin system, not opencode.

- **Manifest**: `.codex-plugin/plugin.json` — `name`, `version`, `description`, `author`, `homepage`, `repository`, `license`, `keywords`, `interface` (displayName/category/capabilities), `skills` (points at repo-root `SKILL.md`), `mcpServers` (points at repo-root `.mcp.json`).
- **MCP config**: `.mcp.json` — launches the same MCP stdio server as the opencode integration (`openai-proxy serve --mcp-stdio`).
- **Marketplace manifest**: `.agents/plugins/marketplace.json` — this repo IS the marketplace root; its single `plugins[]` entry uses `"source": {"source": "local", "path": "."}` (self-referencing — the plugin lives in the same repo as the marketplace manifest, not a separate git-subdir).
- **Install (local clone)**: `codex plugin marketplace add <path-to-this-repo>` to register it as a marketplace, then `codex plugin add openai-proxy@openai-proxy` to install.
- **Install (remote)**: `codex plugin marketplace add <this-repo's-git-url>` works the same way once the repo is pushed — Codex CLI fetches the repo and reads `.agents/plugins/marketplace.json` from its root.
- No self-serve central Codex plugin directory exists yet (as of 2026-07) — distribution is via an explicit marketplace source add, not a public listing.

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
- `"model": "codex/gpt-5.5"` — default model
- `"provider.codex"` — `@ai-sdk/openai-compatible` pointing at `http://localhost:8181/v1`; models block mirrors `src/model_catalog.rs` (see the "opencode plugin (native)" section above for the current list)

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
