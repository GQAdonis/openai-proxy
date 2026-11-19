## Context

Both a single local owner and a shared service need the same capability inventory. In a shared deployment a visitor's browser and the machine running the service can be different hosts. A stdio-only proxy process does not serve web pages.

## Goals / Non-Goals

Use a shared source of effective settings for the admin UI and setup CLI. Keep existing REST and CLI clients operable. Third-party registry publication, remote desktop discovery and arbitrary file or shell operations fall outside this change.

## Decisions

1. Place `GET /api/v1/admin/settings` and `POST /api/v1/admin/config/{preview,apply,undo}` on the authenticated management router. The chat-readable endpoint is `GET /api/v1/me/connection`. It returns only the authenticated subject’s redacted connection state, permitted driver IDs, availability, supported controls and allowed recovery actions. The server derives the subject from the session and refuses a client-supplied subject selector. The response contains no file paths, host settings, other-account identifiers, upstream tokens or shared admin capabilities. The owner can separately request credential reload or eligible managed refresh. Each descriptor reports field ID, effective and configured values (redacted), origin/override, readable and writable scope, value constraints, availability or degradation reason, and effect (per turn, live, on client reload, on service restart, or at build time). For an unavailable field retain its explanation. Do not persist an edit that the UI would misreport as effective.
2. For local owner setup, pair an authenticated browser session through an operator-controlled local channel with a short-lived, single-use secret. Use authenticated, expiring sessions and same-origin request-forgery protection. In shared mode authenticate each subject against a configured identity provider or authenticated gateway and authorize its server-assigned roles. Strip untrusted incoming identity headers. A missing trusted identity configuration leaves management inaccessible. Subscription identity, roles and workspace access are independently scoped.
3. A preview is bound to the actor, integration, service-host identity, approved project/user scope, base revision, exact redacted diff and expiry. On apply recheck the target, file identity, revision and authorization. Stage same-directory temporary files with preserved permissions, replace atomically and record only owned keys and bounded recovery metadata. For multi-target operations report committed targets and recover idempotently. Undo refuses to overwrite later user edits. Preserve JSONC comments with candidate 010 and use a formatting-preserving TOML editor where needed. Secrets are opaque references. An invalid file, unsafe path or version with unknown syntax produces a non-mutating blocked state.
4. Keep `serve --acp-stdio`, `serve --mcp-stdio` and the optional MCP HTTP port as distinct process/transport choices. Retain the project's locked ACP adapter (candidate 011). Change MCP transport using candidate 012 only after establishing version-specific parity. Native CLI registration may be used behind a typed adapter when its scope and non-interactive behavior can be proven. A generic descriptor/export works without modifying a visitor's machine.
5. Keep the documented CLI setup and admin UI on the same merge/precedence path. Write proposed OpenCode V2 plugin or static provider settings and verify the active global configuration from outside the checkout. Installation of the separate Codex marketplace plugin is presented separately from MCP registration and from REST driver selection.

## Settings inventory

| Area | What the operator can inspect or configure |
|---|---|
| Deployment and access | Local owner or shared host, authenticated actor and role, approved workspace, effective target for edits. |
| Service, logs and config | Config path/scaffold, source precedence, host/port, log filters, health, overrides, build/features and restart effects. |
| Identity and routing | Store ownership and redacted state, active account generation, reload/refresh eligibility, ChatGPT Codex/OpenAI Responses/OpenAI Chat profiles, trusted backend override and separate embeddings credential. |
| Chat and models | Model catalog, default/override, stream or buffered mode, model-specific limits and supported system/messages, sampling, stop and tool options. |
| SDK harnesses | Codex SDK, OpenCode SDK, and direct REST, native runtime status, workspace/sandbox, session/interrupt and actual tool permissions. |
| OpenCode on this host | Actual version, configuration precedence, static provider or V2 plugin, provider ID, endpoint, model and project/global scope. |
| Safe machine configuration | Actual resolved destination, redacted preview, revision-bound apply, idempotent repeat, undo and export. |
| ACP | Stdio launch of an absolute proxy executable, working directory/allowed environment, Zed and JetBrains setup, generic stdio export, supported session capabilities. |
| MCP server | Stdio or Streamable HTTP availability/binding, target client and tool list, version and verification. |
| Tool imports | Static tool schema source, collisions, discovery-only source entries and which driver executes a tool. |
| AG-UI and A2UI | Legacy text endpoint, structured endpoint and schema, validated catalog, rendering limits and errors. |
| A2A | Card discovery flag, advertised address, model/skills metadata, disabled state. |
| Skills | Search paths, manifest validation, effective injection limit, profile and message selection. |
| Webhooks | Config path, delivery state, synthetic test and redacted destination for request, text delta, tool start/args/result, completion and error. Distinguish configured from emitted events. |
| Memory and retrieval | Compile flag, enabled/degraded state, database path, embeddings model and billing/source, document create/list/delete/search and injection scope. |
| Packaging and process topology | Native/container host, runtime versions, mounts, HTTP listener, ACP/MCP exclusivity and MCP HTTP companion. Offer export if the managed target is outside the service host. |

## Risks / Trade-offs

Clients use independent schemas. Zed's ACP `agent_servers` differs from MCP `context_servers`. OpenCode V2 expects `mcp.servers.<name>`. Claude project scope uses `.mcp.json` with `mcpServers`, Codex uses `mcp_servers` in TOML or its setup CLI, and VS Code workspace scope uses `.vscode/mcp.json` under `servers`. Consult the installed client's version before writing. Shared-host path validation and the existing permissive CORS policy require server-side review.

## Migration Plan

Keep existing flag names, public routes, four MCP tools and known legacy config. Discover existing entries, display the effective layer and migrate only after an authorized preview. Preserve user overrides and the advisory behavior of MCP `set_model`. For unsupported legacy negotiation retain compatible behavior or report the precise limitation before switching the configured client.

## Verification boundary

The phase contract supplies independent oracles, controls, scratch roots and receipts. Use `bash scripts/verify-codex-console.sh scenario ID` and its `negative ID` companion from the repository root at the consolidated boundary. The runner is a future deliverable. Recalled lesson “Installer behavior tests must exercise installer dispatch” requires launching the real registered client. Gate all accepted combinations through actual clients and installed artifacts. The production implementation must be complete before harness authoring and execution.
