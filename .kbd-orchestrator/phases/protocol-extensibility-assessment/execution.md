EXECUTION: protocol-extensibility-assessment
Project: openai-proxy
Date: 2026-05-02
Selected backend: openspec
Dispatched to: claude-code (self)
Backend rationale: OpenSpec is initialized at project root with all 3 changes scaffolded. OpenSpec provides spec-backed traceability and task-level progress for a Rust binary project with 83 integration tests as the verification gate. Claude Code is executing directly — changes are bounded multi-file Rust implementations well-suited to self-execution.
Backend entrypoint: /opsx:apply <change-id> per change, sequenced per execution rounds
OpenSpec available: YES
Source plan: .kbd-orchestrator/phases/protocol-extensibility-assessment/plan.md

---

EXECUTION SCOPE

- hooks-infrastructure: ProxyHooks trait + WebhookHooks runtime impl (src/hooks.rs, src/lib.rs, src/proxy.rs, src/main.rs, hooks.example.toml)
- a2a-agent-card: A2A Agent Card at GET /.well-known/agent.json, opt-in via --a2a flag (src/a2a.rs, src/lib.rs)
- docs-integration-guide: docs/hooks.md + docs/a2a-integration.md

---

DISPATCH CONTRACTS

- hooks-infrastructure → claude-code (self)
  Entry: Execute openspec/changes/hooks-infrastructure/tasks.md tasks T1–T9 in order
  Model class: medium
  Concrete model: claude-sonnet-4-6
  Model rationale: 9 tasks, new trait + impl, crosses src/hooks.rs → src/proxy.rs boundary — medium complexity
  Progress file: .kbd-orchestrator/phases/protocol-extensibility-assessment/progress.json
  Handoff: Update progress.json tasks_done after each task; cargo test gate before marking DONE

- a2a-agent-card → claude-code (self, parallel with hooks-infrastructure)
  Entry: Execute openspec/changes/a2a-agent-card/tasks.md tasks T1–T5 in order
  Model class: small
  Concrete model: claude-sonnet-4-6
  Model rationale: 5 tasks, new file with simple JSON response, single route wiring — low complexity
  Progress file: .kbd-orchestrator/phases/protocol-extensibility-assessment/progress.json
  Handoff: Update progress.json tasks_done after each task; cargo build gate before marking DONE

- docs-integration-guide → claude-code (self, Round 2 — after hooks-infrastructure + a2a-agent-card DONE)
  Entry: Execute openspec/changes/docs-integration-guide/tasks.md tasks T1–T3
  Model class: small
  Concrete model: claude-sonnet-4-6
  Model rationale: 3 tasks, docs-only, no code changes — low complexity; QA skipped per skip rule (docs-only)
  Progress file: .kbd-orchestrator/phases/protocol-extensibility-assessment/progress.json
  Handoff: Update progress.json tasks_done after each task; mark DONE on T3 completion

---

HANDOFF NOTE for all tools:
1. Read .kbd-orchestrator/current-waypoint.json
2. Read the change spec: openspec/changes/<change-id>/tasks.md
3. On start: update progress.json status → IN_PROGRESS, started_by → <tool>
4. On each task done: increment tasks_done, update last_task_completed + next_task_pending, commit progress.json
5. On completion: status → DONE, completed_by → <tool>; run /opsx:verify → /opsx:archive
6. On blocker: status → BLOCKED, add to blockers array, commit

---

APPROVAL GATES

- After hooks-infrastructure DONE: `cargo test --test integration` must pass all 83 tests
- After a2a-agent-card DONE: `cargo build` must succeed; manual verify `GET /.well-known/agent.json` returns valid JSON when started with `--a2a`
- docs-integration-guide: QA SKIPPED (docs-only, < 3 files modified)

---

FALLBACK CONDITIONS

- If hooks callsites in proxy.rs cause borrow-checker errors due to `&mut stream` + hook fire interaction → fall back to passing cloned event data to hooks (no reference to stream state)
- If `WebhookHooks` reqwest POSTs introduce async executor conflicts with axum → use `tokio::spawn` fire-and-forget pattern inside hook impl
- If `--a2a` flag conflicts with existing clap arg structure → use `--enable-a2a` as alternative

---

VERIFICATION REQUIREMENTS

```bash
# After hooks-infrastructure:
cargo build
cargo test --test integration -- --nocapture
# Expected: 83/83 pass, no regressions

# After a2a-agent-card:
cargo build
# Manual: start with --a2a flag, curl /.well-known/agent.json

# After docs-integration-guide:
# No build step — verify markdown renders without broken links
```

---

PROGRESS LEDGER

- [PENDING] hooks-infrastructure — claude-code
- [PENDING] a2a-agent-card — claude-code
- [PENDING] docs-integration-guide — claude-code

---

OUTPUTS

- src/hooks.rs (new)
- src/a2a.rs (new)
- hooks.example.toml (new)
- docs/hooks.md (new)
- docs/a2a-integration.md (new)
- Modified: src/lib.rs, src/proxy.rs, src/main.rs

---

BLOCKERS

- NONE

---

REFLECTION HANDOFF

kbd-reflect should consume:
- This execution.md for scope boundaries and approval gate results
- .kbd-orchestrator/phases/protocol-extensibility-assessment/progress.json for per-change completion evidence
- cargo test output (83 tests pass = hooks did not regress anything)
- Manual curl output for /.well-known/agent.json (A2A Agent Card format validation)
- docs/hooks.md and docs/a2a-integration.md for documentation completeness check

---

EXECUTION READY
