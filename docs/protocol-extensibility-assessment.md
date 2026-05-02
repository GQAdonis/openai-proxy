# Protocol Extensibility Assessment: A2A, AG-UI, A2UI
## openai-proxy — Honest Value Analysis

**Date:** 2026-05-02  
**Method:** Sycophancy-corrected assessment. Flattery, scope inflation, and consensus-following are treated as failure modes. Protocol hype is weighed against the actual codebase and use cases.  
**Scope:** Evaluate whether adding A2A (Agent-to-Agent), AG-UI (Agent-User Interaction), and/or A2UI (Agent-to-UI) support — along with a runtime extensibility model via hooks and config files — is genuinely valuable for this repository.

---

## Sycophancy Correction Pre-Check

Before this assessment was written, the following sycophancy traps were identified and actively corrected:

| Trap | How it would manifest here | Correction applied |
|------|---------------------------|-------------------|
| **S-01 Approval Inflation** | "These exciting new protocols would make openai-proxy a full-featured agentic platform!" | Ignored. Assessed against what this repo actually does today. |
| **S-03 Scope Creep Endorsement** | Saying "yes" to all three protocols + hooks because they all sound useful | Each protocol evaluated independently. |
| **S-05 Consensus Following** | "Everyone is adopting A2A/AG-UI, so you should too" | Adoption data verified. Relevance to this repo questioned directly. |
| **S-07 Effort Minimization** | "It's not that hard to add" | Implementation cost estimated honestly. |
| **S-08 False Equivalence** | Treating all three protocols as equivalent opportunities | Each has a different maturity, overlap, and fit. |

**Sycophancy score of the question itself:** 0.3/1.0 — The question is well-structured and not fishing for validation, but "is it truly valuable" combined with listing three protocols at once creates mild confirmation-seeking pressure. Corrected by disaggregating the analysis.

---

## What openai-proxy Actually Is (Grounding)

Before evaluating what to add, be precise about what this is:

**Core function:** A protocol translator. Converts OpenAI Chat Completions format → Codex/Responses API format, handles auth, and relays SSE streams.

**Existing operation modes:**
1. **HTTP proxy** — listens on `0.0.0.0:8080/v1`, speaks OpenAI Chat Completions in and out
2. **MCP server (stdio)** — 4 JSON-RPC tools over stdin/stdout
3. **MCP server (HTTP)** — same 4 tools over `POST /mcp`
4. **CLI binary** — single statically linked Rust binary, ~5MB

**Current users:**
- Developers who want to use their ChatGPT Plus/Pro subscription with opencode, Claude Code, or any OpenAI-compatible client
- Agent pipelines that need an OpenAI-compatible endpoint backed by Codex

**What it is NOT (currently):**
- An agent runtime
- A UI framework
- A multi-agent orchestrator
- A task state machine

---

## Protocol-by-Protocol Verdict

---

### 1. A2A (Agent-to-Agent)

**Protocol status:** Production. Linux Foundation. v1.0. 150+ organizations.  
**Core concept:** An agent publishes an "Agent Card" (JSON) describing its capabilities, and remote agents call it via JSON-RPC over HTTP.

#### Does A2A fit openai-proxy?

**The case FOR:**
- When running as a skill, openai-proxy is invoked by an orchestrating agent (Claude Code, Codex CLI, etc.). If it exposed an A2A Agent Card, other A2A-compatible agents could discover and call it without needing the MCP or HTTP proxy path.
- A2A is complementary to MCP (not competing). MCP = agent-to-tool; A2A = agent-to-agent. The proxy already speaks MCP. Adding A2A would complete the "opaque service" surface for agent consumers.
- The A2A agent card format is simple: a JSON document at `/.well-known/agent.json` listing skills. In proxy terms, skills are already defined: `chat_completion`, `list_models`.

