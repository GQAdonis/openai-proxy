# Driver and plugin design

## Context

A provider exposed to the OpenCode host, an OpenCode SDK running as the chat driver, and the existing direct REST model endpoint serve different callers. They need distinct routing contracts even when two of them ultimately use the same model. See the [phase analysis](../../../.kbd-orchestrator/phases/codex-live-auth-opencode-console/analysis.md).

## Goals / Non-Goals

Support local-owner and shared chat, real V2 host discovery, independently attributable Pro usage, and both supervised native SDKs. Host configuration writes belong to `administer-proxy-and-local-clients`. `reconcile-codex-login` owns store resolution, login reconciliation, refresh and account invalidation. `embed-chat-and-validated-surfaces` owns assets, rendering and the AG-UI structured surface contract. This change defines the service API and runtime observations those consumers use.

## Decisions

### V2 plugin and Pro use

Pin the initial compatibility target to OpenCode 2.3.0 and `@opencode/plugin@2.3.0`. Build an ESM server entry at `.` or `./server` with `Plugin.define({ id, setup })`. Prefetch an account-scoped model list outside `ctx.provider.transform`. Register only owned entries with the OpenAI-compatible provider adapter, preserve existing provider/model overrides, and use connection-scoped inventory. Reconcile successful external/plugin login through the identity service, then call `ctx.provider.reload()` after the generation changes. A health toast, when provided, belongs in the TUI part. The server entry works without a TUI or desktop window. Verify scope and pack contents using an actual local-directory and installed-tarball host. V2 registration uses `plugins`, and any V1 compatibility entry needs its own explicit host target. [V2 plugin parts](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/build/plugins/index.mdx) · [Provider transforms](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/build/plugins/server.mdx)

Give the plugin a dedicated, OpenAI-compatible Codex destination. Bind its scoped connection to the authenticated principal and target driver, even when the console default is OpenCode. Refuse routing cycles, including endpoint aliases, by resolving an internal route identity before admission. Do not forward upstream login bearers through plugin credentials or shell hooks. For subscription mode the worker uses the selected Codex owner, controlled environment and private or host-authorized `CODEX_HOME`. Exclude inherited API keys and reject workspace or account mismatches. Use the signed-in account's actual model access. A fixture demonstrates routing, while a separately authorized highest-Pro-tier usage record demonstrates attribution. A plan label or successful text response cannot establish billing. [OpenCode ChatGPT route](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/providers.mdx#chatgpt) · [OpenAI participating integrations](https://learn.chatgpt.com/docs/sign-in-with-chatgpt)

### REST resources, ordered events and ownership

All routes authenticate the caller. Resolve workspace allowlists and identity server-side. Recheck ownership and bound account on every request, including reconnect, resume, stop and approval. Local-owner deployments use the same checks. Use opaque application IDs and keep native IDs and absolute private paths server-side.

| Method and path | Contract |
|---|---|
| `POST /api/v1/conversations` | Accept `driver` (`codex` or `opencode`), approved workspace reference and optional permitted model. Return 201 with opaque ID, effective driver/model and capability flags. |
| `GET /api/v1/conversations/{id}` | Return owner-scoped conversation metadata, current turn, state and capabilities. A follow-up on this ID resumes its bound native context. |
| `POST /api/v1/conversations/{id}/turns` | Accept prompt, client-generated idempotency key and supported options. Return 202 with turn ID after durable admission. The same key and payload return the original admission. Conflicting key reuse or an overlapping turn returns 409. |
| `GET /api/v1/conversations/{id}/turns/{turnId}/events` | Return SSE with per-turn monotonically increasing event IDs for admission, text deltas, permitted tool status, permission requests and terminal status. Honor `Last-Event-ID`. An expired cursor returns 410 with a link to owner-scoped state. |
| `GET /api/v1/conversations/{id}/turns/{turnId}` | Report current native-backed state and accumulated safe output so a client with an expired cursor can recover. |
| `POST /api/v1/conversations/{id}/turns/{turnId}/cancel` | Resolve the active native handle, accept repeated stop idempotently, and distinguish cancellation requested from cancellation confirmed. A completed race returns the existing terminal outcome. |
| `POST /api/v1/conversations/{id}/turns/{turnId}/permissions/{requestId}` | Accept a supported owner decision only for an outstanding request on this native turn. Reject stale or repeated decisions without executing further work. |

