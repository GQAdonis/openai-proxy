# change-002: `--mcp-stdio` CLI mode in main.rs
**Status:** [ ] pending  
**Agent:** rust-reviewer  
**File:** `src/main.rs`  
**Depends on:** change-001

## Goal
Add `--mcp-stdio` boolean CLI flag. When set, skip the axum HTTP server and run as a pure MCP stdio server. This is the mode used in Claude Code's `mcpServers` configuration.

## Tasks
- [ ] Add `#[arg(long)]` `mcp_stdio: bool` to `Args` struct
- [ ] Add `mod mcp;` declaration at top of main.rs
- [ ] After state construction: `if args.mcp_stdio { return mcp::run_stdio(state).await; }`
- [ ] Log: `"Starting in MCP stdio mode"` before the call

## Claude Code Config (for README/SKILL.md reference)
```json
{
  "mcpServers": {
    "openai-proxy": {
      "command": "/path/to/openai-proxy",
      "args": ["--mcp-stdio"]
    }
  }
}
```

## Acceptance
- `openai-proxy --mcp-stdio` starts without binding any TCP port
- Responds to MCP `initialize` JSON-RPC over stdin/stdout
