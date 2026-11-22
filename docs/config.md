# Configuration Reference

`openai-proxy` uses a layered configuration system. Later layers override earlier ones.

## Precedence (lowest → highest)

1. Built-in defaults
2. The selected `config.toml`. Use `--config <path>` or `OPROXY_CONFIG` to select a different file.
3. Environment variables
4. Explicit CLI flags

---

## Config File Location

```bash
# Show the config file path and whether it exists
openai-proxy config path

# Scaffold a default config file
openai-proxy setup config
```

The default is the platform configuration directory under `oproxy/config.toml`. On Linux it uses `$XDG_CONFIG_HOME`, falling back to `~/.config`. On macOS it uses `~/Library/Application Support`. `config path` reports the default location. `--config` selects the file for `serve` or `config show`.

---

## Full Config Reference

```toml
[server]
host = "0.0.0.0"    # Bind address. Env: HOST. CLI: --host
port = 8181         # Listen port.  Env: PORT. CLI: --port

[backend]
# Wire API format for OpenAI API key users.
# "responses" = OpenAI Responses API (default)
# "chat"      = OpenAI Chat Completions API
wire_api = "responses"    # Env: CODEX_WIRE_API. CLI: --wire-api

[skills]
# List of directories to load SKILL.md files from.
# PROXY_SKILLS_DIRS appends colon-separated entries. Skill CLI subcommands also accept --dirs.
dirs = []

# Maximum skills to inject per request.
# Env: PROXY_SKILLS_MAX. Default: 3
max_injected = 3

[mcp]
# Path to mcp.toml containing tool schema definitions.
# Loads callable schema definitions for OpenAI-compatible chat requests.
# config_path = "~/.config/oproxy/mcp.toml"

[hooks]
# File path for webhook event delivery, or a trusted destination alias.
# Env: PROXY_HOOKS_CONFIG. CLI: --hooks-config
# config_path = "/absolute/path/to/hooks.toml"
# destination = "audit"

[memory]
# Requires: cargo build --features memory
enabled = false

# SurrealDB path. Defaults to memory.db under the platform's oproxy data directory.
db_path = ""

# Embedding model for a dedicated local service with PROXY_EMBEDDINGS_API_KEY.
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
| `PROXY_CHAT_AUTH` | — | Set `api_key` to explicitly select an environment API key for chat |
| `OPENAI_API_KEY` | — | API key used by the explicit environment chat selection |
| `CODEX_AUTH_PATH` | — | Path to auth.json |
| `CODEX_BACKEND_URL` | — | Custom backend URL, subject to credential-class restrictions |
| `PROXY_BACKEND_CREDENTIAL_CLASS` | — | Required for a custom backend. Use `api_key` or `subscription`. |
| `CODEX_DEFAULT_MODEL` | — | Default model override |
| `MCP_HTTP_PORT` | — | MCP HTTP server port |
| `PROXY_MCP_TOKEN` | — | Optional bearer credential for MCP HTTP |
| `PROXY_EMBEDDINGS_API_KEY` | — | Separate, optional embeddings API key for a dedicated local service |
| `PROXY_HOOK_DESTINATIONS` | — | JSON map of approved webhook aliases to URLs |
| `PROXY_CONSOLE_DATA` | — | Directory for console state and private service data |
| `PROXY_CONSOLE_WORKSPACE` | — | Assigned workspace in dedicated local mode |
| `PROXY_CONSOLE_ORIGIN` | — | Allowed browser origin and advertised origin when configured |
| `PROXY_CONSOLE_PRINCIPALS` | — | Path to the principal registry for shared mode |
| `PROXY_CONSOLE_LOCAL_AUTO_SESSION` | — | Set to `1` for automatic sessions on a trusted loopback workstation |

See [Codex credential ownership](codex-auth.md) and the [console deployment guide](console.md) for account selection, native SDK runtime variables and shared mode.

---

## Example Configurations

### Minimal — ChatGPT Subscription

No config file needed. Run:
```bash
codex login
openai-proxy serve --host 127.0.0.1
```

### Minimal — OpenAI API Key

```bash
PROXY_CHAT_AUTH=api_key openai-proxy serve --host 127.0.0.1
```

Supply `OPENAI_API_KEY` through the service environment or your secret manager before starting the proxy.

### Full ChatGPT Subscription Config

```toml
[server]
port = 8181

[skills]
dirs = ["~/.config/oproxy/skills"]
max_injected = 5

[modes]
a2a = true
```

### Full API Key with Memory

```toml
[server]
port = 8181

[backend]
wire_api = "responses"

[memory]
enabled = true
db_path = "~/.local/share/oproxy/memory.db"
embedding_model = "text-embedding-3-small"
```

Build with `cargo build --release --locked --features memory`. See [Memory](memory.md) for local retrieval and optional embeddings.

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
required = ["path"]
[tool.input_schema.properties.path]
type = "string"
description = "Absolute path to the file"
```

---

## Config Show

```bash
openai-proxy config show
```

Prints supported settings after file values and environment overrides are applied. Use the authenticated console's Settings page to see the running service configuration and the sources of its effective values.
