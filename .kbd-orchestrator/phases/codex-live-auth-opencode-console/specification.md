# Codex live auth, OpenCode and embedded console specification

The analysis baseline is `1141700` on `main`. Four OpenSpec changes cover the 13 scenarios in the [adopted verification contract](adopted-contract.json). KBD bound `68d8aad4` as revision 2 after verifying the operator signature. The manifest supersedes revision 1 (`7bce50fc`). [Manifest](amendments/opencode-v2-a2ui-1/amend-contract.json) · [Signature](amendments/opencode-v2-a2ui-1/amend-contract.json.sig) · [Revision 2 snapshot](adopted/68d8aad4ac3a015e56d6606d8fc4d0dfb945eecf78b3df63802f21101b33dd28.json) · [Revision 1 snapshot](adopted/7bce50fc154b5d36e8090cd100f2a5fb9fb5dfa775b3a312c6e987a6b04c91cc.json)

| Change | Required outcome | Acceptance IDs |
|---|---|---|
| [Live Codex identity](../../../openspec/changes/reconcile-codex-login/specs/live-codex-identity/spec.md) · [tasks](../../../openspec/changes/reconcile-codex-login/tasks.md) | Adopt an external login without restarting, verify managed refresh, and handle logout, races, independent credentials and billing across transports. | AUTH-1, AUTH-2, AUTH-3 |
| [OpenCode plugin and SDK chat drivers](../../../openspec/changes/integrate-opencode-and-sdk-drivers/specs/sdk-chat-drivers/spec.md) · [tasks](../../../openspec/changes/integrate-opencode-and-sdk-drivers/tasks.md) | Use OpenCode 2.x APIs exclusively and load a native V2 provider with an explicitly attributable Pro turn. Run both REST drivers, preserve native session/cancellation and enforce one tool executor. | PLUGIN-1, CHAT-1, TOOL-1 |
| [Embedded console and validated surfaces](../../../openspec/changes/embed-chat-and-validated-surfaces/specs/embedded-chat-console/spec.md) · [tasks](../../../openspec/changes/embed-chat-and-validated-surfaces/tasks.md) | Build React 19/Vite 8 through Cargo, serve packaged UI at `/ui/`, and support typed AG-UI 1.0.5 and A2UI wire 1.0 rendering, replay and authorized actions. | UI-1, SURFACE-1 |
| [Proxy administration and local clients](../../../openspec/changes/administer-proxy-and-local-clients/specs/proxy-administration/spec.md) · [tasks](../../../openspec/changes/administer-proxy-and-local-clients/tasks.md) | Expose all 16 [settings areas](settings-inventory.md), preserve files during host setup, launch ACP in Zed and JetBrains and register five MCP clients. | ADMIN-1, SETUP-1, ACP-1, MCP-1, FEATURE-1 |

## Compatibility amendment

Five acceptance scenarios changed in adopted contract `68d8aad4`. The eight other scenarios, four change owners, credential pickup deadlines and verification budgets carry forward from revision one.

| Scenarios | Delivery and evidence |
|---|---|
| PLUGIN-1, CHAT-1 | Package V2 plugin and SDK dependencies exclusively. Certify the 2.3.0 host, plugin and SDK together. Exercise both native drivers and keep a separate receipt for the consenting owner's highest-tier ChatGPT Pro allowance. |
| SETUP-1, MCP-1 | Use the 2.x plugin, provider and MCP settings schemas on the service host. Detect the installed binary and config shape, preserve operator-owned files on an unsupported or mixed configuration, and offer standalone V2 export. |
| SURFACE-1 | Use released A2UI wire `1.0` with AG-UI client/core 1.0.5 and the 0.0.16 middleware convention. Select 1.0 explicitly. Validate canonical messages before adapting the renderer's older field names. Observe the accessible name and description, the scoped action and replay in the packaged browser. |

The [console design](../../../openspec/changes/embed-chat-and-validated-surfaces/design.md) records the pinned upstream schemas, package evidence and permitted field mappings. The [driver design](../../../openspec/changes/integrate-opencode-and-sdk-drivers/design.md) identifies the OpenCode V1 imports and setup paths to remove. The service's OpenAI-compatible HTTP routes and versioned admin API have independent contracts.

## Integration order and ownership

1. Establish the identity resolver, store ownership, session principals and workspace boundaries first. The identity owner changes credential use in existing HTTP/stdio paths. The admin owner supplies browser pairing or shared authentication, chat-readable `GET /api/v1/me/connection` and the restricted admin routes.
2. The SDK owner implements native conversation execution and a Codex-targeted provider connection. OpenCode provisioning uses that provider contract. Keep the direct REST route available. Only an authorized actual account observation can establish custom plugin Pro attribution.
3. The console owner owns `/ui/`, React controls, the Cargo asset pipeline, structured AG-UI projection and A2UI rendering. The admin owner supplies setting semantics, privilege policy, config writers and client adapters. Implement the shell against those interfaces, then wire in the owner-specific connection response, native drivers and protected settings.
4. For shared files such as the Axum route table, CLI setup dispatcher, Cargo manifests and Dockerfile, order integration through the runtime or build/release owner. Implement the full production surface before authoring the consolidated boundary runner. The verifier owns the single `scripts/verify-codex-console.sh` entry point and its build/run/scenario/negative/cleanup operations at B4. Every task which cites that runner contributes to the same artifact.

## Evidence and decisions

The [amended calibration examples](amendments/opencode-v2-a2ui-1/calibration-examples.md) distinguish owner-supplied outcomes from timing, compatibility and verification-budget proposals. The [fixture manifest](fixture-manifest.json) supplies reproducible non-secret inputs. Record the installed branch/release artifacts, actual client/OS versions, real versus fixture-assisted results and cleanup independently. Missing live account consent or usable attribution blocks the Pro claim. A passed document check does not certify runtime behavior. Contract adoption, implementation, behavioral acceptance, release eligibility and publication are distinct events.
