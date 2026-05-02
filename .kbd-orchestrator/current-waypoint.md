# KBD Current Waypoint

**Phase:** protocol-extensibility-assessment  
**Status:** PLANNED — ready to execute  
**Change backend:** OpenSpec  
**Last updated:** 2026-05-02  

## Status: REFLECTED — Advancing to next phase

Phase `protocol-extensibility-assessment` complete. Reflection written.

## Next Action

```
/kbd-assess multi-backend-architecture
```

## Execution Order

| Round | Changes | Run |
|-------|---------|-----|
| 1 | `hooks-infrastructure`, `a2a-agent-card` | Parallel |
| 2 | `docs-integration-guide` | After Round 1 |

## OpenSpec Change Specs

- `openspec/changes/hooks-infrastructure/` — ProxyHooks trait + WebhookHooks (9 tasks, Medium)
- `openspec/changes/a2a-agent-card/` — A2A Agent Card endpoint (5 tasks, Low)
- `openspec/changes/docs-integration-guide/` — hooks.md + a2a-integration.md (3 tasks, Low)

## Scope Cuts (do not re-open)

- A2A Task lifecycle — SKIP (proxy is stateless)
- Native AG-UI SSE emitter — SKIP (wrong layer)
- A2UI native support — SKIP (spec still draft v0.9)

## Plan Location
`.kbd-orchestrator/phases/protocol-extensibility-assessment/plan.md`
