# Tasks

[Phase Plan and assignments](../../../.kbd-orchestrator/phases/codex-live-auth-opencode-console/plan.md) — chat-experience owns the React console and surface adapter, build-release owns Cargo embedding and final-image delivery, and the protocol owner integrates AG-UI projection and action authorization. The packaged browser and wire evidence belongs at boundary B4.

## 1. Build and serve the console

- [ ] 1.1 UI-1 — Add the `frontend/` React 19/Vite 8 package, pinned build toolchain, lockfile and explicit dependency acquisition in local, CI and Docker builder paths. Set the Vite base to `/ui/` and document the build entry point.
- [ ] 1.2 UI-1 — Add Cargo `build.rs` input tracking, fail-closed frontend build and index/reference checks. Embed generated assets from `OUT_DIR`, bind scoped Axum routes and set asset MIME, cache and navigation semantics. Preserve optional memory and protocol-only launches.

## 2. Deliver chat and settings interactions

- [ ] 2.1 UI-1 — Build the Impeccable/shadcn console and integrate the SDK owner's create-conversation, create-turn, replay and cancel endpoints. Implement keyboard operation, scroll retention, focus restoration, incremental output, unavailable driver, cancellation, partial failure and explicit retry.
- [ ] 2.2 UI-1 — Integrate the chat-readable `GET /api/v1/me/connection` and the admin owner's capability and preview/apply/undo APIs with the local-owner and shared role views. Map all sixteen inventory areas to effective value/source, host/scope, authority and timing. Cover masked settings, remote export, degraded states and redaction.

## 3. Project AG-UI events and render A2UI surfaces

- [ ] 3.1 SURFACE-1 — Register `POST /api/v1/ag-ui/stream`, validate `RunAgentInput`, project a single driver's events to typed SSE, and keep the legacy route intact. Pin the latest compatible stable `@ag-ui/client` and `@ag-ui/core` 1.0.5, use the published `@ag-ui/a2ui-middleware` 0.0.16 activity convention with catalog `protocolVersion: "1.0"`, and handle interrupted or failed runs.
- [ ] 3.2 SURFACE-1 — Pin the 0.15.0 React and web_core package roots and the v1 Basic Catalog. Validate stable `version: "1.0"` against the frozen official schema and permitted local catalog. Implement the scoped stable-wire adapter for `@accessibility` and allowed `@catalogId`, atomic bounded snapshots, accessible focus/fallback, scoped updates/deletion and idempotent replay.
- [ ] 3.3 SURFACE-1 — Validate the stable A2UI 1.0 renderer-to-agent action before the middleware projection and implement its round trip to a correlated run with server-derived identity, active component/operation authorization, CSRF/origin checks, deduplication and privilege separation.

## 4. Verify through the packaged boundary at B4

- [ ] 4.1 UI-1 — Run `bash scripts/verify-codex-console.sh scenario UI-1` on a clean release binary outside the checkout and the final image. Capture artifact ID, HTTP MIME/cache, rendered keyboard journey, both deployment modes and SDK availability separately from asset availability.
- [ ] 4.2 UI-1 — Run `bash scripts/verify-codex-console.sh negative UI-1`. Exercise a broken incremental Vite build, stale/missing assets, API fallback, unauthorized direct admin writes and partial/interrupted chat. Record expected errors, retained text and absence of side effects.
- [ ] 4.3 SURFACE-1 — Run `bash scripts/verify-codex-console.sh scenario SURFACE-1` against the packaged service with a typed AG-UI reader and the real React renderer. Record exact versions, a typed `1.0` create/update/replay/action/delete journey, accessible name and description from `@accessibility` on the actual control, retained `@` keys in user data and one independently observed action effect.
- [ ] 4.4 SURFACE-1 — Run `bash scripts/verify-codex-console.sh negative SURFACE-1`. Reject old and unknown wire versions, unsupported RPC/functions, unapproved or colliding component fields, unknown catalogs, oversized/truncated snapshots and foreign/deleted/privileged/replayed actions. Record retained state, useful fallback and zero unauthorized effects. Independently exercise the original demo text stream to prove JSON-looking text stays inert.
