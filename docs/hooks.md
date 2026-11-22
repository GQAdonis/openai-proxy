# Hooks System

openai-proxy can send JSON webhooks at key points in the OpenAI-compatible chat
request lifecycle. Use them to observe requests, responses and tool calls. For
AG-UI clients, use the separate [AG-UI SSE endpoint](ag-ui.md).

Hooks are opt-in. The service sends webhooks only when configured with a file
or trusted destination alias. A dedicated local service can send text and tool
arguments. Choose receivers that are authorized to handle that content.

---

## Quick Start

```bash
# Copy the example config and point the proxy at it
cp hooks.example.toml hooks.toml

# Start the proxy with hooks enabled
openai-proxy --hooks-config hooks.toml
```

Or use the environment variable:

```bash
PROXY_HOOKS_CONFIG=hooks.toml openai-proxy
```

---

## Startup Configuration

| Method | Example |
|--------|---------|
| CLI flag | `openai-proxy serve --hooks-config /etc/proxy/hooks.toml` |
| Environment variable | `PROXY_HOOKS_CONFIG=/etc/proxy/hooks.toml openai-proxy serve` |
| Service configuration | Set `hooks.config_path` to the file, or set `hooks.destination` to an alias |

The file is read at startup. Changes require a proxy restart. An explicitly
configured file takes precedence over the alias option. An invalid file or alias
leaves webhook delivery disabled.

Set `PROXY_HOOK_DESTINATIONS` to a JSON map of approved alias names to receiver URLs
if operators need to select an alias from Settings. Alias mode emits only
`on_request_received`. Destinations must use HTTPS or loopback HTTP. URLs may
include a path and query, and must omit user credentials and fragments. The
webhook client refuses redirects.

---

## `hooks.toml` Format Reference

Each section configures a single event type. Omitting a section silences that
event — no HTTP POST is made for it.

### `[on_request_received]`

Fired when the OpenAI-compatible chat handler has resolved request credentials.

```toml
[on_request_received]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_text_delta]`

Fired as the proxy processes text deltas from the Responses backend.

```toml
[on_text_delta]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_tool_call_start]`

Fired when the Responses backend announces a function call.

```toml
[on_tool_call_start]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_tool_call_args]`

Fired as the proxy processes argument deltas from the Responses backend.

```toml
[on_tool_call_args]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_tool_result_submitted]`

Fired when a tool result message is submitted back to the model in a multi-turn
conversation (i.e. a message with `role = "tool"` in the request body).

```toml
[on_tool_result_submitted]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_response_complete]`

Fired when the proxy processes a completion event from the Responses backend,
or the result of a non-streaming request to the Chat Completions backend.

```toml
[on_response_complete]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_error]`

Fired for explicit model-availability errors and rejected upstream requests.

```toml
[on_error]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### Full example

```toml
[on_request_received]
url = "http://localhost:3000/events"

[on_text_delta]
url = "http://localhost:3000/events"

[on_tool_call_start]
url = "http://localhost:3000/events"

[on_tool_call_args]
url = "http://localhost:3000/events"

[on_tool_result_submitted]
url = "http://localhost:3000/events"

[on_response_complete]
url = "http://localhost:3000/events"