Return 401 for absent authentication, 404 for an inaccessible conversation/turn, a capability validation error for unsupported inputs, and 503 for an unavailable worker. Keep `/v1/chat/completions` compatible for existing direct callers. Durable binding includes principal, workspace, account identity, driver, native ID and last event position. Credential rotation for the same account can advance its generation. Reauthentication as a different account cannot silently resume earlier native work. Persist admission before starting native work, assign one owner to each active turn, and persist or reconcile its effects before retry. SSE disconnection detaches a subscriber. The cancel operation stops native work.

### Runtime and release

Run SDKs behind a supervised, versioned local worker transport. Authenticate and restrict access to the channel, scope workers and persistent stores to the effective principal, validate allowed workspaces, and bound concurrency, request size, event retention and shutdown time. Report worker build/version, capabilities and health without credential values. After a crash reconcile native state, expose any indeterminate turn, and release stale leases before admitting a conflicting turn. Gate enabled runtime features on packaged SDK, native executable, permissions, durable storage and release-platform support. Browser assets can still run when a driver is unavailable.

- **Codex.** Use `@openai/codex-sdk@0.169.0` with the matching approved executable, `startThread`/`resumeThread`, `runStreamed` and `AbortSignal`. Bind the ID from `thread.started`. Deduplicate cumulative assistant-item snapshots by item ID and previously emitted text. Distinguish interruptions, failures, and normal end. Choose a fail-closed execution policy and expose interactive approval only where the native adapter implements it. [Tagged Codex SDK](https://github.com/openai/codex/tree/rust-v0.169.0/sdk/typescript/src)
- **OpenCode.** Use `@opencode/sdk@2.3.0` with `OpenCode.create` and an explicit durable database. Create sessions at the approved location. Subscribe before `sessions.prompt`, filter events to the bound native session, and call `sessions.interrupt` on stop. Scope permissions to the native request and dispose the runtime on shutdown. Reject self-referential provider configuration when the embedded host resolves its model. The separate HTTP client may attach to an existing host in a future mode. [Tagged OpenCode SDK](https://github.com/anomalyco/opencode/blob/v2.3.0/packages/sdk/README.md)

### Tool ownership

The text Codex provider reports `tools: false`. A caller requiring arbitrary host-supplied functions gets an explicit unsupported response before a Codex turn starts. Apply an actual restricted inner execution policy, including workspace, native tools, network and filesystem access. A read-only workspace alone leaves other tools usable. Where OpenCode intentionally delegates, expose a named plugin tool that runs exactly one whole Codex turn. Bind the delegation ID and permissions to the initiating principal, parent turn and workspace. Report the inner tools as observations and return a single outer result. An OpenCode-owned tool workflow may select an explicitly tool-capable provider. Native command/file/MCP events already carry effects and never become new executor requests. [Codex inputs and item types](https://github.com/openai/codex/tree/rust-v0.169.0/sdk/typescript/src)

## Risks / Trade-offs

The V2 host must demonstrate it can complete an ordinary turn for a model with host tools disabled. A host which still requires tool schemas blocks certification of this route. The packaged driver must prove stream/cancel/resume on its declared OS and JS runtime. Account-level implementation or fixture evidence alone leaves Pro attribution unverified. Preserve failure details in redacted phase evidence and show resulting capability limits.

## Migration Plan

Deploy the credential principal contract, then enable versioned conversation and plugin routes for the intended tenant. Activate each driver after its worker dependencies and storage are available. Use the admin change's preview, apply and undo to install the owned V2 provider entry. Keep the direct REST surface available during rollout. Disable admission and drain or explicitly terminate native work before disabling a driver or rolling back its worker. Retain conversation mappings and event data under the normal retention policy.
