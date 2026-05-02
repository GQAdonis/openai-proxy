# Plan: MCP Server + agentskills.io Skill Packaging
**Phase:** mcp-skill-packaging  
**Date:** 2026-05-01  
**Based on:** assessment.md (Verdict: PROCEED)

---

## Ordered Change List

| # | Change ID | Description | Agent | Est. Lines | Dependencies |
|---|---|---|---|---|---|
| 1 | change-001 | Add MCP protocol types + stdio transport | rust-reviewer | ~200 | none |
| 2 | change-002 | Add `--mcp-stdio` CLI mode to main.rs | rust-reviewer | ~50 | change-001 |
| 3 | change-003 | Add Streamable HTTP MCP transport | rust-reviewer | ~150 | change-001 |
| 4 | change-004 | Add `--mcp-http` CLI mode to main.rs | rust-reviewer | ~30 | change-003 |
| 5 | change-005 | Add `CODEX_DEFAULT_MODEL` env var support | rust-reviewer | ~20 | none |
| 6 | change-006 | Extend plugin shell.env hook | typescript-reviewer | ~15 | change-005 |
| 7 | change-007 | Create SKILL.md + scripts/ + references/ | general-purpose | ~120 | change-002 |
| 8 | change-008 | Update README.md with MCP + skill sections | general-purpose | ~80 | change-007 |

---

## Change Details

### change-001: MCP protocol types + stdio transport
**File:** `src/mcp.rs` (new)  
**Goal:** Implement the MCP JSON-RPC 2.0 protocol layer and a stdio transport that exposes `chat_completion`, `list_models`, `check_auth`, and `set_model` tools. Reuses `AppState` and calls existing `codex::*` functions — zero changes to proxy logic.

Key types:
- `JsonRpcRequest { jsonrpc, id, method, params }`
- `JsonRpcResponse { jsonrpc, id, result/error }`
- `McpTool { name, description, input_schema }`
- `McpToolResult { content: Vec<McpContent> }`
- `pub async fn run_stdio(state: AppState)` — read line-delimited JSON from stdin, dispatch, write to stdout

Tool implementations:
- `handle_initialize` → returns server info + tool list
- `handle_tools_list` → returns all 4 tool definitions with JSON Schema input_schema
- `handle_tools_call` → dispatches to `chat_completion_tool`, `list_models_tool`, `check_auth_tool`, `set_model_tool`

**Tasks:**
- [ ] Add `src/mcp.rs` with all types and tool handlers
- [ ] Implement `chat_completion_tool`: calls `codex::convert_request` + fires reqwest to backend, returns text
- [ ] Implement `list_models_tool`: returns model list as text
- [ ] Implement `check_auth_tool`: checks `auth.access_token.is_some()` or `auth.api_key.is_some()`
- [ ] Implement `set_model_tool`: stores preferred model in a thread-local or returns confirmation text
- [ ] Implement `run_stdio` loop: BufReader on stdin, line → JSON parse → dispatch → JSON write to stdout
- [ ] Handle MCP lifecycle: `initialize`, `initialized`, `tools/list`, `tools/call`, `ping`

### change-002: `--mcp-stdio` CLI mode in main.rs
**File:** `src/main.rs`  
**Goal:** Add `--mcp-stdio` boolean flag to `Args`. When set, skip HTTP server startup and call `mcp::run_stdio(state).await` instead. Binary becomes a pure MCP stdio server — usable in Claude Code's `mcpServers` config.

**Tasks:**
- [ ] Add `#[arg(long)]` `mcp_stdio: bool` to `Args`
- [ ] Branch after state construction: if `mcp_stdio` → `mcp::run_stdio(state).await` else → existing axum serve block
- [ ] Add `mod mcp;` declaration

### change-003: Streamable HTTP MCP transport
**File:** `src/mcp.rs` (extend)  
**Goal:** Add `pub async fn run_http(state: AppState, port: u16)` that binds an axum router on the given port, handles `POST /mcp` with JSON-RPC dispatch, and supports SSE streaming for `tools/call` responses when the tool streams (i.e. `chat_completion` with `stream: true`).

Per the 2026 MCP spec: Streamable HTTP uses `POST /mcp` with `Content-Type: application/json` for requests and `text/event-stream` for streaming responses.

**Tasks:**
- [ ] Add `run_http(state, port)` function with its own axum Router
- [ ] `POST /mcp` handler: parse JSON-RPC, dispatch, return JSON or SSE
- [ ] Share tool dispatch logic with stdio transport (extract `dispatch_tool_call` fn)
- [ ] Add streaming response path for `chat_completion` tool

### change-004: `--mcp-http` CLI mode in main.rs
**File:** `src/main.rs`  
**Goal:** Add `--mcp-http-port` optional u16 flag. When set, spawn `mcp::run_http(state.clone(), port)` as a concurrent task alongside the existing HTTP proxy server. Both run simultaneously.

