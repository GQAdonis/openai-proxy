# Configuration Reference

`openai-proxy` uses a layered configuration system. Later layers override earlier ones.

## Precedence (lowest → highest)

1. Built-in defaults
2. `$XDG_CONFIG_HOME/oproxy/config.toml` (typically `~/.config/oproxy/config.toml`)
3. `--config <path>` CLI flag / `OPROXY_CONFIG` env var
4. Environment variables
5. CLI flags (highest priority)

---

## Config File Location

```bash
# Show the config file path and whether it exists
openai-proxy config path

# Scaffold a default config file
openai-proxy setup config
```

Default path: `~/.config/oproxy/config.toml`

---

## Full Config Reference

```toml
[server]
host = "0.0.0.0"    # Bind address. Env: HOST. CLI: --host
port = 8080         # Listen port.  Env: PORT. CLI: --port

[backend]
# Wire API format for OpenAI API key users.
# "responses" = OpenAI Responses API (default)
# "chat"      = OpenAI Chat Completions API
wire_api = "responses"    # Env: CODEX_WIRE_API. CLI: --wire-api

[skills]
# Colon-separated list of directories to load SKILL.md files from.
# Env: PROXY_SKILLS_DIRS (merged, not replaced). CLI: --dirs (in subcommands)
dirs = []

# Maximum skills to inject per request.
# Env: PROXY_SKILLS_MAX. Default: 3
max_injected = 3

[mcp]
# Path to mcp.toml containing tool schema definitions.
# When set, tool schemas are injected into every proxied request.
# config_path = "~/.config/oproxy/mcp.toml"

[hooks]
# Path to hooks.toml for webhook event delivery.
# Env: PROXY_HOOKS_CONFIG. CLI: --hooks-config
# config_path = "~/.config/oproxy/hooks.toml"

[memory]
# Requires: cargo build --features memory
enabled = false

# SurrealDB file path. Defaults to $XDG_DATA_HOME/oproxy/memory.db
# (~/.local/share/oproxy/memory.db)
db_path = ""

# OpenAI embedding model. Requires OPENAI_API_KEY.
embedding_model = "text-embedding-3-small"

[modes]
# Enable A2A Agent Card at GET /.well-known/agent.json
a2a = false    # CLI: --a2a
```

---

## Environment Variables

| Variable | Config key | Description |
|----------|------------|-------------|
| `HOST` | `server.host` | Bind address |
| `PORT` | `server.port` | Listen port |
| `CODEX_WIRE_API` | `backend.wire_api` | `responses` or `chat` |
| `PROXY_SKILLS_DIRS` | `skills.dirs` | Colon-separated skill dirs (merged) |
| `PROXY_SKILLS_MAX` | `skills.max_injected` | Max skills per request |
| `PROXY_HOOKS_CONFIG` | `hooks.config_path` | Hooks config path |
| `OPROXY_CONFIG` | — | Config file path override |
| `OPENAI_API_KEY` | — | OpenAI API key (auth fallback) |
| `CODEX_AUTH_PATH` | — | Path to auth.json |
| `CODEX_BACKEND_URL` | — | Override backend URL |
| `CODEX_DEFAULT_MODEL` | — | Default model override |
| `MCP_HTTP_PORT` | — | MCP HTTP server port |

---

## Example Configurations

### Minimal — ChatGPT Subscription

No config file needed. Run:
```bash
codex login
openai-proxy serve
```

### Minimal — OpenAI API Key

```bash
OPENAI_API_KEY=sk-... openai-proxy serve
```

### Full ChatGPT Subscription Config

```toml
[server]
port = 8080

[skills]
dirs = ["~/.config/oproxy/skills"]
max_injected = 5

[modes]
a2a = true
```

### Full API Key with Memory

```toml
[server]
port = 8080

[backend]
wire_api = "responses"

[memory]
enabled = true
db_path = "~/.local/share/oproxy/memory.db"
embedding_model = "text-embedding-3-small"
```

Build: `cargo build --release --features memory`

### MCP Tool Passthrough

```toml
[mcp]
config_path = "~/.config/oproxy/mcp.toml"
```

`~/.config/oproxy/mcp.toml`:
```toml
[[tool]]
name = "read_file"
description = "Read the contents of a file"
[tool.input_schema]
type = "object"
[tool.input_schema.properties.path]
type = "string"
description = "Absolute path to the file"
required = ["path"]
```

---

## Config Show

```bash
openai-proxy config show
```

Prints the resolved configuration with all defaults, env var overrides, and config file values applied.
