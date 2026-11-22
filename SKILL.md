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
  repository: https://github.com/GQAdonis/openai-proxy
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
| Rust stable | Compile the proxy | [rustup](https://rustup.rs/) |
| Node 26.5.0 and npm 11.16.0 | Build the embedded console | [Frontend build guide](frontend/README.md) |
| Codex CLI 0.169.0 | Managed OAuth login and the native Codex worker | [Install the native drivers](docs/console.md#install-the-native-drivers) |
| A ChatGPT Plus or Pro subscription | authentication | Required for Codex backend |

### Step 1: Authenticate

```bash
codex login
```

Use the Codex profile assigned to the service. The proxy watches the selected file store for login changes and periodically checks for missed updates. [Codex credential operation](docs/codex-auth.md)

### Step 2: Build and run

```bash
npm ci --prefix frontend --include=dev
cargo build --release --locked
./target/release/openai-proxy serve --host 127.0.0.1 --port 8181
```

The loopback endpoints are `http://127.0.0.1:8181/v1` and `http://127.0.0.1:8181/ui/`.

### Step 3: Point your client at the proxy

```bash
# OpenAI-compatible client base URL
export OPENAI_BASE_URL=http://127.0.0.1:8181/v1
```

For the native OpenCode 2.x subscription provider, build the plugin, register it with `plugins`, and configure the OpenCode server with a scoped service credential. Select an available `codex/<model>` ID. [OpenCode setup](docs/opencode-setup.md)

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `HOST` | `0.0.0.0` | Interface to bind |
| `PORT` | `8181` | HTTP proxy port |
| `CODEX_AUTH_PATH` | `~/.codex/auth.json` | Override auth file path |
| `CODEX_BACKEND_URL` | auto | Override upstream endpoint |
| `CODEX_DEFAULT_MODEL` | — | Default Codex model for generic aliases |
| `MCP_HTTP_PORT` | — | Enable MCP Streamable HTTP on this port |
| `OPENAI_API_KEY` | — | Fallback if no auth.json |
| `RUST_LOG` | — | Log filter (e.g. `openai_proxy=debug`) |

### Verify it's working

```bash
curl http://localhost:8181/health
# → {"status":"ok"}

curl http://localhost:8181/v1/models
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

For service-host client registration and transport settings, see [Provision local clients](docs/opencode-setup.md#provision-local-clients). The MCP `set_model` response provides an advisory recommendation.

### Reference Files

- `references/auth.md` — Auth file format, token refresh, troubleshooting