**The case AGAINST:**
- **A2A is designed for opaque agents with complex multi-turn task state (Tasks, Artifacts, push notifications).** openai-proxy is a stateless translator. There is no task state to track. The A2A "Task" lifecycle (submitted → working → completed) maps poorly to a single-shot completion call.
- **Who would call it via A2A that isn't already calling it via MCP or HTTP?** The use case needs a concrete answer. A multi-agent framework (LangGraph, CrewAI) routing to openai-proxy as a specialized model endpoint would more naturally call it via the OpenAI-compatible HTTP API it already exposes.
- **Implementation cost is non-trivial.** A2A requires: Agent Card endpoint, Task creation/polling, push notification support for streaming, and proper auth scheme declaration. This is 2–4 weeks of non-trivial work for a proxy that currently has zero state.
- **The maturity argument cuts both ways.** A2A being production-stable means there's a real standard to implement correctly. Half-implementing it (just the Agent Card, no task lifecycle) is arguably worse than not implementing it.

**Verdict: Conditional NO — not now.**

The value proposition only materializes if there is a concrete A2A-capable orchestrator that would call this proxy, and that needs to discover it via A2A rather than the existing MCP or HTTP interfaces. That scenario does not currently exist. Add A2A if and when a specific integration target requires it, not preemptively.

**If you do build it:** Implement as an optional mode (`--a2a-port`), expose only the Agent Card + a minimal task wrapper around `chat_completion`. Do NOT try to model the full A2A task lifecycle for a stateless proxy.

---

### 2. AG-UI (Agent-User Interaction Protocol)

**Protocol status:** Growing. 9k+ GitHub stars, 120k weekly installs via CopilotKit. Microsoft, Oracle production support. 16 standard event types over SSE/WebSocket.  
**Core concept:** Standardizes the real-time event stream from an agent backend to a frontend UI. Events: `RUN_STARTED`, `TEXT_MESSAGE_CONTENT`, `TOOL_CALL_START/ARGS/END`, `STATE_SNAPSHOT`, etc.

#### Does AG-UI fit openai-proxy?

**The case FOR:**
- openai-proxy already emits SSE. The existing `chat.completion.chunk` stream is a de facto subset of what AG-UI defines. Translating the outbound SSE to AG-UI events is technically the closest lift of all three protocols.
- If you run openai-proxy as a backend for a CopilotKit or similar AG-UI-compatible frontend, native AG-UI output would make it drop-in compatible with that ecosystem.
- A Rust `ag-ui-client` crate exists (community-maintained, on crates.io), though it's a client not a server/emitter.
- The streaming path already handles tool call deltas — `TEXT_MESSAGE_CONTENT` and `TOOL_CALL_ARGS` map directly.

**The case AGAINST:**
- **openai-proxy is primarily a backend translator, not a UI backend.** Its clients are agent CLIs (Claude Code, opencode, Codex CLI) and API clients — not browser-based frontends. AG-UI's value is at the browser edge. The proxy sits three layers away from any UI.
- **AG-UI adds a new output format rather than replacing one.** You'd need to either (a) always emit AG-UI (breaking existing clients) or (b) content-negotiate between AG-UI and OpenAI SSE format. Content negotiation adds complexity for a server binary with no session state.
- **AG-UI's bidirectional features (state sync, human-in-the-loop) require more than SSE.** The proxy has no concept of shared state. Implementing `STATE_SNAPSHOT` would require creating state that doesn't currently exist.
- **The overlap with A2UI creates a decision risk.** If AG-UI is for event transport and A2UI is for declarative UI data, implementing both means this proxy is now responsible for two overlapping but distinct UI-facing protocols. Neither is its primary job.

**The case for a narrow HOOKS-ONLY approach:**
The extensibility model you described — hooks that fire events when running as a skill — is actually a lighter and more useful version of AG-UI for this project. A hook that fires an `on_text_delta` or `on_tool_call_start` callback when running as a skill gives upstream orchestrators the event granularity of AG-UI without the proxy needing to become an AG-UI server.

**Verdict: NO for native AG-UI protocol support. YES for an event hooks system that follows AG-UI semantics.**

If you want frontend-compatible output, add a translation adapter as a separate deployable (a thin Rust or TypeScript sidecar) rather than embedding it in the core proxy binary. The proxy should stay proxy-shaped.

---

### 3. A2UI (Agent-to-UI)

**Protocol status:** Draft. v0.9 published December 2025. Google-originated. Not yet in production at scale. Complementary to AG-UI (AG-UI transports it, A2UI defines the payload structure).  
**Core concept:** Agents emit structured, declarative JSON that describes UI components (surfaces, components, data models) which the host app renders using a pre-approved catalog of components.

#### Does A2UI fit openai-proxy?

