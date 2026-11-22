# A2A Integration Guide

Use the [Agent Card](a2a.md) to discover the proxy's advertised origin and capabilities, then call its OpenAI-compatible endpoints. The Agent Card is an opt-in discovery surface. An orchestrator must support the advertised REST calls as well as discovery.

## Discover the proxy

```bash
openai-proxy serve --a2a --host 127.0.0.1 --port 8181
curl --fail http://127.0.0.1:8181/.well-known/agent.json
```

Use `card.url` as the request origin. Verify that it matches the expected origin before sending any credentials. For a remote service, use an explicitly configured HTTPS origin.

## List models and send chat requests

`list_models` maps to `GET /v1/models`. Pick an ID from the response for `chat_completion`, which maps to `POST /v1/chat/completions`. For the private workstation example:

```bash
curl --fail http://127.0.0.1:8181/v1/models
```

```http
POST /v1/chat/completions HTTP/1.1
Host: 127.0.0.1:8181
Content-Type: application/json

{
  "model": "<id-from-v1-models>",
  "messages": [
    { "role": "user", "content": "Summarize the supplied release notes." }
  ],
  "stream": true
}
```

The response uses OpenAI-compatible SSE when `stream` is `true` and JSON when it is `false`. Include the history in `messages` for subsequent turns. For shared deployments, supply the service-issued principal credential as a bearer for `/v1` requests. The discovery route itself is public. The proxy selects the upstream credential from its own configuration. [Credential ownership](codex-auth.md)

## Related interfaces

- To expose MCP tools alongside the card, start `openai-proxy serve --a2a --host 127.0.0.1 --mcp-http-port 8081`. Connect MCP clients to `http://127.0.0.1:8081/mcp`.
- To render validated A2UI surfaces with native chat, use the [embedded console](console.md). Plain text returned by an OpenAI-compatible completion may contain JSON for callers to handle in their own applications.
- For AG-UI streaming, see the [AG-UI endpoint reference](ag-ui.md).
