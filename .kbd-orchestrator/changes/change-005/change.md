# change-005: `CODEX_DEFAULT_MODEL` env var support
**Status:** [ ] pending  
**Agent:** rust-reviewer  
**Files:** `src/main.rs`, `src/codex.rs`, `.env.example`

## Goal
Allow clients to set a preferred default model via `CODEX_DEFAULT_MODEL` environment variable. Stored in `AppState` and used in `codex::convert_request` as a model override hint.

## Tasks
- [ ] Add `default_model: Option<String>` to `AppState`
- [ ] Read `std::env::var("CODEX_DEFAULT_MODEL").ok()` in main.rs, store in state
- [ ] Pass `state.default_model.as_deref()` into `codex::convert_request`
- [ ] In `convert_request`: if `default_model.is_some()` and client sent a generic model name (gpt-4o, gpt-3.5-turbo), use `default_model` instead of `map_model()` output
- [ ] Add `# CODEX_DEFAULT_MODEL=codex-mini` to `.env.example` with comment

## Acceptance
- `CODEX_DEFAULT_MODEL=codex-mini openai-proxy` routes all "gpt-4o" requests to "codex-mini"
- Explicit model names (gpt-5.3-codex, codex-mini) still pass through unchanged
