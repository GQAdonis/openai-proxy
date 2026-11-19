# Live Codex identity

## Purpose

Allow a running proxy to observe the credential owner selected by a user, pick up a completed Codex login, and recover managed sessions without switching principals or billing paths.

## ADDED Requirements

### Requirement: External login becomes effective in the running proxy

The service SHALL recover from signed-out startup, reconcile the selected authoritative store after external or plugin-initiated login, and publish one complete generation to every supported consumer. Under healthy storage the observable pickup bound SHALL be 2 seconds for a delivered file-change event and 30 seconds for polling or keyring reconciliation.

#### Scenario: External login replaces the credential file

- **WHEN** a separate writer atomically replaces the selected file for the same owner with a valid new login
- **THEN** the next eligible request after the pickup bound uses the new generation's bearer, account, model view, trusted destination and wire profile, while the proxy PID stays unchanged
- **AND** HTTP chat, AG-UI, MCP stdio/HTTP and ACP observe a coherent supported route, and model discovery and fresh CLI setup agree with that route

#### Scenario: Store change without a file notification

- **WHEN** the selected keyring changes, a relevant filesystem event is lost, or a completed plugin login signals reconciliation
- **THEN** the service obtains the authorized owner's committed state within the applicable bound and reports the effective source and generation to that owner
- **AND** unrelated writes, incomplete files, unavailable stores and notification-only login attempts cannot publish a new identity

#### Scenario: Store capability includes deployment topology

- **WHEN** credentials use a custom path, encrypted keyring, auto, ephemeral, external owner or container mount
- **THEN** the service advertises only verified read, reload, refresh or native execution capabilities for the selected owner and topology
- **AND** a private host-owner connection or a dedicated directory mount must demonstrate visibility across replacement before advertising container hot reload

### Requirement: Managed refresh proves renewal and preserves billing boundaries

The service SHALL coalesce managed refresh for each principal and generation, use Codex as the grant exchanger, and distinguish a fresh or reused credential from an unverified, expired or permanently invalid one. Billing source changes SHALL require explicit authorization.

#### Scenario: Concurrent requests encounter expiry

- **WHEN** an eligible managed credential approaches expiry or returns an authentication-specific 401 while multiple eligible requests arrive
- **THEN** the service reconciles the store first, requests at most one managed exchange for that generation, and releases eligible requests using a demonstrably usable credential for the original owner
- **AND** an RPC success containing account metadata alone cannot report renewal, and a timeout after a possible exchange cannot trigger another exchange of the uncertain generation

#### Scenario: Independent credentials and provider limits

- **WHEN** a request uses a static API key, external OAuth or a subscription with an unavailable model, quota denial or unknown plan
- **THEN** the service preserves the selected identity and upstream outcome. Only the external token owner may renew an external token, and static API keys never enter managed OAuth refresh
- **AND** optional embedding requests use only an explicitly selected, authorized embeddings API key. A subscription or quota failure cannot select that key for chat or issue a paid fallback

#### Scenario: Caller requests maintenance

- **WHEN** the credential owner requests manual refresh or another principal attempts to inspect or refresh those credentials
- **THEN** the owner receives redacted status through the authorized service interface, and a cross-principal request is denied before any store or upstream effect

### Requirement: Logout and account switches supersede stale work

The service SHALL invalidate superseded credential generations for new work and retries, reject late refresh publication, and enforce the request principal through cancellation, replay and session resume.

#### Scenario: New login wins a pending refresh

- **WHEN** login B or a confirmed logout supersedes A during A's delayed refresh and its completion later arrives
- **THEN** no new request uses A after the confirmed switch or logout, the delayed result cannot resurrect A, and A's turn cannot resume or retry under B
- **AND** a confirmed logout exposes recoverable signed-out status while the proxy remains running

#### Scenario: Incomplete writes and retry safety

- **WHEN** a file temporarily cannot be parsed, an owner times out during rotation, or an upstream request has already emitted output or a tool effect
- **THEN** the service records the appropriate degraded or indeterminate status, reconciles with the same owner, and never falls through to another account or resumes an expired credential
- **AND** it permits at most one retry for a known-safe operation before any visible effect and never replays emitted content or a possibly executed tool