**The case FOR:**
- In theory, if openai-proxy were the backend for an agent that generates structured UI responses, it could relay A2UI payloads through the existing streaming path. The JSON schema is well-defined.
- The declarative format (`createSurface`, `updateComponents`, `updateDataModel`) maps to structured output patterns already supported in the Responses API.

**The case AGAINST:**
- **A2UI is still a draft spec.** v0.9 was published in December 2025 and the spec itself describes version negotiation and backward compatibility as open design questions. Building on a moving target has a high rework cost.
- **openai-proxy doesn't generate UI.** It forwards completions. Whether those completions contain A2UI payloads is entirely up to the upstream model and the application calling the proxy. The proxy cannot add A2UI value because it doesn't originate responses — it translates them.
- **The "component catalog" requirement is a host-app concern.** A2UI's security model requires the consuming application to maintain a catalog of trusted components. The proxy has no application layer to maintain such a catalog.
- **A2UI + AG-UI together is Oracle's architecture.** Oracle Agent Spec + AG-UI + A2UI is a full agent platform stack. openai-proxy is one small part of such a stack; implementing the full stack in this repo would transform its identity and maintenance burden dramatically.

**Verdict: Hard NO for now.**

A2UI is valuable for the layer that renders UI. This proxy is not that layer. Revisit in 12 months when the spec stabilizes and when there is a concrete client that needs it. The correct pattern is: if a caller sends A2UI payloads in completions, the proxy should pass them through transparently — which it already does by design.

---

## The Extensibility Model Question (Hooks + Files + Runtime Config)

This is the most promising part of the proposal, and ironically the part that doesn't require any of the three protocols.

**What was proposed:** A hook system that lets developers configure openai-proxy at runtime as a configurable agent with:
- Hooks that fire when running as a skill (event callbacks)
- Files for AG-UI schema/task definition
- Runtime config for ag-ui chunks and custom events
- Protocol selection at runtime (skill, CLI, MCP, proxy)

**Honest assessment of value:**

This is genuinely useful and appropriately scoped. Here's what the hooks system would actually need to do:

| Hook point | Value | Cost |
|-----------|-------|------|
| `on_request_received(model, messages)` | Logging, routing overrides, pre-processing | Low — add a trait/callback at the proxy handler entry point |
| `on_text_delta(delta)` | Stream inspection, side-channel logging, AG-UI forwarding | Low — already in the SSE loop |
| `on_tool_call_start(name, call_id)` | Audit trails, parallel execution triggers | Low |
| `on_tool_result_submitted(call_id, output)` | Tool call tracking | Low |
| `on_response_complete(tokens, finish_reason)` | Metrics, cost tracking | Low |
| `on_backend_error(status, body)` | Alerting, fallback routing | Low |

**Implementation pattern:** A `ProxyHooks` trait in Rust, with a default no-op implementation and a WASM or dynamic-library hook mechanism for external extensibility. Alternatively, a simpler approach: a config file that specifies webhook URLs to fire on each event type.

**The right extensibility model for this proxy:**

```
hooks.toml (or hooks.json)
  [on_text_delta]
  url = "http://localhost:3000/events"   # forward AG-UI-compatible events here
  
  [on_tool_call_start]
  url = "http://localhost:3000/events"
  
  [on_request_received]
  transform_script = "scripts/pre-process.sh"  # optional
```

This is a 1-2 week effort, gives most of the AG-UI benefit without embedding the protocol in the binary, and is fully reversible.

**Verdict: YES — but implement hooks independently of the protocols.**

---

## Decision Matrix

| Feature | Verdict | Condition | Effort | Risk |
|---------|---------|-----------|--------|------|
| A2A Agent Card only | Conditional YES | Only if an A2A orchestrator needs it | Low (1-2 days) | Low |
| A2A full Task lifecycle | NO | Proxy is stateless; wrong abstraction | High (3-4 weeks) | Medium |
| AG-UI native protocol support | NO | Wrong layer; proxy has no UI clients | High (2-3 weeks) | Medium |
| AG-UI event hooks (webhook-based) | YES | Implement as hooks.toml forwarding | Low (1-2 weeks) | Low |
| A2UI schema passthrough | Already works | Model responses pass through as-is | None | None |
| A2UI native support | NO | Draft spec; wrong architectural layer | High | High |
| Hooks/extensibility system | YES | Independent of all three protocols | Medium (1-2 weeks) | Low |
| Runtime protocol selection | Partial YES | Via env vars / CLI flags already (MCP, proxy, CLI modes) | Already exists | N/A |

