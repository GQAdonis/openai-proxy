# change-003: Streamable HTTP MCP transport
**Status:** [ ] pending  
**Agent:** rust-reviewer  
**File:** `src/mcp.rs` (extend)  
**Depends on:** change-001

## Goal
Add `pub async fn run_http(state: AppState, port: u16)` for MCP Streamable HTTP transport. Per 2026 MCP spec: `POST /mcp` with JSON-RPC body, responds JSON or SSE stream. Shares dispatch logic with stdio transport.

## Tasks
- [ ] Extract `async fn dispatch(state: &AppState, req: JsonRpcRequest) -> JsonRpcResponse` shared by both transports
- [ ] Add `run_http(state, port)` function with axum Router
- [ ] `POST /mcp` handler: parse body, call dispatch, return `application/json` response
- [ ] For `tools/call chat_completion` with `stream: true`: return `text/event-stream` SSE
- [ ] CORS headers on `/mcp` endpoint (same `CorsLayer::permissive()` pattern)

## Acceptance
- `curl -X POST http://localhost:8081/mcp -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' ` returns tool list JSON
