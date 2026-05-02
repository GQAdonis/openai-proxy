## Why

The hooks system and A2A Agent Card are only useful if developers know how to configure them. Both features are opt-in and config-driven, so documentation is the activation surface. Without docs, the features effectively don't exist for end users.

## What Changes

- Add `docs/hooks.md`: full reference for `hooks.toml` format, all 7 event types, AG-UI-compatible payload schemas per event, example frontend integration (CopilotKit/AG-UI subscriber), and operational notes (error handling, timeouts, ordering guarantees).
- Add `docs/a2a-integration.md`: how to start the proxy with `--a2a`, what the Agent Card returns, how an A2A orchestrator discovers and calls it, and explicit scope boundaries (no task lifecycle, no push notifications). Notes A2UI passthrough (already works by default).

## Capabilities

### New Capabilities
- `hooks-documentation`: Developer reference for the hooks system.
- `a2a-integration-guide`: Setup guide for A2A Agent Card.

## Impact

- Affected files: `docs/hooks.md` (new), `docs/a2a-integration.md` (new).
- No code changes.
- Docs-only, no risk.
