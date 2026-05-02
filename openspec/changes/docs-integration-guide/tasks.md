# Tasks: docs-integration-guide

## Status: PENDING

## Task List

- [ ] **T1** — Create `docs/hooks.md`: overview section, `hooks.toml` format reference with all 7 event sections and field definitions, AG-UI payload schema per event (JSON examples), operational notes (async fire-and-forget, error isolation, no ordering guarantees across concurrent requests), example CopilotKit/AG-UI frontend subscriber setup
- [ ] **T2** — Create `docs/a2a-integration.md`: quick-start (start proxy with `--a2a`, hit `GET /.well-known/agent.json`), full Agent Card JSON example, how an A2A orchestrator calls `chat_completion` skill, explicit scope note (no Task lifecycle / no push notifications / no artifact management), A2UI passthrough note (pass-through works by default — nothing to configure)
- [ ] **T3** — Verify both docs render correctly as Markdown (no broken links, code blocks valid)
