# KBD Current Waypoint

**Phase:** opencode-cross-integration-assessment  
**Status:** PLANNED — ready to execute  
**Change backend:** OpenSpec  
**Last updated:** 2026-05-10  

## Goal

Fix integration bugs between openai-proxy and opencode, document the built-in CodexAuthPlugin conflict, unify model catalog, align skill scoring algorithms, and prepare the plugin for npm publishing.

## Next Action

```
/kbd-execute opencode-cross-integration-assessment
```

## Execution Order

| Round | Group | Changes | Notes |
|-------|-------|---------|-------|
| 1 | A (parallel) | `proxy-bug-apikey-field`, `proxy-bug-codex-login-shape` | P0 bugs — run in parallel |
| 2 | B (parallel) | `plugin-codex-conflict-docs`, `model-catalog-unification` | After bugs fixed |
| 3 | C (single) | `skill-scoring-parity` | After `skills-selection-algorithm` (existing) applied |
| 4 | D (single) | `plugin-npm-publish` | After P0 bugs fixed |

## OpenSpec Changes (new)

- `openspec/changes/proxy-bug-apikey-field/` — fix `auth?.apiKey` type mismatch
- `openspec/changes/proxy-bug-codex-login-shape/` — validate `spawnCodexLogin()` return shape
- `openspec/changes/plugin-codex-conflict-docs/` — document built-in plugin conflict
- `openspec/changes/model-catalog-unification/` — fetch models from proxy at runtime
- `openspec/changes/skill-scoring-parity/` — IDF-weighted scoring in `src/skills.rs`
- `openspec/changes/plugin-npm-publish/` — npm packaging for `opencode-codex-proxy`

## Plan Location

`.kbd-orchestrator/phases/opencode-cross-integration-assessment/plan.md`
