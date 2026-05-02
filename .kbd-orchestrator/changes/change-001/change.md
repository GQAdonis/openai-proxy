# change-001: MCP protocol types + stdio transport
**Status:** [ ] pending  
**Agent:** rust-reviewer  
**File:** `src/mcp.rs` (new)

## Goal
Create a new `src/mcp.rs` module implementing MCP JSON-RPC 2.0 protocol types and a stdio transport. Exposes four tools (`chat_completion`, `list_models`, `check_auth`, `set_model`) by reusing existing `AppState` and `codex::*` logic — zero changes to proxy.rs or codex.rs.

## Tasks
- [ ] Define `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError` serde types
- [ ] Define `McpTool`, `McpContent`, `McpToolResult` types
- [ ] Implement `handle_initialize` → server info + capabilities
- [ ] Implement `handle_tools_list` → 4 tool definitions with JSON Schema
- [ ] Implement `chat_completion_tool` → calls codex backend, returns text
- [ ] Implement `list_models_tool` → returns model list as text content
- [ ] Implement `check_auth_tool` → validates auth presence, returns status text
- [ ] Implement `set_model_tool` → confirmation text (session hint)
- [ ] Implement `run_stdio(state: AppState)` → BufReader stdin loop, JSON dispatch, stdout write
- [ ] Handle MCP lifecycle: initialize, initialized notification, tools/list, tools/call, ping

## Acceptance
- `echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' | openai-proxy --mcp-stdio` returns valid initialize response
- `tools/list` returns all 4 tools
