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

# 3. Open opencode — select provider "openai-proxy"
opencode
```

The `setup opencode` command detects whether you have a ChatGPT subscription token or an API key and generates the correct model list.

---

## Manual Configuration

Create or edit `~/.config/opencode/opencode.json` (global) or `.opencode/opencode.json` (project):

### ChatGPT Subscription (Plus/Pro) — OpenAI Max Plan

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "openai-proxy": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "OpenAI Proxy (ChatGPT Subscription Plus/Pro)",
      "options": {
        "baseURL": "http://127.0.0.1:8080/v1",
        "apiKey": "not-required"
      },
      "models": {
        "gpt-5.3-codex": { "name": "GPT-5.3 Codex", "limit": { "context": 400000, "output": 32768 } },
        "gpt-5.4":       { "name": "GPT-5.4",       "limit": { "context": 400000, "output": 32768 } },
        "gpt-5.5":       { "name": "GPT-5.5",       "limit": { "context": 400000, "output": 32768 } }
      }
    }
  },
  "model": "openai-proxy/gpt-5.5"
}
```

Start the proxy: `openai-proxy serve --port 8080`

### OpenAI API Key — Responses API

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "openai-proxy": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "OpenAI Proxy (API Key)",
      "options": {
        "baseURL": "http://127.0.0.1:8080/v1",
        "apiKey": "not-required"
      },
      "models": {
        "gpt-5.5":       { "name": "GPT-5.5",       "limit": { "context": 1000000, "output": 32768 } },
        "gpt-5.5-pro":   { "name": "GPT-5.5 Pro",   "limit": { "context": 1000000, "output": 32768 } },
        "gpt-5.4":       { "name": "GPT-5.4",       "limit": { "context": 200000,  "output": 16384 } },
        "gpt-5.3-codex": { "name": "GPT-5.3 Codex", "limit": { "context": 200000,  "output": 16384 } },
        "codex-mini":    { "name": "Codex Mini",     "limit": { "context": 96000,   "output": 8192  } }
      }
    }
  },
  "model": "openai-proxy/gpt-5.5"
}
```

Start the proxy: `OPENAI_API_KEY=sk-... openai-proxy serve --port 8080`

---

## Model Selection

| Model | Context | Auth |
|-------|---------|------|
| `gpt-5.5` | 400K (sub) / 1M (API key) | Both |
| `gpt-5.5-pro` | 1M | API key only |
| `gpt-5.4` | 400K / 200K | Both |
| `gpt-5.3-codex` | 400K / 200K | Both |
| `codex-mini` | 96K | API key only |

---

## Troubleshooting

**"Provider not found"** — Make sure the proxy is running before opening opencode.

**"Connection refused"** — Check the port matches between `--port` and `baseURL`.

**400 errors from ChatGPT backend** — Ensure you are using a supported model. `gpt-5.5-pro` and `codex-mini` are not available on the ChatGPT subscription path.
