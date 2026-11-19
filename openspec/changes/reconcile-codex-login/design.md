# Design

## Context

`src/main.rs` resolves credentials once and exits early for MCP or ACP stdio. The typed Rust loader accepts nested and flat file formats, tries the direct OS keyring after a file failure and can then use an environment API key. The plugin and one-shot setup independently inspect top-level JSON fields. An access token without its account ID can currently select the ChatGPT route even when the bearer builder falls back to an API key. The embeddings store retains an API key copied at startup. The single-file read-only Compose mount can keep the old inode after a host login replaces the file. The runtime image currently lacks a Codex executable.

## Goals / Non-Goals

Deliver prompt external-login pickup, verifiable managed refresh, isolated principals, recoverable signed-out startup, and a redacted status contract for the console. Leave SDK turn execution, entitlement certification, frontend rendering and client config writing to their companion changes.

## Decisions

### Resolve and publish one identity

Start a resolver before any server-mode dispatch. Capture one immutable snapshot per request containing the server-derived principal, credential mode/source, upstream account, generation, credential handle, trusted destination, wire profile and model view. The generation identifies an observed effective login change. Status includes capability, effective source, last reconciliation, last successful renewal and retry state. Keep secrets, raw headers and opaque account identifiers out of generic health, MCP diagnostics, browser payloads and logs.

For the normal Codex-managed source, resolve the explicit `--auth-path` or `CODEX_AUTH_PATH`, then the selected Codex home, then the default home. Use the home/store the Codex process actually owns. A custom filename may support read-only reconciliation. Advertise managed refresh only if the writer's target is demonstrably that same store. Keep a deliberately configured API-key connection available as its own mode. Deny a malformed OAuth token/account pair before profile selection. A custom upstream must be explicitly authorized for that credential class and principal. Missing files, permission failures, a 429 and missing model entitlement cannot authorize another billing destination.

### Observe the authoritative source

| Mode | Reconciliation and delivery |
|---|---|
| File | Watch the resolved parent directory and filter on the credential entry. Debounce create/write/rename/remove, recover after directory replacement and periodically reread. Preserve supported legacy, nested and uppercase API-key shapes. |
| Keyring or auto | Read through the selected owner's supported storage interface. Apply Codex auto-store precedence and distinguish unavailable secure storage from a confirmed account change. A stale file is insufficient proof of ownership. Include the direct keyring and Codex's encrypted-secrets keyring backend in capability discovery. |
| Ephemeral or externally supplied OAuth | Keep credentials within their owning process or external integration. Use an authorized runtime for that principal when available. A different OS process cannot recover an ephemeral login from a file. Report unsupported direct forwarding if no authorized reader exists. |
| Container | For a host login, provide an authenticated, principal-bound host owner connection. For a container-owned login, provision Codex and a dedicated writable credential directory visible across renames. A host single-file read-only bind cannot qualify as external-login or managed-refresh support. |

Treat notifications, including the plugin's successful login callback, as hints. A complete authoritative read determines publication. Under healthy local storage, bound file-event pickup to 2 seconds and periodic/keyring pickup to 30 seconds. Reopening a parent watch, filtering unrelated writes and handling a partial file must work without restarting the proxy. Each host process owns its own reconciliation loop, including stdio-only servers. Keep helper IPC separate from protocol stdout.

### Delegate managed rotation to Codex

Adapt phase candidate 001, Codex CLI/App Server 0.169.0, for managed refresh. Adopt candidate 002, notify 8.2.2 plus Tokio, for file hints and deadline-based reconciliation. Candidate 003, the matching TypeScript SDK, reopens the CLI for a turn and must use the same scoped home. It is a companion consumer of this contract. Candidate 013, a second OAuth grant exchanger in the proxy, would introduce an independent rotation writer. Keep the grant exchange and persistence in the Codex owner. [Candidate decisions](../../../.kbd-orchestrator/phases/codex-live-auth-opencode-console/library-candidates.json)

Coalesce concurrent expiry, eligible 401 and owner-authorized manual refresh for each principal and observed generation. Re-read first. A newly usable credential can satisfy queued callers. Otherwise ask the owning App Server for managed refresh and reconcile its durable result. Its `account/read` success can contain cached metadata after a failed refresh. Establish freshness from a new usable same-owner generation or independently observed successful authenticated use of that credential. Distinguish reuse from demonstrated renewal. Emit bounded retry/backoff for transient failures and require a new login on confirmed permanent invalidation. [Codex account read](https://github.com/openai/codex/blob/rust-v0.169.0/codex-rs/app-server/src/request_processors/account_processor/workspace_routing.rs#L259-L294)

A long-lived App Server guards refresh by the account it already has. Retire or rebind it after an externally committed owner switch. Its network exchange precedes locked compare-and-save. A service-local lock cannot serialize a separate CLI process. Reread after conflicts and treat timeouts/crashes following a possible exchange as indeterminate. Suppress another exchange of that generation until the owner establishes the outcome or a new authorized login supersedes it. Stop using confirmed revoked, logged-out or expired credentials. [Guarded refresh](https://github.com/openai/codex/blob/rust-v0.169.0/codex-rs/login/src/auth/manager.rs#L3455-L3517) · [Exchange and persistence](https://github.com/openai/codex/blob/rust-v0.169.0/codex-rs/login/src/auth/manager.rs#L3759-L3789)

### Integrate at actual request boundaries

HTTP streaming and buffered requests, MCP stdio/HTTP tools, ACP and AG-UI must build the outbound request and its headers from the same snapshot. A profile unsupported by a transport returns an explicit compatibility error. Use the same resolver for model visibility and one-shot `setup opencode`. MCP status requires scoped redaction. Bind SDK starts/resumes to the invoking principal and approved workspace. Use a separately selected API key only for explicitly enabled API-billed embeddings. A ChatGPT plan string, including `promax`, is informational.

## Risks / Trade-offs

Filesystem events may be dropped. Polling the owner recovers missed changes at bounded cost. Storage or issuer outages may make freshness unknowable. Represent unavailable or pending status and preserve tenant isolation. Learn from the earlier packaged-artifact and installer-dispatch lessons: exercise real `serve` modes and actual `setup opencode` with disposable stores at the phase boundary. Record redacted observations, process identity and upstream counters so a restarted process, changed UI badge or mocked helper alone cannot pass.

## Library decisions

- **library: cand-001** — adapt the Codex 0.169.0 managed credential owner. Re-read the actual store under the active account and generation, and verify the receiving backend accepts a refreshed credential. An account metadata response can survive an unsuccessful refresh. [Tagged account processor](https://github.com/openai/codex/blob/rust-v0.169.0/codex-rs/app-server/src/request_processors/account_processor/workspace_routing.rs#L259-L294)
- **library: cand-002** — use notify 8.2.2 on the parent directory and a separate Tokio reconciliation timer to cover atomic replacement, missed events and stores without file notifications. [notify documentation](https://docs.rs/notify/8.2.2/notify/)
- **cand-013** — reject an independent proxy OAuth exchange. Keep one native grant writer per owner, including after an uncertain response.
