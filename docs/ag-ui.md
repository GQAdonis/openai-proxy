# AG-UI Streaming Endpoint Reference

The proxy exposes a native AG-UI (Agent-User Interaction) streaming endpoint, compatible with the [AG-UI protocol](https://github.com/ag-ui-protocol/ag-ui) and [CopilotKit](https://copilotkit.ai).

---

## What is AG-UI?

AG-UI is a lightweight protocol for streaming structured lifecycle events from an AI agent to a frontend. Instead of raw SSE text chunks, AG-UI sends typed events (`RUN_STARTED`, `TEXT_MESSAGE_START`, `TEXT_MESSAGE_CONTENT`, `TEXT_MESSAGE_END`, `RUN_FINISHED`) that frontend frameworks can map directly to UI state.

This proxy implements AG-UI natively with 5 event types. When an official Rust SDK is published ([ag-ui-protocol/ag-ui#239](https://github.com/ag-ui-protocol/ag-ui/issues/239)), these types can be replaced with the SDK's definitions.

---

## Endpoint

```
POST /ag-ui/stream
Content-Type: application/json
Accept: text/event-stream
```

---

## Request Format

```json
{
  "messages": [
    { "role": "user", "content": "What is the capital of France?" }
  ],
  "model": "gpt-5.5"
}
```

`model` is optional — defaults to the proxy's configured default model.

---

## Response: SSE Event Stream

Each event is a JSON-encoded AG-UI event prefixed with `data: `.

```
data: {"type":"RUN_STARTED","run_id":"f47ac10b-..."}

data: {"type":"TEXT_MESSAGE_START","message_id":"9f..."}

data: {"type":"TEXT_MESSAGE_CONTENT","message_id":"9f...","delta":"Paris"}

data: {"type":"TEXT_MESSAGE_CONTENT","message_id":"9f...","delta":" is"}

data: {"type":"TEXT_MESSAGE_CONTENT","message_id":"9f...","delta":" the capital"}

data: {"type":"TEXT_MESSAGE_END","message_id":"9f..."}

data: {"type":"RUN_FINISHED","run_id":"f47ac10b-..."}
```

---

## Event Type Reference

| Type | Fields | Description |
|------|--------|-------------|
| `RUN_STARTED` | `run_id` | Emitted once at the start |
| `TEXT_MESSAGE_START` | `message_id` | Assistant begins generating |
| `TEXT_MESSAGE_CONTENT` | `message_id`, `delta` | Incremental text chunk |
| `TEXT_MESSAGE_END` | `message_id` | Generation complete |
| `RUN_FINISHED` | `run_id` | Run lifecycle complete |

---

## CopilotKit Integration

```tsx
import { useCoAgent } from "@copilotkit/react-core";

const { run } = useCoAgent({
  url: "http://localhost:8080/ag-ui/stream",
});

// Trigger a run
run({ messages: [{ role: "user", content: "Hello" }] });
```

---

## Task Configuration (YAML)

The AG-UI endpoint accepts structured task requests when the request body includes a `task` key:

```json
{
  "messages": [{ "role": "user", "content": "..." }],
  "task": {
    "id": "summarize-pr",
    "description": "Summarize a pull request",
    "scope": "project"
  }
}
```

The `scope` field sets the `X-Memory-Scope` header for RAG injection (when memory is enabled). The `id` and `description` are passed through for observability.

> **Note**: Full task YAML configuration (task definitions stored in `.oproxy/tasks/*.yaml`) is planned for a future release.
