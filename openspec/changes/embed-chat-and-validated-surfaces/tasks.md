# Tasks

## 1. Build and serve the console

- [ ] 1.1 UI-1 — Add the `frontend/` React 19/Vite 8 package, pinned build toolchain, lockfile and explicit dependency acquisition in local, CI and Docker builder paths. Set the Vite base to `/ui/` and document the build entry point.
- [ ] 1.2 UI-1 — Add Cargo `build.rs` input tracking, fail-closed frontend build and index/reference checks. Embed generated assets from `OUT_DIR`, bind scoped Axum routes and set asset MIME, cache and navigation semantics. Preserve optional memory and protocol-only launches.

## 2. Deliver chat and settings interactions

- [ ] 2.1 UI-1 — Build the Impeccable/shadcn console and integrate the SDK owner's create-conversation, create-turn, replay and cancel endpoints. Implement keyboard operation, scroll retention, focus restoration, incremental output, unavailable driver, cancellation, partial failure and explicit retry.
- [ ] 2.2 UI-1 — Integrate the chat-readable `GET /api/v1/me/connection` and the admin owner's capability and preview/apply/undo APIs with the local-owner and shared role views. Map all sixteen inventory areas to effective value/source, host/scope, authority and timing. Cover masked settings, remote export, degraded states and redaction.

## 3. Project AG-UI events and render A2UI surfaces

- [ ] 3.1 SURFACE-1 — Register `POST /api/v1/ag-ui/stream`, validate `RunAgentInput`, project a single driver's events to typed SSE, and keep the legacy route intact. Pin the matching AG-UI client/core pair and handle interrupted or failed runs.
- [ ] 3.2 SURFACE-1 — Pin the v0.9 renderer/processor exports. Implement atomic bounded replacement snapshots with stable identity, local catalog/schema validation, accessible focus and text fallback, scoped update/deletion and idempotent replay.
- [ ] 3.3 SURFACE-1 — Implement the validated v0.9 action round trip to a correlated run with server-derived identity, active component/operation authorization, CSRF/origin checks, deduplication and privilege separation.

## 4. Verify through the packaged boundary at B4

- [ ] 4.1 UI-1 — Run `bash scripts/verify-codex-console.sh scenario UI-1` on a clean release binary outside the checkout and the final image. Capture artifact ID, HTTP MIME/cache, rendered keyboard journey, both deployment modes and SDK availability separately from asset availability.
- [ ] 4.2 UI-1 — Run `bash scripts/verify-codex-console.sh negative UI-1`. Exercise a broken incremental Vite build, stale/missing assets, API fallback, unauthorized direct admin writes and partial/interrupted chat. Record expected errors, retained text and absence of side effects.
- [ ] 4.3 SURFACE-1 — Run `bash scripts/verify-codex-console.sh scenario SURFACE-1` against the packaged service with a typed AG-UI reader and the real React renderer. Record an accessible create/update/replay/action/delete journey and one independently observed action effect.
- [ ] 4.4 SURFACE-1 — Run `bash scripts/verify-codex-console.sh negative SURFACE-1`. Deliver unsupported, oversized and truncated snapshots, replayed actions, foreign or deleted surfaces, JSON-looking text and attempted admin actions. Record the retained safe state, error/terminal behavior and zero unauthorized effects.