[on_error]
url = "http://localhost:3000/events"
```

All seven sections can route to the same endpoint or to different ones. You can
also route different event types to entirely different services.

---

## Webhook payloads

Every webhook POST delivers a JSON object with these common fields plus
event-specific fields. The `Content-Type` header is `application/json`.

### Common fields (all events)

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Event type identifier (matches the TOML section name) |
| `timestamp` | string | UTC-formatted event timestamp |

In shared mode, the proxy sends only request, completion and error events. It
reports `model` as `shared`, `finish_reason` as `complete`, and the error
`message` as `request failed`. Text, tool payloads and upstream error detail stay
in the request's owner scope.

### `on_request_received`

```json
{
  "type": "on_request_received",
  "timestamp": "2026-05-02T12:00:00Z",
  "model": "gpt-4o",
  "message_count": 3
}
```

| Field | Type | Description |
|-------|------|-------------|
| `model` | string | Model string from the incoming request |
| `message_count` | integer | Number of messages in the conversation |

### `on_text_delta`

```json
{
  "type": "on_text_delta",
  "timestamp": "2026-05-02T12:00:01Z",
  "delta": "Hello"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `delta` | string | Incremental text content chunk from the stream |

### `on_tool_call_start`

```json
{
  "type": "on_tool_call_start",
  "timestamp": "2026-05-02T12:00:01Z",
  "name": "get_weather",
  "call_id": "call_abc123"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Function name being called |
| `call_id` | string | Unique tool call identifier |

### `on_tool_call_args`

```json
{
  "type": "on_tool_call_args",
  "timestamp": "2026-05-02T12:00:01Z",
  "call_id": "call_abc123",
  "args_delta": "{\"location\": \"San"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `call_id` | string | Tool call this delta belongs to |
| `args_delta` | string | Incremental JSON arguments fragment |

### `on_tool_result_submitted`

```json
{
  "type": "on_tool_result_submitted",
  "timestamp": "2026-05-02T12:00:02Z",
  "call_id": "call_abc123"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `call_id` | string | Tool call that produced this result |

### `on_response_complete`

```json
{
  "type": "on_response_complete",
  "timestamp": "2026-05-02T12:00:03Z",
  "finish_reason": "stop"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `finish_reason` | string | `"stop"`, `"tool_calls"`, `"length"`, or `"content_filter"` |

### `on_error`

```json
{
  "type": "on_error",
  "timestamp": "2026-05-02T12:00:01Z",
  "status": 503,
  "message": "upstream model unavailable"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | integer | HTTP status code from the upstream error (e.g. 400, 500, 503) |
| `message` | string | Human-readable error detail |

---

## Operational Notes

### Fire-and-forget delivery

Hook delivery is asynchronous and fire-and-forget. Each POST is spawned on a
background Tokio task and runs independently of the proxy's response to the
client. Webhook errors — including non-2xx responses, network failures, and
timeouts — are logged at `WARN` level but are **never propagated to the caller**.
A broken webhook endpoint cannot cause client requests to fail or slow down.

### Timeout

The embedded `reqwest` HTTP client is built with a **5-second timeout** per
webhook POST. If the endpoint does not respond within 5 seconds the request is
dropped and a warning is logged.

### Delivery order

Each POST runs independently. Receivers may see events out of order even for a
single request. `call_id` correlates tool events for one call. There is no shared
request identifier in the payload for demultiplexing concurrent conversations.

### Retries

There is no built-in retry logic. If delivery reliability is required, place a
durable message queue (e.g. Redis Streams, Kafka) or an HTTP ingestion buffer
behind the webhook URL and implement retries there.

---

## Example: a local webhook viewer

The following TypeScript sketch shows a single-user browser subscriber that
connects to a server-sent-events (SSE) bridge built on top of the proxy's
webhook endpoint and renders streaming text deltas in real time.

Run this example only on loopback for a dedicated local service. The Express
bridge receives webhooks and broadcasts them to connected browsers:
1. Receives POST requests from the proxy webhook
2. Fans the events out to connected SSE clients

### Server-side SSE bridge (Node.js / Express sketch)

```typescript
import express from "express";

const app = express();
app.use(express.json());

// SSE clients waiting for events
const clients: express.Response[] = [];

// Webhook receiver — called by openai-proxy
app.post("/events", (req, res) => {
  const event = req.body;
  clients.forEach((client) => {
    client.write(`data: ${JSON.stringify(event)}\n\n`);
  });
  res.sendStatus(204);
});

// SSE stream — consumed by the browser
app.get("/stream", (req, res) => {
  res.setHeader("Content-Type", "text/event-stream");
  res.setHeader("Cache-Control", "no-cache");
  res.setHeader("Connection", "keep-alive");
  clients.push(res);
  req.on("close", () => {
    clients.splice(clients.indexOf(res), 1);
  });
});

app.listen(3000, "127.0.0.1");
```

### Browser subscriber

```typescript
interface ProxyEvent {
  type: string;
  timestamp: string;
  delta?: string;
  model?: string;
  message_count?: number;
  name?: string;
  call_id?: string;
  args_delta?: string;
  finish_reason?: string;
  status?: number;
  message?: string;
}

function connectToProxyStream(onDelta: (text: string) => void): EventSource {
  const source = new EventSource("/stream");

  source.onmessage = (event: MessageEvent) => {
    const payload: ProxyEvent = JSON.parse(event.data);

    switch (payload.type) {
      case "on_request_received":
        console.log(`Request started: model=${payload.model}`);
        break;

      case "on_text_delta":
        if (payload.delta) {
          onDelta(payload.delta);
        }
        break;

      case "on_tool_call_start":
        console.log(`Tool call started: ${payload.name} (${payload.call_id})`);
        break;

      case "on_tool_call_args":
        // Accumulate args_delta fragments per call_id if needed
        break;

      case "on_tool_result_submitted":
        console.log(`Tool result submitted for ${payload.call_id}`);
        break;

      case "on_response_complete":
        console.log(`Response complete: finish_reason=${payload.finish_reason}`);
        break;

      case "on_error":
        console.error(`Proxy error ${payload.status}: ${payload.message}`);
        break;
    }
  };

  source.onerror = () => {
    source.close();
  };

  return source;
}

// Usage
let outputText = "";
const stream = connectToProxyStream((delta) => {
  outputText += delta;
  document.getElementById("output")!.textContent = outputText;
});
```
