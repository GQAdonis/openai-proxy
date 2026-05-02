## Why

A2A (Agent-to-Agent) is a Linux Foundation production standard adopted by 150+ organizations. openai-proxy already exposes skills (`chat_completion`, `list_models`) via MCP. Exposing an A2A Agent Card at `GET /.well-known/agent.json` adds discoverability for A2A-capable orchestrators at near-zero cost — it's a static JSON document derived from already-known data. The full A2A Task lifecycle is explicitly out of scope (proxy is stateless).

## What Changes

- Add `src/a2a.rs` with an axum handler for `GET /.well-known/agent.json` returning a valid A2A Agent Card.
- Card content: name, description, version, URL (from bind address), capabilities (streaming: true, tools: true), skills: `chat_completion` + `list_models`, input_modes: ["text"], output_modes: ["text", "stream"].
- Mount route only when `--a2a` CLI flag is present (opt-in, not default).
- Wire into `src/lib.rs` router inside the `--a2a` conditional.

## Capabilities

### New Capabilities
- `a2a-agent-card`: A2A-compliant Agent Card endpoint for agent discoverability.

### Modified Capabilities
- None — additive only, gated behind `--a2a` flag.

## Impact

- Affected files: `src/a2a.rs` (new), `src/lib.rs` (route wiring).
- No new dependencies.
- Backward compatible: disabled by default.
- Does NOT implement: A2A Task endpoints, push notifications, artifact management.
