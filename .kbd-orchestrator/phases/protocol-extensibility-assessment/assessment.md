# KBD Assessment: Protocol Extensibility (A2A / AG-UI / A2UI)

**Phase:** protocol-extensibility-assessment  
**Date:** 2026-05-02  
**Verdict:** PARTIAL PROCEED — Hooks system YES. Protocols: selective, conditional, phased.

---

## Current State

openai-proxy is a Rust binary (~5MB) that:
- Translates OpenAI Chat Completions → Codex Responses API
- Runs in 3 modes: HTTP proxy, MCP stdio, MCP HTTP
- Has 83 integration tests passing, full tool calling support, multi-backend routing
- Has NO runtime extensibility (no hooks, no event callbacks, no config-driven behavior)

## Gap Against Proposed Goals

| Goal | Gap | Priority |
|------|-----|----------|
| Runtime hooks for skill/event firing | Missing entirely | HIGH |
| A2A Agent Card for agent discoverability | Missing | MEDIUM |
| AG-UI event stream output | Missing | LOW |
| A2UI structured UI generation | Missing (and wrong layer) | SKIP |
| Protocol-mode runtime selection | Partially present (env vars + CLI flags) | EXISTS |

## Key Findings (Research-Backed)

### A2A
- Production stable (Linux Foundation, v1.0, 150+ orgs)
- Misfit: proxy is stateless, A2A Task lifecycle assumes durable state
- **Actionable:** Agent Card only (`GET /.well-known/agent.json`) — low cost, real discoverability value
- **Avoid:** Full Task/Artifact/push-notification lifecycle

### AG-UI  
- Growing fast (9k stars, 120k weekly installs, Microsoft/Oracle production)
- Misfit: proxy has no browser frontend clients; AG-UI is a UI-layer protocol
- **Actionable:** Implement AG-UI-compatible webhook hooks internally; let upstream consumers subscribe
- **Avoid:** Native AG-UI SSE emitter built into the core binary

### A2UI
- Draft spec (v0.9, December 2025, Google)
- Wrong layer: proxy translates completions, doesn't originate UI
- **Actionable:** Pass-through already works by default; nothing to build
- **Avoid:** Any native implementation until spec reaches v1.0 (estimated 2026+)

### Hooks System (Independent of Protocols)
- Highest value item; enables all three protocols as consumers without embedding them
- Pattern: `hooks.toml` with webhook URLs per event type
- Events: `on_request_received`, `on_text_delta`, `on_tool_call_start`, `on_tool_call_args`, `on_tool_result_submitted`, `on_response_complete`, `on_error`
- Cost: 1–2 weeks
- Risk: Low — additive, opt-in, no breaking changes

## Recommended Phase Plan

### Change 1: Hooks Infrastructure (2 weeks)
- `ProxyHooks` trait with no-op default
- `WebhookHooks` impl that reads `hooks.toml` and POSTs events
- Load at startup via `--hooks-config` flag or `PROXY_HOOKS_CONFIG` env var
- Events follow AG-UI semantics (compatible but not protocol-locked)

### Change 2: A2A Agent Card (2-3 days)
- `GET /.well-known/agent.json` handler in axum
- Static JSON derived from `BackendProfile` + available models
- Skills: `chat_completion`, `list_models`
- Enable only when `--a2a` flag is set

### Change 3: Docs + Integration Guide (1 day)
- Document hooks.toml format
- Show AG-UI frontend integration via hooks
- Show A2A discovery setup
- Skip A2UI until spec stable

## What NOT to Build (Explicitly Scoped Out)

- A2A Task lifecycle / artifact management / push notifications
- Native AG-UI SSE emitter as default output format  
- A2UI component catalog or surface management
- Any form of durable task state in the proxy binary

## Files to Modify/Create

| File | Change |
|------|--------|
| `src/hooks.rs` | New — ProxyHooks trait + WebhookHooks impl |
| `src/lib.rs` | Add hooks field to AppState |
| `src/proxy.rs` | Call hooks at key points in the SSE pipeline |
| `src/main.rs` | Load hooks config at startup |
| `src/a2a.rs` | New (optional) — Agent Card endpoint |
| `hooks.example.toml` | New — example hooks config |
| `docs/hooks.md` | New — hooks documentation |
| `docs/a2a-integration.md` | New — A2A setup guide |

## Assessment Complete

`assessment_complete: true`  
`recommended_action: proceed_with_hooks_and_a2a_card_only`  
`skip: a2ui, ag-ui-native`  
`revisit: 2027-01-01 for A2UI spec v1.0, AG-UI if browser frontend use case emerges`
