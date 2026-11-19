# Proposal

## Why

A developer who starts the proxy currently needs a separate chat client. A shared-deployment operator has no browser view of which connection or configuration is active. The existing AG-UI stream can carry text, including JSON-looking text, but a browser needs a versioned contract to safely display an interactive surface and submit its actions.

## What Changes

- Build a React 19 and Vite 8 console from Cargo and embed it in the distributed binary and container. Serve chat and settings under the same origin at `/ui/`.
- Let users choose a Codex SDK or OpenCode SDK conversation, stream and stop turns, keep partial text, and recover without duplicating an executor or tool action.
- Present settings according to server-reported scope, source, effective value, authority and time of effect in local-owner and shared deployments.
- Add a versioned AG-UI SSE endpoint for typed text and A2UI v0.9 activity snapshots. Render approved accessible components and route allowed user actions to their originating conversation.

## Capabilities

### New Capabilities

- `embedded-chat-console` covers the packaged chat and settings interface, versioned AG-UI projection and validated A2UI renderer/action round trip.

### Modified Capabilities

## Impact

Chat-experience owns `frontend/` and the browser interaction contract. Build-release owns `build.rs`, asset embedding, `Cargo.toml`, release builds and `Dockerfile`. The protocol owner integrates the new endpoint with `src/agui.rs` and the HTTP router in `src/lib.rs`. `integrate-opencode-and-sdk-drivers` owns the runtime adapters, conversation API and replay/cancellation semantics. `administer-proxy-and-local-clients` owns administrative capabilities, authorization, configuration writers and setup API. `reconcile-codex-login` supplies the credential state presented here. The new UI and versioned protocol are additive to existing clients.
