# PLAN: protocol-extensibility-assessment

**Project:** openai-proxy  
**Date:** 2026-05-02  
**OpenSpec available:** YES  
**Changes to implement:** 3  
**Scope:** Hooks system + A2A Agent Card + docs. AG-UI native, A2UI, and A2A Task lifecycle are explicitly out of scope per the sycophancy-corrected assessment.

---

## CHANGE LIST (ordered)

### 1. `hooks-infrastructure` — ProxyHooks trait + WebhookHooks runtime impl
- **Scope:** api (src/hooks.rs new, src/lib.rs, src/proxy.rs, src/main.rs, hooks.example.toml)
- **Depends on:** NONE
- **Recommended agent:** Claude Code / Antigravity
- **Est. complexity:** M (3–5 hours)
- **Complexity score:** Medium
- **Model class:** medium
- **Customer value:** HIGH
- **Details:** Add a `ProxyHooks` trait with a no-op default implementation. Add `WebhookHooks` that reads a `hooks.toml` config file and POSTs AG-UI-compatible JSON payloads to configured URLs on each event. Events: `on_request_received`, `on_text_delta`, `on_tool_call_start`, `on_tool_call_args`, `on_tool_result_submitted`, `on_response_complete`, `on_error`. Load at startup via `--hooks-config <path>` flag or `PROXY_HOOKS_CONFIG` env var. No-op when not configured. Ship `hooks.example.toml` showing the format.

### 2. `a2a-agent-card` — A2A Agent Card endpoint (`GET /.well-known/agent.json`)
- **Scope:** api (src/a2a.rs new, src/lib.rs router wiring)
- **Depends on:** NONE (parallel with change 1)
- **Recommended agent:** Claude Code / Codex
- **Est. complexity:** S (1–2 hours)
- **Complexity score:** Low
- **Model class:** small
- **Customer value:** MEDIUM
- **Details:** Add `src/a2a.rs` with an axum handler for `GET /.well-known/agent.json`. Returns a static A2A Agent Card derived from `BackendProfile` + available models. Skills declared: `chat_completion`, `list_models`. Only mounted when `--a2a` CLI flag is set. No task lifecycle, no push notifications.

### 3. `docs-integration-guide` — hooks.md + a2a-integration.md
- **Scope:** docs only (docs/hooks.md, docs/a2a-integration.md)
- **Depends on:** hooks-infrastructure, a2a-agent-card
- **Recommended agent:** Claude Code / OpenCode
- **Est. complexity:** S (1 hour)
- **Complexity score:** Low
- **Model class:** small
- **Customer value:** MEDIUM
- **Details:** Write `docs/hooks.md` documenting the hooks.toml format, all event types, payload schemas, and an example AG-UI frontend integration. Write `docs/a2a-integration.md` showing A2A discovery setup with the `--a2a` flag and what orchestrators can call. Explicitly note A2UI passthrough (already works) and that native A2UI/AG-UI protocol support is deferred.

---

## EXECUTION ROUND ORDER

**Round 1 (parallel):** `hooks-infrastructure`, `a2a-agent-card`  
**Round 2 (sequential):** `docs-integration-guide` (after both Round 1 changes complete)

---

## SYCOPHANCY CHECK

- **S-02:** Assessment grounded in 83 passing tests and verified codebase state. No feasibility stretch.
- **S-07:** Scope is tighter than the original proposal — A2A Task lifecycle, native AG-UI, and A2UI all cut. Three changes is the minimum to deliver the assessed value.
- **S-03:** Trade-off surfaced: A2A Agent Card only has value if a concrete A2A orchestrator integration target exists. It is built on-demand (opt-in via `--a2a` flag), not default-on.

---

## COMMANDS TO RUN

```
/opsx:new hooks-infrastructure
/opsx:new a2a-agent-card
/opsx:new docs-integration-guide
```
