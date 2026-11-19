# Proposal

## Why

A developer can complete `codex login` while the proxy continues using the old account. Startup captures credentials, backend URL and wire profile separately. HTTP, MCP, ACP, AG-UI, model listing and the setup CLI can consequently disagree about which identity is in use. Expiry currently interrupts the session and a restart can conceal the underlying problem.

## What Changes

- Resolve credentials by owner and source, publish a single consistent generation, and pick up external login or logout in each long-lived server mode.
- Let Codex renew its own managed ChatGPT credentials. Detect when the new credential is usable, coalesce requests, and report recovery or sign-in-required status to the authorized owner.
- Preserve explicit routing and billing choices across reload. Give embeddings a separately authorized API-key path. Scope status and administrative actions to the caller's authority.
- Support host-managed credentials and container deployments with an explicit owner and a directory that exposes atomic replacement.

## Capabilities

### New Capabilities

- `live-codex-identity` covers external login pickup, managed renewal and safe identity transitions across service consumers.

### Modified Capabilities

None.

## Impact

The identity and runtime roles own the resolver and its integration with `src/main.rs`, `src/lib.rs`, `src/codex.rs`, the HTTP and stdio adapters, model discovery, CLI setup and optional memory embeddings. Build/release owns the Compose mount and helper provisioning. The change depends on a trusted Codex runtime or an explicitly supported external owner for each offered mode. It provides the owner/generation, redacted status and reload contracts to `integrate-opencode-and-sdk-drivers`, `embed-chat-and-validated-surfaces` and `administer-proxy-and-local-clients`. Those changes own plugin registration, chat turns, UI and client setup. The phase's real-account evidence is separate from fixture-based identity verification.