**Tasks:**
- [ ] Add `#[arg(long, env = "MCP_HTTP_PORT")]` `mcp_http_port: Option<u16>` to `Args`
- [ ] If `mcp_http_port.is_some()` → `tokio::spawn(mcp::run_http(state.clone(), port))`
- [ ] Add log line: `"MCP Streamable HTTP server listening on http://0.0.0.0:{port}/mcp"`

### change-005: `CODEX_DEFAULT_MODEL` env var
**Files:** `src/main.rs`, `src/codex.rs`, `.env.example`  
**Goal:** Read `CODEX_DEFAULT_MODEL` at startup and store in `AppState`. `codex::map_model` uses it as a fallback override when the client sends a generic model name.

**Tasks:**
- [ ] Add `default_model: Option<String>` to `AppState`
- [ ] Read `CODEX_DEFAULT_MODEL` env var in `main.rs`, store in state
- [ ] Pass `state.default_model` into `codex::convert_request` as an override hint
- [ ] Add `CODEX_DEFAULT_MODEL=` to `.env.example` with a comment

### change-006: Plugin shell.env hook extension
**File:** `plugin/src/index.ts`  
**Goal:** Extend the `shell.env` hook to also inject `CODEX_PROXY_URL` and `CODEX_DEFAULT_MODEL` into every shell the agent spawns, so subprocesses can pick up the preferred proxy URL and model without any manual configuration.

**Tasks:**
- [ ] In the `shell.env` hook, add `CODEX_PROXY_URL: process.env.CODEX_PROXY_URL ?? PROXY_BASE_URL`
- [ ] Add `CODEX_DEFAULT_MODEL: process.env.CODEX_DEFAULT_MODEL ?? ""` (omit if empty)

### change-007: SKILL.md + scripts/ + references/
**Files:** `SKILL.md` (new), `scripts/start.sh` (new), `scripts/start-mcp.sh` (new), `references/auth.md` (new)  
**Goal:** Package the proxy as an agentskills.io compliant skill with two sub-skills: `openai-proxy/setup` and `openai-proxy/mcp`. The SKILL.md provides progressive disclosure: metadata at startup, full body on activation, reference files on demand.

**Tasks:**
- [ ] Create `SKILL.md` with `name`, `description`, `license: MIT`, `compatibility` fields
- [ ] Write `openai-proxy/setup` skill body: prerequisites, build steps, auth setup, env vars table
- [ ] Write `openai-proxy/mcp` skill body: Claude Code config block, tool reference table
- [ ] Create `scripts/start.sh`: builds if needed, runs `./target/release/openai-proxy`
- [ ] Create `scripts/start-mcp.sh`: runs with `--mcp-stdio` flag (for Claude Code mcpServers)
- [ ] Create `references/auth.md`: detailed auth.json format, token refresh, troubleshooting
- [ ] Make scripts executable (`chmod +x`)

### change-008: README.md updates
**File:** `README.md`  
**Goal:** Add MCP Server section (Level 4 integration) and agentskills.io section after existing content. Keep existing sections intact.

**Tasks:**
- [ ] Add "Level 4 — MCP Server" section under opencode integration
- [ ] Add Claude Code mcpServers config block
- [ ] Add Streamable HTTP MCP usage block
- [ ] Add "agentskills.io Skill" section with skill install command
- [ ] Add `MCP_HTTP_PORT` and `CODEX_DEFAULT_MODEL` to env vars table
- [ ] Add `--mcp-stdio`, `--mcp-http-port` to CLI flags table

---

## Recommended Execution Order

```
change-001 (MCP core)
    └── change-002 (stdio CLI)
    └── change-003 (HTTP transport)
            └── change-004 (HTTP CLI)
change-005 (default model env)
    └── change-006 (plugin hook)
change-001+002 done →
    change-007 (SKILL.md + scripts)
        └── change-008 (README)
```

Parallelizable: change-005/006 can run alongside change-001 through 004.

---

## Cargo.toml Changes

No new dependencies required. The MCP protocol is JSON-RPC 2.0 over stdio/HTTP — `serde_json`, `tokio`, and `axum` (already present) cover everything. The 2026 MCP spec deliberately avoids requiring specialized crates.

---

## Acceptance Criteria

- [ ] `cargo build --release` succeeds with zero errors
- [ ] `openai-proxy --mcp-stdio` starts as a valid MCP server (responds to `initialize` request)
- [ ] Claude Code can be configured with `mcpServers` pointing to `openai-proxy --mcp-stdio` and list tools
- [ ] `openai-proxy --mcp-http-port 8081` binds on 8081 and responds to `POST /mcp`
- [ ] `SKILL.md` passes agentskills.io schema validation
- [ ] `scripts/start.sh` and `scripts/start-mcp.sh` execute successfully
- [ ] All existing proxy behavior unchanged (no regression on `POST /v1/chat/completions`)
