# ACP stdio Server Reference

The proxy implements [ACP (Agent Client Protocol)](https://agentclientprotocol.org) v0.11 — JSON-RPC 2.0 over stdin/stdout — enabling use as a native AI backend for editors that support ACP.

---

## Starting the ACP Server

```bash
openai-proxy serve --acp-stdio
```

The process reads JSON-RPC 2.0 from stdin and writes responses to stdout. All diagnostic logging goes to stderr.

---

## ACP Event Stream

The proxy streams `session/notification` messages as the model generates tokens:

```json
{
  "jsonrpc": "2.0",
  "method": "session/notification",
  "params": {
    "sessionId": "<uuid>",
    "update": {
      "agentMessageChunk": {
        "content": { "type": "text", "text": "Hello" }
      }
    }
  }
}
```

Streaming is incremental — chunks arrive as soon as bytes are available from the upstream backend. The full body is never buffered.

---

## Zed IDE Configuration

In Zed's `settings.json`:

```json
{
  "assistant": {
    "version": "2",
    "provider": {
      "name": "custom",
      "type": "acp",
      "command": "/path/to/openai-proxy",
      "args": ["serve", "--acp-stdio"]
    }
  }
}
```

Or with environment variable for API key:

```json
{
  "assistant": {
    "version": "2",
    "provider": {
      "name": "openai-proxy",
      "type": "acp",
      "command": "openai-proxy",
      "args": ["serve", "--acp-stdio"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

---

## JetBrains Configuration

Install the [AI Assistant plugin](https://plugins.jetbrains.com/plugin/22282-ai-assistant). In Settings → AI Assistant → Custom Providers → Add:

- **Type**: ACP
- **Command**: `/path/to/openai-proxy serve --acp-stdio`
- **Environment**: `OPENAI_API_KEY=sk-...`

---

## Protocol Details

| Method | Description |
|--------|-------------|
| `initialize` | Capability negotiation |
| `session/new` | Create a session with a working directory |
| `session/prompt` | Send a prompt, receive streamed notifications |

The proxy maps each `session/prompt` request to a single `POST /v1/chat/completions` call and streams text deltas back as `session/notification` chunks.
