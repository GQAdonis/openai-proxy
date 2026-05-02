---
name: openai-proxy
description: Use your ChatGPT Plus/Pro subscription as an OpenAI-compatible API via a local Rust proxy. Bridges any OpenAI Chat Completions client to the Codex Responses API with MCP server support for direct agent integration.
license: MIT
compatibility:
  claude-code: ">=1.0"
  codex-cli: ">=2.0"
  gemini-cli: ">=0.1"
metadata:
  author: openai-proxy contributors
  repository: https://github.com/yourusername/openai-proxy
  tags:
    - openai
    - codex
    - proxy
    - mcp
    - chatgpt
    - opencode
allowed-tools:
  - Bash
  - Read
  - Write
---

# openai-proxy Skills

Two skills are available. Load the one matching your task:

- **`openai-proxy/setup`** — Install, configure, and run the proxy
- **`openai-proxy/mcp`** — Use the proxy as an MCP server with Claude Code / Codex CLI

---

## Skill: openai-proxy/setup

**Activation keywords:** start openai-proxy, codex proxy, chatgpt subscription, opencode setup, codex login

### Prerequisites

| Tool | Purpose | Install |
|---|---|---|
| Rust 1.80+ | compile the proxy | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Codex CLI | OAuth login flow | `npm i -g @openai/codex` |
| A ChatGPT Plus or Pro subscription | authentication | Required for Codex backend |

### Step 1: Authenticate

```bash
codex login
```

This opens a browser, completes OAuth, and writes credentials to `~/.codex/auth.json`. The proxy reads this file automatically at startup.

### Step 2: Build and run

```bash
# Option A: use the provided script (builds if needed, then runs)
./scripts/start.sh

# Option B: direct cargo
cargo build --release
./target/release/openai-proxy
```

The proxy listens on `http://0.0.0.0:8080/v1` by default.

### Step 3: Point your client at the proxy

```bash
# Any OpenAI-compatible client
export OPENAI_BASE_URL=http://localhost:8080/v1
export OPENAI_API_KEY=anything   # not forwarded upstream; auth.json is used

# opencode: provider appears automatically via opencode.json / plugin
opencode
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `HOST` | `0.0.0.0` | Interface to bind |
| `PORT` | `8080` | HTTP proxy port |
| `CODEX_AUTH_PATH` | `~/.codex/auth.json` | Override auth file path |
| `CODEX_BACKEND_URL` | auto | Override upstream endpoint |
| `CODEX_DEFAULT_MODEL` | — | Default Codex model for generic aliases |
| `MCP_HTTP_PORT` | — | Enable MCP Streamable HTTP on this port |
| `OPENAI_API_KEY` | — | Fallback if no auth.json |
| `RUST_LOG` | — | Log filter (e.g. `openai_proxy=debug`) |

### Verify it's working

```bash
curl http://localhost:8080/health
# → {"status":"ok"}

curl http://localhost:8080/v1/models
# → lists available models

curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer anything" \
  -H "Content-Type: application/json" \
  -d '{"model":"codex-mini","messages":[{"role":"user","content":"Say hi"}]}'
```

---

## Skill: openai-proxy/mcp

**Activation keywords:** mcp server, claude code mcp, codex mcp, openai-proxy mcp, use codex via mcp

### Overview

`openai-proxy` includes a built-in MCP (Model Context Protocol) server. This lets Claude Code, Codex CLI, Gemini CLI, and other MCP-compatible tools call Codex completions as native tool calls — no base URL reconfiguration needed.

### MCP Tools Available

| Tool | Description |
|---|---|
| `chat_completion` | Send messages to Codex via your subscription |
| `list_models` | List available models and aliases |
| `check_auth` | Verify credentials are loaded |
| `set_model` | Get a model recommendation for your task |

### Option A: stdio transport (Claude Code — recommended)

Add to `~/.claude.json` (or project `.claude.json`):

```json
{
  "mcpServers": {
    "openai-proxy": {
      "command": "/absolute/path/to/openai-proxy",
      "args": ["--mcp-stdio"]
    }
  }
}
```

Or use the provided script:

```json
{
  "mcpServers": {
    "openai-proxy": {
      "command": "/absolute/path/to/openai-proxy/scripts/start-mcp.sh"
    }
  }
}
```

Restart Claude Code. The `openai-proxy` MCP server appears in your tool list.

### Option B: Streamable HTTP transport (remote/multi-user)

```bash
# Start proxy with MCP HTTP server on port 8081
openai-proxy --mcp-http-port 8081

# or via environment variable
MCP_HTTP_PORT=8081 ./scripts/start.sh
```

Then point your MCP client at `http://localhost:8081/mcp`.

### Test the MCP server

```bash
# stdio: send initialize request
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' \
  | ./target/release/openai-proxy --mcp-stdio

# HTTP: list tools
curl -X POST http://localhost:8081/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

### Client Context Customization Hooks

| Hook | How |
|---|---|
| Override proxy URL | Set `CODEX_PROXY_URL` — picked up by the opencode plugin shell.env hook |
| Override default model | Set `CODEX_DEFAULT_MODEL` — forwarded to all spawned shells |
| Session model switch | Call `set_model` MCP tool — advisory recommendation |
| Auth path | Set `CODEX_AUTH_PATH` — injected automatically by the plugin |

### Reference Files

- `references/auth.md` — Auth file format, token refresh, troubleshooting
