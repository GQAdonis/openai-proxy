# A2A Integration Guide

A2A (Agent-to-Agent) is a Linux Foundation production standard adopted by 150+
organizations. It defines a common protocol that lets AI agents discover and
interact with each other through a well-known Agent Card document.

openai-proxy can expose an A2A Agent Card at `GET /.well-known/agent.json`.
The card describes the proxy's identity, capabilities, and available skills so
that any A2A-capable orchestrator can discover and call it without manual
configuration.

This is an **opt-in** feature gated behind the `--a2a` CLI flag. The route is
not mounted by default.

---

## Quick Start

```bash
# Start the proxy with A2A discovery enabled
openai-proxy --a2a

# Fetch the Agent Card
curl http://localhost:8080/.well-known/agent.json
```

The proxy returns the Agent Card JSON immediately. No authentication is required
for the discovery endpoint.

---

## Agent Card Response

`GET /.well-known/agent.json` returns a JSON object with the following shape.
The `url` field reflects the `--bind` address of the running proxy instance.

```json
{
  "name": "openai-proxy",
  "description": "OpenAI Chat Completions proxy backed by Codex/Responses API",
  "url": "http://localhost:8080",
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

### Field Reference

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Always `"openai-proxy"` |
| `description` | string | Human-readable description of the proxy |
| `url` | string | Base URL of the running proxy (from `--bind`) |
| `version` | string | Crate version from `Cargo.toml` at build time |
| `capabilities.streaming` | boolean | Always `true` — proxy supports SSE streaming |
| `capabilities.tools` | boolean | Always `true` — proxy forwards tool/function calls |
| `capabilities.multi_turn` | boolean | Always `true` — proxy is stateless but supports multi-turn messages in a single request |
| `skills` | array | Skills this proxy exposes (see below) |
| `input_modes` | array | Always `["text"]` |
| `output_modes` | array | Always `["text", "stream"]` |

### Skills

| Skill ID | Name | Description |
|----------|------|-------------|
| `chat_completion` | Chat Completion | Calls `POST /v1/chat/completions` on the proxy |
| `list_models` | List Models | Calls `GET /v1/models` on the proxy |

---

## How an A2A Orchestrator Uses This

### 1. Discover the agent

The orchestrator reads the Agent Card from the well-known URL:

```bash
GET /.well-known/agent.json HTTP/1.1
Host: localhost:8080
```

### 2. Identify available skills

From the card the orchestrator learns that this agent supports `chat_completion`
and `list_models`. It also learns that the agent supports streaming output and
tool use.

### 3. Call `chat_completion`

The orchestrator sends an OpenAI-compatible chat completions request to the
`url` from the card:

```bash
POST /v1/chat/completions HTTP/1.1
Host: localhost:8080
Content-Type: application/json
Authorization: Bearer <your-key>

{
  "model": "gpt-4o",
  "messages": [
    { "role": "user", "content": "Summarize the latest news about Rust." }
  ],
  "stream": true
}
```

The proxy forwards the request to the configured upstream backend (Codex,
Responses API, or another OpenAI-compatible endpoint) and streams the response
back to the orchestrator.

### 4. Call `list_models`

To discover which models are available on the configured backend:

```bash
GET /v1/models HTTP/1.1
Host: localhost:8080
Authorization: Bearer <your-key>
```

### TypeScript orchestrator example

```typescript
interface AgentCard {
  name: string;
  description: string;
  url: string;
  version: string;
  capabilities: {
    streaming: boolean;
    tools: boolean;
    multi_turn: boolean;
  };
  skills: Array<{ id: string; name: string; description: string }>;
  input_modes: string[];
  output_modes: string[];
}

async function discoverAndChat(
  agentBaseUrl: string,
  userMessage: string,
  apiKey: string,
): Promise<void> {
  // Step 1: discover
  const cardRes = await fetch(`${agentBaseUrl}/.well-known/agent.json`);
  const card: AgentCard = await cardRes.json();

  const hasChatSkill = card.skills.some((s) => s.id === "chat_completion");
  if (!hasChatSkill) {
    throw new Error("Agent does not expose chat_completion skill");
  }

  // Step 2: call chat_completion skill
  const completionRes = await fetch(`${card.url}/v1/chat/completions`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: "gpt-4o",
      messages: [{ role: "user", content: userMessage }],
    }),
  });

  const data = await completionRes.json();
  console.log(data.choices[0].message.content);
}
```

---

## Scope and Limitations

This implementation exposes **Agent Card discovery only**. The following A2A
protocol features are explicitly **out of scope** for this proxy:

- **A2A Task lifecycle** — no `tasks/send`, `tasks/get`, `tasks/cancel`
- **Push notifications** — no webhook callbacks from the agent to the orchestrator
- **Artifact management** — no file or binary artifact exchange
- **Agent authentication** — no A2A-specific auth; use the proxy's standard
  `Authorization: Bearer` header as with any OpenAI-compatible client

The proxy is stateless. It does not track conversation IDs or task state between
requests.

---

## A2UI Passthrough

If a model returns A2UI-formatted JSON in its completion content (for example,
an assistant message whose content is a structured A2UI envelope), the proxy
passes it through transparently. No special configuration is needed. The proxy
does not inspect or transform completion content — whatever the upstream model
returns is forwarded to the caller unchanged.

---

## MCP and A2A Together

openai-proxy also exposes an MCP server endpoint. A2A and MCP can be enabled
simultaneously:

```bash
openai-proxy --a2a --mcp
```

An orchestrator that supports both protocols can use the MCP endpoint for tool
orchestration while using the A2A Agent Card for discoverability.
