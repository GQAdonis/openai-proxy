# AG-UI streams and A2UI surfaces

The proxy has a Responses-backed text stream and an authenticated stream for KnowMe console conversations. The console uses AG-UI client packages at 1.0.5 and validates surfaces against the A2UI 1.0 wire schemas. [Console operation](console.md)

## Responses-backed text stream

Send a JSON body to `POST /ag-ui/stream` with `Content-Type: application/json` and `Accept: text/event-stream`. The route uses the configured Responses API path for the local owner or authenticated shared principal. Requests accept `messages` and an optional `model`.

```json
{
  "messages": [
    { "role": "user", "content": "What is the capital of France?" }
  ]
}
```

The service uses its default model when the request omits `model`. To choose a model, look up the currently available IDs through `GET /v1/models`. Each SSE `data:` field contains one JSON event.

```text
data: {"type":"RUN_STARTED","run_id":"f47ac10b-..."}

data: {"type":"TEXT_MESSAGE_START","message_id":"9f..."}

data: {"type":"TEXT_MESSAGE_CONTENT","message_id":"9f...","delta":"Paris"}

data: {"type":"TEXT_MESSAGE_END","message_id":"9f..."}

data: {"type":"RUN_FINISHED","run_id":"f47ac10b-..."}
```

| Event | Fields | Meaning |
| --- | --- | --- |
| `RUN_STARTED` | `run_id` | Opens the text run. |
| `TEXT_MESSAGE_START` | `message_id` | Opens the assistant message. |
| `TEXT_MESSAGE_CONTENT` | `message_id`, `delta` | Adds text to the assistant message. |
| `TEXT_MESSAGE_END` | `message_id` | Closes the assistant message. |
| `RUN_FINISHED` | `run_id` | Closes the run. |

## Console AG-UI stream and A2UI 1.0

`POST /api/v1/ag-ui/stream` projects a turn that the service has already admitted. Send the authorized conversation ID as `threadId` and turn ID as `runId`. Use a valid console session with the matching origin and `X-CSRF-Token` header, or the scoped service bearer for that owner.

```json
{
  "threadId": "97e8e2be-052a-49de-ab2e-c653d4033d56",
  "runId": "c71afdd3-798c-4c1b-bfa9-33ced6dca388",
  "protocolVersion": "1.0",
  "messages": []
}
```

The stream uses camel-case event fields such as `threadId`, `runId`, and `messageId`. The service supplies the history and native permissions from the admitted turn. Terminal failures produce a `RUN_ERROR` event.

The OpenCode console worker can submit A2UI operations through its `render_surface` tool. A validated snapshot appears as `ACTIVITY_SNAPSHOT` with `activityType` set to `a2ui-surface`, `replace` set to `true`, and ordered A2UI operations in `content.a2ui_operations`. Each operation has version `1.0`. The console renders approved Basic catalog components and validates actions against the originating conversation and turn. The schemas ship with the service and browser build. [Vendored schemas and catalog](https://github.com/GQAdonis/openai-proxy/blob/main/frontend/src/schemas/README.md)
