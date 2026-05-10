# opencode Setup Guide

`openai-proxy` acts as an OpenAI-compatible provider for [opencode](https://opencode.ai), routing requests to either a **ChatGPT Subscription (Plus/Pro)** account via OAuth or an **OpenAI API key** via the Responses API.

---

## Prerequisites

- Rust toolchain (`cargo build --release` or a pre-built binary)
- One of:
  - **ChatGPT Plus/Pro subscription** — sign in with `codex login` (uses `~/.codex/auth.json`)
  - **OpenAI API key** — set `OPENAI_API_KEY` environment variable
- opencode v1.14+ installed

---

## Quick Start (Automatic)

```bash
# 1. Start the proxy (uses ~/.codex/auth.json or OPENAI_API_KEY automatically)
openai-proxy serve --port 8080

# 2. In another terminal, write the opencode config for you
openai-proxy setup opencode --global --port 8080

# 3. Open opencode — select provider "codex"
opencode
```

The `setup opencode` command detects whether you have a ChatGPT subscription token or an API key and generates the correct model list.

---

## Plugin Installation (v0.1.0+)

The plugin is published on npm as `@prometheus-ags/opencode-codex-proxy`. This is the recommended installation method.

### From npm

```bash
npm i @prometheus-ags/opencode-codex-proxy
```

Load globally in `~/.config/opencode/opencode.json`:

```json
{
  "plugin": ["node_modules/@prometheus-ags/opencode-codex-proxy"]
}
```

### From local source

```bash
cd plugin && bun install && bun run build
```

Load from the local path:

```json
{
  "plugin": ["file:./plugin"]
}
```

The `opencode.json` at the repo root already declares the local plugin — it works out of the box when you clone the repo.

### What the plugin does

| Hook | Effect |
|------|--------|
| `config` | Injects the `codex` provider and all models into opencode's live config — no manual edits needed |
| `auth` | Registers `"codex"` in `/connect` with OAuth (`codex login`) or API key options; reads from opencode's own auth store, then falls back to `~/.codex/auth.json` |
| `shell.env` | Injects `CODEX_AUTH_PATH`, `CODEX_PROXY_URL`, and `CODEX_DEFAULT_MODEL` into every shell the agent spawns |
| `event` | On `session.created`, pings `/health`, shows a TUI warning if the proxy is not running, and refreshes model context limits from `/v1/models` |

---

## Manual Configuration

Create or edit `~/.config/opencode/opencode.json` (global) or `.opencode/opencode.json` (project):

> **Note:** The provider ID is `codex` (changed from `openai-proxy` in v0.1.0). Update any existing config that uses the old name.

### ChatGPT Subscription (Plus/Pro)

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "codex": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Codex (via proxy)",
      "options": {
        "baseURL": "http://127.0.0.1:8080/v1",
        "apiKey": "codex-proxy"
      },
      "models": {
        "gpt-5.5":       { "name": "GPT-5.5",       "limit": { "context": 400000, "output": 32768 } },
        "gpt-5.4":       { "name": "GPT-5.4",       "limit": { "context": 400000, "output": 32768 } },
        "gpt-5.4-mini":  { "name": "GPT-5.4 Mini",  "limit": { "context": 200000, "output": 16384 } },
        "gpt-5.4-nano":  { "name": "GPT-5.4 Nano",  "limit": { "context": 128000, "output":  8192 } },
        "gpt-5.3-codex": { "name": "GPT-5.3 Codex", "limit": { "context": 400000, "output": 32768 } },
        "gpt-5.3-chat":  { "name": "GPT-5.3 Chat",  "limit": { "context": 128000, "output": 16384 } },
        "gpt-5.2-chat":  { "name": "GPT-5.2 Chat",  "limit": { "context": 128000, "output": 16384 } }
      }
    }
  },
  "model": "codex/gpt-5.5"
}
```

Start the proxy: `openai-proxy serve --port 8080`

### OpenAI API Key — Responses API

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "codex": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Codex (via proxy)",
      "options": {
        "baseURL": "http://127.0.0.1:8080/v1",
        "apiKey": "codex-proxy"
      },
      "models": {
        "gpt-5.5":       { "name": "GPT-5.5",       "limit": { "context": 1000000, "output": 32768 } },
        "gpt-5.5-pro":   { "name": "GPT-5.5 Pro",   "limit": { "context": 1000000, "output": 32768 } },
        "gpt-5.4":       { "name": "GPT-5.4",       "limit": { "context":  400000, "output": 32768 } },
        "gpt-5.4-mini":  { "name": "GPT-5.4 Mini",  "limit": { "context":  200000, "output": 16384 } },
        "gpt-5.4-nano":  { "name": "GPT-5.4 Nano",  "limit": { "context":  128000, "output":  8192 } },
        "gpt-5.3-codex": { "name": "GPT-5.3 Codex", "limit": { "context":  400000, "output": 32768 } },
        "gpt-5.3-chat":  { "name": "GPT-5.3 Chat",  "limit": { "context":  128000, "output": 16384 } },
        "gpt-5.2-chat":  { "name": "GPT-5.2 Chat",  "limit": { "context":  128000, "output": 16384 } }
      }
    }
  },
  "model": "codex/gpt-5.5"
}
```

