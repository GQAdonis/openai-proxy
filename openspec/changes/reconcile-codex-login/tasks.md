# Tasks

## 1. Shared credential owner

- [ ] 1.1 AUTH-1 Implement source-aware resolution, signed-out startup, event and polling reconciliation, deployment capability detection and generation-consistent requests across direct HTTP, AG-UI, ACP stdio, MCP stdio/HTTP, models and CLI setup. Agree the scoped snapshot/status handoff with the companion SDK and settings changes.
- [ ] 1.2 AUTH-2 Integrate same-owner Codex managed refresh, deduplicate expiry/401/manual requests, verify committed freshness, suppress uncertain exchanges, and preserve limits, external token ownership, API-key selection and separate embeddings billing. Deliver redacted owner-scoped refresh and recovery status.
- [ ] 1.3 AUTH-3 Invalidate stale helpers and generations on logout, revocation and account switch. Enforce principal/workspace checks on resume and bounded safe retries. Document and expose unsupported store/transport combinations.

## Verification at the phase boundary

The phase verification owner delivers the runner at B4 after the agreed production scope is complete. Use a packaged service, disposable Codex homes, a controlled clock and issuer, and a separate synthetic login writer. Capture redacted upstream receipts including principal aliases, route, generation, request/refresh/effect counts, process identity and timestamps. Test the host-owner or dedicated credential directory deployment separately from native runs. Keep real account evidence distinct.

| Matrix ID | Positive observation | Negative control |
|---|---|---|
| AUTH-1 | `bash scripts/verify-codex-console.sh scenario AUTH-1` drives external login with the same PID, proves the post-login bearer and paired account at the receiving boundary, observes 2 s / 30 s deadlines, and covers stdio and normal CLI dispatch. | `bash scripts/verify-codex-console.sh negative AUTH-1` demonstrates unrelated, partial or uncommitted writes cannot publish an identity. A permanently stale single-file mount cannot qualify. |
| AUTH-2 | `bash scripts/verify-codex-console.sh scenario AUTH-2` verifies one managed exchange under concurrency and independently observes a fresh credential on a subsequent safe request. | `bash scripts/verify-codex-console.sh negative AUTH-2` returns success metadata with an expired credential, interrupts a possible exchange and exercises API-key, external-token, quota and unauthorized refresh controls. Assert zero false renewal, repeated grant exchange or unexpected billed request. |
| AUTH-3 | `bash scripts/verify-codex-console.sh scenario AUTH-3` delays A's refresh until B commits or logout completes, then verifies the receiving principal and the service's recovery in the same process. | `bash scripts/verify-codex-console.sh negative AUTH-3` attempts resurrection, cross-account resume and retry after a tool effect or partial stream. Assert zero stale or duplicated outbound effects. |

See the [phase task model assignments](../../../.kbd-orchestrator/phases/codex-live-auth-opencode-console/plan.md#task-model-assignments) for this change and its scoped task IDs.
