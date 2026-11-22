# A2A Agent Card Reference

The proxy exposes an optional Agent Card at `GET /.well-known/agent.json`. The card advertises OpenAI-compatible chat and model discovery. Clients invoke those capabilities through the proxy HTTP API. See the [integration guide](a2a-integration.md) for examples.

## Enabling discovery

```bash
openai-proxy serve --a2a --host 127.0.0.1 --port 8181
curl --fail http://127.0.0.1:8181/.well-known/agent.json
```

To enable discovery in the service's [configuration file](config.md), set:

```toml
[modes]
a2a = true
```

The card is public when discovery is enabled, including in shared mode. Chat and model discovery follow the authentication rules of their own routes.

## Agent Card response

```json
{
  "name": "openai-proxy",
  "description": "OpenAI Chat Completions proxy backed by Codex/Responses API",
  "url": "http://127.0.0.1:8181",
  "version": "0.1.0",
  "capabilities": {
    "streaming": true,
    "tools": true,
    "multi_turn": true
  },
  "skills": [
    {
      "id": "chat_completion",
      "name": "Chat Completion",
      "description": "Complete a conversation using the configured model backend"
    },
    {
      "id": "list_models",
      "name": "List Models",
      "description": "List available models for the configured backend profile"
    }
  ],
  "input_modes": ["text"],
  "output_modes": ["text", "stream"]
}
```

`url` uses the origin of a valid `PROXY_CONSOLE_ORIGIN` when configured. Otherwise it is derived from the server's listening address. An unspecified bind address advertises `127.0.0.1` with the effective port. Set an HTTPS origin for deployments reached through a reverse proxy.

| Advertised skill | Client endpoint |
| --- | --- |
| `chat_completion` | `POST /v1/chat/completions` |
| `list_models` | `GET /v1/models` |

`multi_turn` describes sending conversation history as `messages` in the OpenAI-compatible request. Agent Card support is limited to discovery. Task lifecycle, push notifications and artifact exchange are outside this interface.
