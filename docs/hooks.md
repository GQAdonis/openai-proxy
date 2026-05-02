# Hooks System

openai-proxy ships a webhook-based hooks system that fires AG-UI-compatible JSON
payloads to configured HTTP endpoints at key points in the proxy request lifecycle.
Use it to observe streaming events, feed token deltas into AG-UI-compatible
frontends, trigger parallel skill execution, audit tool calls, or route events
to any custom observability pipeline — all without modifying or forking the proxy
binary.

Hooks are **opt-in**: if no config file is provided the proxy uses a no-op
`NullHooks` implementation and behaves identically to an unhoooked installation.

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
| CLI flag | `openai-proxy --hooks-config /etc/proxy/hooks.toml` |
| Environment variable | `PROXY_HOOKS_CONFIG=/etc/proxy/hooks.toml openai-proxy` |

The file is read once at startup. Changes require a proxy restart.

---

## `hooks.toml` Format Reference

Each section configures a single event type. Omitting a section silences that
event — no HTTP POST is made for it.

### `[on_request_received]`

Fired at the entry point of every chat completions request.

```toml
[on_request_received]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_text_delta]`

Fired for every text delta emitted during a streaming response.

```toml
[on_text_delta]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_tool_call_start]`

Fired when the model initiates a new tool call.

```toml
[on_tool_call_start]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_tool_call_args]`

Fired for each incremental chunk of tool call arguments (streaming JSON fragments).

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

Fired when the model signals it has finished generating a response.

```toml
[on_response_complete]
url = "http://localhost:3000/events"
```

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | HTTP endpoint that receives the POST |

### `[on_error]`

Fired when the proxy encounters an upstream error or model-not-available condition.

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

## AG-UI Payload Schema

Every webhook POST delivers a JSON object with these common fields plus
event-specific fields. The `Content-Type` header is `application/json`.

### Common fields (all events)

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Event type identifier (matches the TOML section name) |
| `timestamp` | string | ISO 8601 UTC timestamp of when the event was fired |

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

### No ordering guarantees across concurrent requests

Hooks for a single request are fired in order: `on_request_received` →
`on_text_delta` (repeated) / `on_tool_call_*` → `on_response_complete` or
`on_error`. However, when multiple requests are being processed concurrently
their hook events may be interleaved at the webhook endpoint in arbitrary order.
If your endpoint must demultiplex concurrent conversations, correlate events
using the `call_id` field on tool events or add your own correlation ID at the
application layer.

### Retries

There is no built-in retry logic. If delivery reliability is required, place a
durable message queue (e.g. Redis Streams, Kafka) or an HTTP ingestion buffer
behind the webhook URL and implement retries there.

---

## Example: AG-UI Frontend Integration

The following TypeScript example shows a minimal browser subscriber that
connects to a server-sent-events (SSE) bridge built on top of the proxy's
webhook endpoint and renders streaming text deltas in real time.

This pattern assumes you have a small server-side SSE bridge (e.g. a Next.js
Route Handler or an Express endpoint) that:
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

app.listen(3000);
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

### AG-UI CopilotKit integration

If you are using [CopilotKit](https://docs.copilotkit.ai/), you can wire the
proxy's webhook payloads directly into a `useCoAgent` or custom `useCopilotChat`
integration by mapping the event types above to CopilotKit's AG-UI message
envelope:

```typescript
// Map a proxy hook event to an AG-UI-style message
function toAgUiMessage(event: ProxyEvent) {
  if (event.type === "on_text_delta" && event.delta) {
    return { type: "TEXT_MESSAGE_CHUNK", content: event.delta };
  }
  if (event.type === "on_tool_call_start") {
    return { type: "TOOL_CALL_START", toolCallId: event.call_id, toolName: event.name };
  }
  if (event.type === "on_response_complete") {
    return { type: "RUN_FINISHED" };
  }
  return null;
}
```

The proxy's payloads are intentionally shaped to match AG-UI semantics, so
adaptation is straightforward.
