# Tasks: hooks-infrastructure

## Status: PENDING

## Task List

- [ ] **T1** — Add `toml` crate to `Cargo.toml` if not already present; add `reqwest` with `json` feature for webhook POSTs
- [ ] **T2** — Create `src/hooks.rs`: define `HookEvent` enum covering all 7 event types with their payload structs; define `ProxyHooks` async trait; implement `NullHooks` no-op default
- [ ] **T3** — Implement `WebhookHooks` in `src/hooks.rs`: reads `hooks.toml`, builds a URL map per event type, POSTs AG-UI-compatible JSON payloads on each `fire()` call; log errors but do not propagate to caller
- [ ] **T4** — Define `hooks.toml` schema: `[on_request_received]`, `[on_text_delta]`, `[on_tool_call_start]`, `[on_tool_call_args]`, `[on_tool_result_submitted]`, `[on_response_complete]`, `[on_error]` — each with `url` field
- [ ] **T5** — Add `hooks: Arc<dyn ProxyHooks + Send + Sync>` to `AppState` in `src/lib.rs`; initialize to `Arc::new(NullHooks)` by default in `build_app()`
- [ ] **T6** — Add `--hooks-config <path>` CLI flag in `src/main.rs`; also check `PROXY_HOOKS_CONFIG` env var; if set, load `WebhookHooks` and pass into `AppState`
- [ ] **T7** — Add 7 hook callsites in `src/proxy.rs`: `on_request_received` at handler entry, `on_text_delta` in SSE text delta loop, `on_tool_call_start` on `ResponseOutputItemAdded` for function_call, `on_tool_call_args` on `ResponseFunctionCallArgumentsDelta`, `on_tool_result_submitted` when tool results are submitted in multi-turn, `on_response_complete` on `response.completed`, `on_error` in error paths
- [ ] **T8** — Create `hooks.example.toml` at project root with all 7 sections, example webhook URLs (`http://localhost:3000/events`), and inline comments explaining each event
- [ ] **T9** — Run `cargo build` clean; run `cargo test --test integration` — all 83 tests must still pass
