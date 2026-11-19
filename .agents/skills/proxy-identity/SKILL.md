---
name: proxy-identity
description: Design and maintain credential loading, managed OAuth refresh, Codex SDK integration and hot reload after login.
---

# Codex identity and refresh

Inspect `src/codex.rs`, `src/main.rs`, `AppState` in `src/lib.rs`, `src/mcp.rs` and `plugin/src/codex-login.ts`. Today the proxy selects a backend at startup and several callers read a cloned credential. Route callers through a shared credential provider. Treat this document as a target design.

1. Resolve the explicit auth path and the documented Codex home and credential store. Distinguish managed ChatGPT login, externally managed OAuth, and static API keys. Use the Codex SDK for thread/turn execution when that is the requested integration. For account management verify the installed App Server wire schema with `codex app-server generate-ts --out <temporary-directory>`.
2. For managed login, initialize a local App Server session over stdio against the same Codex home/store. On impending expiry or an authentication-specific 401, have one refresh owner call `account/read` with `refreshToken: true`, then reload the committed credential from its authoritative store. The response carries account metadata. Leave refresh-token rotation and persistence to Codex. API keys do not refresh. An externally supplied token needs its owning identity provider or host to refresh it. The external-token refresh RPC is a server-to-client request.
3. Watch the credential file's **parent directory**, filtering the resolved filename. Coalesce create, write and atomic rename notifications. Re-read after a successful plugin login and on App Server account/login notifications. A separate CLI process may emit no notification to this service. Reconcile periodically and on request after a bounded interval to cover lost events, changed keyring entries, replacement directories and container mounts. Treat a transient incomplete write separately from confirmed logout, revocation, deletion and an account change.
4. Publish one snapshot of bearer, account identity, routing profile and generation. Use the runtime owner's explicit backend policy when credential kind changes. Keep readers on a consistent generation. Coalesce refreshes per identity, reread before refresh, discard stale results after a later login or logout, and bound retries with backoff. Classify invalid grants as requiring interactive sign-in. Retry a request only when safe before effects or stream output; never replay emitted content or tools.
5. Expose only redacted state and refresh outcomes. Require server-side operator authorization for manual refresh and account-specific isolation in shared mode. Never use account data from one principal for another. Coordinate all transports and the plugin so login does not require restarting the service.

Verify with a fake clock, fake App Server and temporary credential store. Exercise expiry under concurrency, a newer login racing slow refresh, same-path rename, keyring update, external login, partial write, logout, irrevocable expiry, API-key mode and admin authorization. Keep production credentials out of fixtures and diagnostics.

Sources: [Codex SDK](https://developers.openai.com/codex/sdk/), [App Server account API](https://developers.openai.com/codex/app-server/), [credential storage](https://developers.openai.com/codex/auth/), [account schema](https://github.com/openai/codex/blob/main/codex-rs/app-server-protocol/src/protocol/v2/account.rs).
