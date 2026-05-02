# Auth Reference

## ~/.codex/auth.json

Written by `codex login` after a successful OAuth browser flow.

```json
{
  "access_token": "eyJ...",
  "account_id":   "db1fc050-5df3-42c1-be65-9463d9d23f0b",
  "api_key":      "sk-proj-..."
}
```

### Field Reference

| Field | Required | When used |
|---|---|---|
| `access_token` | With `account_id` | ChatGPT subscription → `chatgpt.com/backend-api/codex/responses` |
| `account_id` | With `access_token` | Required by the ChatGPT backend as the `chatgpt-account-id` header |
| `api_key` | Alone | Standard OpenAI API key → `api.openai.com/v1/responses` |

If both `access_token` and `api_key` are present, `access_token` takes priority.

## Priority Order

The proxy resolves credentials in this order at startup:

1. `~/.codex/auth.json` with `access_token` + `account_id` → ChatGPT backend
2. `~/.codex/auth.json` with `api_key` → OpenAI API
3. `OPENAI_API_KEY` env var → OpenAI API
4. None found → process exits with a clear error

## Token Expiry

ChatGPT OAuth tokens expire approximately **1 hour** after issue. The opencode plugin sets a conservative 55-minute expiry hint so opencode prompts for refresh before the token actually expires.

To refresh manually:

```bash
codex login
```

Or through opencode:

```
opencode
/connect
→ Select "codex"
→ Select "Sign in with ChatGPT (Codex subscription)"
```

## Custom Auth Path

Override the default `~/.codex/auth.json` path:

```bash
# CLI flag
openai-proxy --auth-path /custom/path/auth.json

# Environment variable
CODEX_AUTH_PATH=/custom/path/auth.json openai-proxy

# .env file
CODEX_AUTH_PATH=/custom/path/auth.json
```

The opencode plugin injects `CODEX_AUTH_PATH` into every shell it spawns, so the path follows the agent automatically.

## Security

- Treat `~/.codex/auth.json` like a password file
- Do not commit it, share it, or paste it into issue trackers
- The file is in `.gitignore` by default
- The MCP `check_auth` tool only reports presence/type, never the token value

## Troubleshooting

**`Cannot load ~/.codex/auth.json`**  
Run `codex login`. The file is written after a successful browser OAuth flow.

**`401 Unauthorized` from upstream**  
Token expired (~1 hour TTL). Run `codex login` or use `/connect` in opencode.

**`403 Forbidden` from upstream**  
The ChatGPT backend rejected the request headers. Check the [openai/codex releases](https://github.com/openai/codex/releases) for updated required header sets.

**`check_auth` MCP tool reports "No credentials loaded"**  
Neither `access_token` nor `api_key` was found. Verify the file exists and is valid JSON:

```bash
cat ~/.codex/auth.json | python3 -m json.tool
```

**`429 Too Many Requests` from upstream**  
You've hit your ChatGPT subscription's Codex usage limit. Check at [chatgpt.com/codex](https://chatgpt.com/codex).
