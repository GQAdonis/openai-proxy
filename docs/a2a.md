# A2A Agent Card Reference

The proxy implements the [A2A (Agent-to-Agent)](https://google.github.io/A2A/) specification's Agent Card endpoint, enabling AI orchestrators to discover and call this proxy as a task-capable agent.

---

## Enabling A2A

```bash
openai-proxy serve --a2a --port 8080
```

Or in `~/.config/oproxy/config.toml`:
```toml
[modes]
a2a = true
```

---

## Agent Card Endpoint

```
GET /.well-known/agent.json
```

### Example Response

```json
{
  "name": "openai-proxy",
  "description": "OpenAI-compatible proxy routing to ChatGPT Subscription or OpenAI Responses API",
  "url": "http://127.0.0.1:8080",
  "version": "0.1.0",
  "capabilities": {
    "streaming": true,
    "pushNotifications": false,
    "stateTransitionHistory": false
  },
  "defaultInputModes": ["text/plain"],
  "defaultOutputModes": ["text/plain"],
  "skills": [
    {
      "id": "chat",
      "name": "Chat Completion",
      "description": "Process chat completion requests and stream responses",
      "inputModes": ["text/plain"],
      "outputModes": ["text/plain"]
    }
  ]
}
```

---

## A2A Task Request

A2A orchestrators send tasks to `POST /v1/chat/completions` in standard OpenAI format. The proxy translates and forwards the request.

### Request

```json
{
  "model": "gpt-5.5",
  "messages": [
    { "role": "user", "content": "Summarize this code file: ..." }
  ],
  "stream": true
}
```

### Response (streaming)

Standard OpenAI SSE `text/event-stream`.

---

## Orchestrator Discovery

An A2A orchestrator discovers this proxy by fetching `GET /.well-known/agent.json`. The `url` field tells it where to send tasks. Orchestrators that follow the A2A spec will use `POST /` with A2A task format — the proxy maps these to the OpenAI chat completions endpoint.

---

## Integration with opencode

opencode supports A2A for multi-agent coordination. When `--a2a` is enabled, opencode (or any A2A-aware tool) can treat this proxy as a named agent in a pipeline:

```json
{
  "agents": {
    "openai-proxy": {
      "url": "http://127.0.0.1:8080"
    }
  }
}
```