Start the proxy: `OPENAI_API_KEY=sk-... openai-proxy serve --port 8080`

---

## Model Selection

| Model | ChatGPT sub ctx | API key ctx | Notes |
|-------|:-----------:|:-------:|-------|
| `gpt-5.5` | 400K | 1M | Default; use for most tasks |
| `gpt-5.5-pro` | — | 1M | API key only |
| `gpt-5.4` | 400K | 400K | Balanced speed/quality |
| `gpt-5.4-mini` | 200K | 200K | `codex-mini` aliases here |
| `gpt-5.4-nano` | 128K | 128K | Lightweight tasks |
| `gpt-5.3-codex` | 400K | 400K | `gpt-4o`, `gpt-4` alias here |
| `gpt-5.3-chat` | 128K | 128K | Conversational |
| `gpt-5.2-chat` | 128K | 128K | |

The `GET /v1/models` endpoint always returns the authoritative list for the active backend profile.

---

## opencode Integration — Two Approaches

opencode ships a **built-in Codex plugin** that handles ChatGPT OAuth directly without a proxy. This project provides a **separate proxy-based plugin** that routes all requests through the proxy binary. They solve the same problem with different architectures and **must not be used simultaneously**.

### Approach 1: Built-in opencode Codex plugin (no proxy needed)

opencode includes `CodexAuthPlugin` internally. It intercepts requests to Codex-model IDs and routes them directly to `chatgpt.com/backend-api/codex/responses` using opencode's own Effect-TS fetch interception layer.

- **Credentials:** stored in `~/.local/share/opencode/auth.json` (opencode's own auth store)
- **Setup:** run `opencode auth login` — no external binary required
- **Scope:** opencode only — Claude Code, Zed, and other tools cannot share this credential path

### Approach 2: openai-proxy plugin (this project)

The TypeScript plugin in `plugin/` registers `codex` as a provider in opencode, routing all requests through the proxy binary at `localhost:8080/v1`.

- **Credentials:** reads from opencode's auth store (`~/.local/share/opencode/auth.json`) first, then falls back to `~/.codex/auth.json`
- **Setup:** start the proxy binary; install via `npm i @prometheus-ags/opencode-codex-proxy` or build locally
- **Scope:** shared — Claude Code (MCP), Zed (ACP), and AG-UI frontends all route through the same proxy instance

### Decision table

| Scenario | Recommended approach |
|----------|---------------------|
| Only using opencode, want minimal setup (no Rust required) | Built-in opencode Codex plugin |
| Using opencode + Claude Code + Zed in the same workflow | openai-proxy plugin |
| Need skill injection (`PROXY_SKILLS_DIRS`) per request | openai-proxy plugin |
| Need memory RAG (`--features memory`) | openai-proxy plugin |
| Need MCP or ACP transport for non-opencode clients | openai-proxy plugin |

> **Warning:** Do not activate both simultaneously. The built-in plugin and the proxy plugin both register the `codex` provider ID. Having both active will produce duplicate provider entries and unpredictable routing behavior.

### Disabling the built-in Codex plugin

If you are using the proxy-based approach and want to prevent the built-in plugin from interfering, add to your `opencode.json`:

```json
{
  "disabled": ["opencode:codex"]
}
```

---

## Auth credential path

The proxy reads `~/.codex/auth.json` by default. The plugin's `shell.env` hook sets `CODEX_AUTH_PATH` for child shells, so scripts that call the proxy binary inherit the correct path automatically.

If you ran `opencode auth login` previously and tokens are in `~/.local/share/opencode/auth.json`, the plugin's `auth.loader` reads them directly — no manual migration needed.

To point the proxy at a different credential file:

```bash
CODEX_AUTH_PATH=~/.local/share/opencode/auth.json openai-proxy serve
```

---

## Troubleshooting

**"Provider not found"** — Make sure the proxy is running before opening opencode.

**"Connection refused"** — Check the port matches between `--port` and `baseURL`.

**Provider shows as `openai-proxy` instead of `codex`** — You have an older `opencode.json`. The provider ID changed to `codex` in v0.1.0. Regenerate with `openai-proxy setup opencode` or update the provider key manually.

**400 errors from ChatGPT backend** — Ensure you are using a supported model. `gpt-5.5-pro` is not available on the ChatGPT subscription path; use `gpt-5.5` instead.

**Auth not working after `opencode auth login`** — The plugin's `auth.loader` tries opencode's own auth store first. If it still fails, run `codex login` to write `~/.codex/auth.json` as a fallback, or set `CODEX_AUTH_PATH` to point to opencode's auth file.

**Toast warning "Codex proxy not running"** — The proxy binary is not running or is on a different port. Start it with `openai-proxy serve` or set `CODEX_PROXY_URL` to match the actual URL.
