# Tasks: a2a-agent-card

## Status: PENDING

## Task List

- [ ] **T1** — Create `src/a2a.rs`: define `AgentCard` struct (serde Serialize) matching the A2A v1.0 Agent Card JSON schema — fields: `name`, `description`, `url`, `version`, `capabilities` (`streaming`, `tools`, `multi_turn`), `skills` (array of `AgentSkill` with `id`, `name`, `description`), `input_modes`, `output_modes`
- [ ] **T2** — Implement `agent_card_handler` axum handler in `src/a2a.rs`: takes `State<Arc<AppState>>`, constructs the card from `AppState` (backend_profile, backend_url, default_model), returns `Json(AgentCard)` with `Content-Type: application/json`
- [ ] **T3** — Add `--a2a` boolean CLI flag in `src/main.rs` / CLI args struct
- [ ] **T4** — In `src/lib.rs` `build_app()`: conditionally mount `GET /.well-known/agent.json` → `agent_card_handler` when `--a2a` flag is set; no-op otherwise
- [ ] **T5** — Run `cargo build` clean; verify `GET /.well-known/agent.json` returns valid JSON when started with `--a2a`; run `cargo test --test integration` — all 83 tests must still pass
