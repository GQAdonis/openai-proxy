# change-004: `--mcp-http-port` CLI mode in main.rs
**Status:** [ ] pending  
**Agent:** rust-reviewer  
**File:** `src/main.rs`  
**Depends on:** change-003

## Goal
Add `--mcp-http-port` optional u16 flag. When set, spawn `mcp::run_http` as a concurrent tokio task alongside the main HTTP proxy. Both proxy and MCP HTTP server run simultaneously.

## Tasks
- [ ] Add `#[arg(long, env = "MCP_HTTP_PORT")]` `mcp_http_port: Option<u16>` to `Args`
- [ ] After state construction: if `mcp_http_port.is_some()` → `tokio::spawn(mcp::run_http(state.clone(), port))`
- [ ] Log: `"MCP Streamable HTTP server listening on http://0.0.0.0:{port}/mcp"`

## Acceptance
- `openai-proxy --mcp-http-port 8081` logs both the proxy bind address and MCP bind address
- Both endpoints respond simultaneously