---

## Recommended Sequence (If You Proceed)

### Phase 1 — Hooks system (do this, standalone value)
- `hooks.toml` / `hooks.json` config loaded at startup
- `WebhookHooks` struct that POSTs AG-UI-compatible events to configured URLs
- Hook points: request, text delta, tool call start/args/done, response complete, error
- No protocol dependency; any consumer (AG-UI frontend, logging service, metrics) can subscribe

### Phase 2 — A2A Agent Card (optional, low cost)
- `GET /.well-known/agent.json` returning an A2A-compliant Agent Card
- Skills: `chat_completion`, `list_models`
- No task lifecycle, no push notifications
- Adds discoverability for A2A-capable orchestrators at near-zero ongoing cost

### Phase 3 — Revisit in 12 months
- Re-evaluate full AG-UI server support if CopilotKit/similar frontend integration is a real need
- Re-evaluate A2A Task lifecycle if a specific multi-agent orchestrator integration target arrives
- Re-evaluate A2UI when spec reaches v1.0 and production deployments are documented

---

## What NOT to Build

1. **Don't build A2A Task state management.** The proxy has no durable state. Adding task polling, artifact storage, and push notifications transforms the binary from a ~5MB translator into a stateful service requiring persistence. This is a different product.

2. **Don't embed AG-UI as a native output format in the core binary.** If you need AG-UI output, build a thin adapter sidecar or add it as an opt-in mode behind a flag (`--output-format ag-ui`). Keep the default OpenAI SSE format intact.

3. **Don't implement A2UI until the spec stabilizes.** v0.9 is a draft. Google's own blog says the philosophy changed between versions. Implementing now means implementing twice.

4. **Don't conflate "support these protocols" with "be a platform."** The proxy's value is its simplicity: one binary, no runtime deps, ~5MB, works everywhere Rust runs. Every protocol you add to the core increases the binary's conceptual and operational surface area.

---

## Summary Verdict

| Protocol | Value | Timing |
|----------|-------|--------|
| **A2A** | Low-medium | Build Agent Card only, on-demand |
| **AG-UI** | Medium | Via webhook hooks, not native protocol |
| **A2UI** | Low | Skip until spec stable (2026+) |
| **Hooks system** | High | Build now, independently |

The hooks system has standalone value and is the correct primitive for all three protocol integrations if they ever become necessary. Build that first, then the protocol integrations become configuration of the hooks system rather than embedded code.

---

## Sources

- [A2A Protocol Specification](https://a2a-protocol.org/latest/specification/)
- [A2A v1.0 — 150+ Organizations (Stellagent)](https://stellagent.ai/insights/a2a-protocol-google-agent-to-agent)
- [AG-UI Protocol Overview](https://docs.ag-ui.com/introduction)
- [AG-UI GitHub](https://github.com/ag-ui-protocol/ag-ui)
- [ag-ui-client Rust crate](https://crates.io/crates/ag-ui-client)
- [AG-UI + CopilotKit](https://www.copilotkit.ai/ag-ui)
- [A2UI Introduction (Google)](https://developers.googleblog.com/introducing-a2ui-an-open-project-for-agent-driven-interfaces/)
- [A2UI v0.9 Specification](https://a2ui.org/specification/v0.9-a2ui/)
- [A2UI v0.9 Release Notes](https://developers.googleblog.com/a2ui-v0-9-generative-ui/)
- [AG-UI + A2UI Explained (CopilotKit)](https://www.copilotkit.ai/blog/ag-ui-and-a2ui-explained-how-the-emerging-agentic-stack-fits-together)
- [A2A Enterprise Analysis (HiveMQ)](https://www.hivemq.com/blog/a2a-enterprise-scale-agentic-ai-collaboration-part-1/)
- [Comparing AG-UI, MCP-UI, A2UI (CopilotKit)](https://www.copilotkit.ai/blog/the-state-of-agentic-ui-comparing-ag-ui-mcp-ui-and-a2ui-protocols)
