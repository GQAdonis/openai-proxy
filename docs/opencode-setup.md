# opencode Integration Guide

`openai-proxy` integrates with [opencode](https://opencode.ai) through a TypeScript plugin that registers a `codex` provider, handles authentication, and keeps the model list in sync with the running proxy. This guide covers everything from a fresh install to advanced configuration.

---

## Quick Start

```bash
# 1. Install the plugin
npm i @prometheus-ags/opencode-codex-proxy

# 2. Add to ~/.config/opencode/opencode.json
# (see Plugin Installation below for the exact JSON)

# 3. Authenticate — choose one:
codex login                          # ChatGPT Plus/Pro subscription
# — or —
export OPENAI_API_KEY=sk-...         # OpenAI API key

# 4. Start the proxy
openai-proxy serve

# 5. Open opencode
opencode
```

The plugin auto-injects the provider and models — no manual provider block needed.

---

## Prerequisites

- **opencode v1.14+**
- **openai-proxy** binary on your PATH (`cargo build --release` or a pre-built binary)
- One of:
  - ChatGPT Plus/Pro subscription — authenticated via `codex login`
  - OpenAI API key — set as `OPENAI_API_KEY`
- Node.js (for the npm plugin path) or Bun (for the local build path)

---

## Plugin Installation

### Path A — npm (recommended)

Install the package:

```bash
npm i @prometheus-ags/opencode-codex-proxy
```

Then add it to your opencode config. For a global install, edit `~/.config/opencode/opencode.json`:

```json
{
  "plugin": ["node_modules/@prometheus-ags/opencode-codex-proxy"],
  "disabled": ["opencode:codex"]
}
```

For a project-scoped install, place the same JSON in `opencode.json` at your project root.

> **`"disabled": ["opencode:codex"]`** is required. opencode ships a built-in Codex plugin that also registers the `codex` provider ID. Both cannot be active at the same time. See [Conflict with the Built-in Plugin](#conflict-with-the-built-in-plugin) for details.

> **Note on the plugin path:** The path in `plugin[]` is resolved relative to the config file's directory. If you install globally with `npm i -g`, the path will differ — use an absolute path in that case (e.g., `"/usr/local/lib/node_modules/@prometheus-ags/opencode-codex-proxy"`). Project-local installs with `npm i` (no `-g`) produce `node_modules/@prometheus-ags/opencode-codex-proxy`, which resolves correctly when opencode is started from the project directory.

### Path B — Build from source

Clone the repo and build the plugin:

```bash
cd /path/to/openai-proxy/plugin
bun install
bun run build
```

Then reference the local path:

```json
{
  "plugin": ["file:///path/to/openai-proxy/plugin"],
  "disabled": ["opencode:codex"]
}
```

Use `file://` with an absolute path to avoid resolution ambiguity. The repo root's `opencode.json` already references the local plugin using a relative file path — it works out of the box when opencode is launched from the repo directory.

---

## Auth Setup

The plugin supports three authentication flows. Pick the one that matches your access.

### Flow 1 — ChatGPT Plus/Pro subscription

```bash
# Install the Codex CLI (one-time)
npm i -g @openai/codex

# Authenticate — opens browser, writes ~/.codex/auth.json
codex login

# Start the proxy (reads ~/.codex/auth.json automatically)
openai-proxy serve
```

Open opencode. The plugin reads `~/.codex/auth.json` and the `codex` provider appears automatically. Tokens last approximately one hour; run `codex login` again when you see 401 errors.

### Flow 2 — OpenAI API key

```bash
# Start the proxy with your key
OPENAI_API_KEY=sk-... openai-proxy serve
```

Open opencode and navigate to `/connect`. Select the `codex` provider and choose **"OpenAI API key (non-subscription fallback)"**. Enter your key.

### Flow 3 — opencode unified auth (opencode auth login)

If you previously ran `opencode auth login`, your credentials are in `~/.local/share/opencode/auth.json`. The plugin's auth loader checks that file first.

To make the proxy use those same credentials:

```bash
CODEX_AUTH_PATH=~/.local/share/opencode/auth.json openai-proxy serve
```

---

## What the Plugin Does

The plugin (`@prometheus-ags/opencode-codex-proxy`, default export `CodexProxyPlugin`) registers four hooks with opencode.

### config hook

Mutates the live opencode config object before opencode finishes loading. It injects a `codex` provider entry:

```
config.provider.codex = {
  npm: "@ai-sdk/openai-compatible",
  name: "Codex (via proxy)",
  options: {
    baseURL: PROXY_BASE_URL,   // default: "http://localhost:8080/v1"
    apiKey: <access_token or api_key or "codex-proxy">
  },
  models: { ... }
}
```

Guard: if `config.provider.codex` already exists (user has a manual block), the hook skips — your override is respected.

The initial model list is built from the static `PROXY_MODELS` array in `plugin/src/config.ts`. The event hook (below) replaces this with a live list fetched from `/v1/models` once the proxy confirms it is healthy.

### auth hook

Registers the `codex` provider in opencode's `/connect` UI.

- `auth.provider = "codex"`
- `auth.loader` is called whenever opencode needs credentials:
  1. Calls opencode's own auth store (`~/.local/share/opencode/auth.json`)
  2. If `stored.access` is truthy, returns `{ apiKey: stored.access }`
  3. Otherwise calls `readCodexAuth()`, which reads `~/.codex/auth.json` (or `CODEX_AUTH_PATH`)
  4. Returns `{ apiKey: access_token ?? api_key }` or `null`
- `auth.methods` exposes two options in the `/connect` UI:
  - **OAuth** (`type: "oauth"`) — runs `spawnCodexLogin()`, which spawns `codex login` as a subprocess with `stdio: "inherit"` so the CLI handles the TTY and browser flow. After the CLI exits, the plugin reads `~/.codex/auth.json` and returns the token with a 55-minute expiry.
  - **API key** (`type: "api"`, label: `"OpenAI API key (non-subscription fallback)"`) — standard key entry.

### shell.env hook

Injects environment variables into every shell the opencode agent spawns:

| Variable | Source | Purpose |
|----------|--------|---------|
| `CODEX_AUTH_PATH` | `CODEX_AUTH_PATH` env var, else `~/.codex/auth.json` | Tells child processes where credentials live |
| `CODEX_PROXY_URL` | `CODEX_PROXY_URL` env var, else `PROXY_BASE_URL` | Tells child processes which proxy to hit |
| `CODEX_DEFAULT_MODEL` | `CODEX_DEFAULT_MODEL` env var only | Only set if the env var is defined |

### event hook (session.created)

Fires once per opencode session on the `session.created` event:

1. Pings `{proxy_host}/health` with a 1.5-second timeout.
2. **If not healthy:** displays a TUI warning toast:
   > "Codex proxy not running on http://localhost:8080/v1. Start it with: cargo run..."
3. **If healthy:** fetches `{PROXY_BASE_URL}/models` with a 2-second timeout. Parses `{ data: [{ id, context_length?, max_output_tokens? }] }` and refreshes the model list used by the config hook.

---

## Environment Variables

| Variable | Default | Effect |
|----------|---------|--------|
| `CODEX_PROXY_URL` | `http://localhost:8080/v1` | Override the proxy base URL. Set before Node loads the plugin for the config hook to pick it up; the shell.env hook also injects it into agent shells. |
| `CODEX_AUTH_PATH` | `~/.codex/auth.json` | Override the credential file path. Also injected into agent shells. |
| `CODEX_DEFAULT_MODEL` | _(not set)_ | If set, injected into agent shells as `CODEX_DEFAULT_MODEL`. |

---

## Static Config Alternative (no Node, no plugin)

The repo root contains an `opencode.json` that declares the `codex` provider statically using `@ai-sdk/openai-compatible`. Drop it into any project directory to get opencode routing requests to the proxy without installing the plugin.

Trade-offs compared to the plugin:

- No dynamic model refresh from `/v1/models`
- No auth integration with opencode's `/connect` UI
- No health-check toast warnings
- No environment variable injection into agent shells
- Model list and context limits are static — you must update the JSON manually when models change

This path works well for CI environments, Docker containers, or any situation where Node is unavailable.

---

## Conflict with the Built-in Plugin

opencode ships an internal `CodexAuthPlugin` that also registers provider ID `codex`. If both are active simultaneously, you get duplicate provider entries and unpredictable routing.

**To disable the built-in plugin:**

```json
{
  "disabled": ["opencode:codex"]
}
```

Add this to whichever `opencode.json` loads the proxy plugin.

### Key differences between the two

| | Built-in opencode Codex plugin | openai-proxy plugin |
|---|---|---|
| Credentials stored at | `~/.local/share/opencode/auth.json` | `~/.codex/auth.json` (primary), opencode store (fallback) |
| Proxy binary required | No | Yes |
| Shared with Claude Code / Zed | No | Yes (all tools route through the proxy) |
| SKILL.md injection per request | No | Yes (via proxy) |
| Memory RAG | No | Yes (`--features memory`) |
| MCP / ACP transport | No | Yes |

### Decision table

| Scenario | Recommended approach |
|----------|---------------------|
| Only using opencode, no Rust build, minimal setup | Built-in opencode Codex plugin |
| Using opencode + Claude Code + Zed + other tools | openai-proxy plugin |
| Need SKILL.md injection per request | openai-proxy plugin |
| Need memory RAG (`--features memory`) | openai-proxy plugin |
| Need MCP or ACP transport for non-opencode clients | openai-proxy plugin |
| Need shared credential path across all tools | openai-proxy plugin |

---

## Model Reference

All context and output limits shown are tokens.

| Model | ChatGPT sub context | API key context | Output | Notes |
|-------|:------------------:|:---------------:|:------:|-------|
| `gpt-5.5` | 400,000 | 1,000,000 | 32,768 | Default model (`DEFAULT_MODEL`) |
| `gpt-5.5-pro` | not available | 1,000,000 | 32,768 | API key only — not on ChatGPT subscription |
| `gpt-5.4` | 400,000 | 400,000 | 32,768 | |
| `gpt-5.4-mini` | 200,000 | 200,000 | 16,384 | `codex-mini` aliases here |
| `gpt-5.4-nano` | 128,000 | 128,000 | 8,192 | |
| `gpt-5.3-codex` | 400,000 | 400,000 | 32,768 | |
| `gpt-5.3-chat` | 128,000 | 128,000 | 16,384 | |
| `gpt-5.2-chat` | 128,000 | 128,000 | 16,384 | |

The plugin's event hook refreshes these limits from `/v1/models` on each session start, so the live limits reflect what the proxy reports rather than the static defaults.

---

## Verifying the Setup

```bash
# Check proxy is healthy
curl http://localhost:8080/health

# List available models
curl http://localhost:8080/v1/models | jq .

# Test a completion (non-streaming)
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-5.5","messages":[{"role":"user","content":"hello"}],"stream":false}'
```

---

## Troubleshooting

**"Provider not found" / "Connection refused"**
The proxy is not running or is on the wrong port. Run `openai-proxy serve` and confirm it binds to port 8080. If you changed the port, set `CODEX_PROXY_URL` to match.

**Provider shows as `openai-proxy` instead of `codex`**
You have an older `opencode.json` that uses the old provider key. The provider ID changed to `codex` in v0.1.0. Delete the old provider block and let the plugin inject it, or regenerate with `openai-proxy setup opencode`.

**Both built-in and proxy plugin are active**
Add `"disabled": ["opencode:codex"]` to the `opencode.json` that loads the proxy plugin.

**Auth not working after `opencode auth login`**
The plugin's auth loader reads opencode's own auth store first. If the proxy binary also needs those credentials, start it with `CODEX_AUTH_PATH=~/.local/share/opencode/auth.json openai-proxy serve`.

**Toast warning: "Codex proxy not running"**
The proxy binary is not running or is on a different port than `PROXY_BASE_URL` (default `http://localhost:8080/v1`). Start the proxy or set `CODEX_PROXY_URL` to the actual URL before starting opencode.

**400 errors with `gpt-5.5-pro` on a ChatGPT subscription**
`gpt-5.5-pro` is not available on ChatGPT Plus/Pro subscriptions. Use `gpt-5.5` instead. The model is only accessible with an OpenAI API key.

**Plugin not loading after `npm install`**
Confirm the path in `plugin[]` resolves to the package directory. For a project-local install, `node_modules/@prometheus-ags/opencode-codex-proxy` is correct when opencode starts from the project root. For a global install, use the absolute path.

**`codex login` fails**
The Codex CLI is not installed. Run `npm i -g @openai/codex` first.

**401 errors / token expired**
Tokens issued by `codex login` last approximately one hour. Run `codex login` again to refresh. The plugin reads the updated `~/.codex/auth.json` on the next session start.
