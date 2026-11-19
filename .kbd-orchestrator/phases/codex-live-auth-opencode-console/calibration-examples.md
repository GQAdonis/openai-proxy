# Owner examples and proposed calibration

## Outcomes supplied by the project owner

- An external `codex login` changes the identity used by the running proxy, and eligible expired managed credentials refresh without restarting the service.
- A consenting user with OpenAI's highest ChatGPT Pro tier can use the OpenCode 2.x plugin through Codex. Codex SDK and OpenCode SDK are both selectable REST chat drivers.
- Local-owner and shared deployments both use the embedded React 19/Vite 8 chat and settings application.
- “Also render validated A2UI surfaces in chat.” A plain-text rendering of the JSON does not satisfy this example.
- Settings cover the service's modes, configure OpenCode on the service host, set up ACP for Zed and other clients, and register the service as an MCP server for tools on that machine.

## Proposed checks for those examples

The source is the committed [analysis](analysis.md), [inventory](settings-inventory.md) and [scenario candidates](scenario-candidates.md). This section records the team's calibration for owner adoption. It does not record a subscriber login, billing consent, a product pass, or authorization to change an actual workstation.

| Topic | Positive example | Rejection or rare critical example |
|---|---|---|
| Login and refresh | A healthy file-backed store publishes the new login within 2 seconds. Keyring reconciliation or a missed event recovers within 30 seconds. Measure from authoritative commit/return of login to an independently observed request under the new account. | An old refresh completing after logout cannot resurrect the old account. A metadata-only success cannot prove token renewal. |
| Subscription and tools | An explicitly authorized subscriber obtains a turn through the installed plugin and can identify its allowance. One permitted native tool creates one identifiable effect. | A generic completion, plan string or zero-cost UI label cannot prove billing. A denial cannot silently switch to API-key billing or another executor. |
| Clients and modes | Reloadable proxy auth works in native and container/host-owner deployments. Zed and JetBrains launch ACP. Codex, Claude Code, OpenCode, Zed and VS Code/Copilot discover and invoke MCP in their declared configurations. | A global setup observed only inside this project, a preview that truncates JSONC, or a valid file without an actual client round trip fails acceptance. |
| Chat and settings | The packaged UI renders a validated v0.9 surface, behaves correctly under replay and sends one authorized action in the originating conversation. The effective settings inventory covers all 16 areas. | An unknown catalog, text that resembles a surface, or a forged cross-user action cannot create privileged effects. A chat user's direct management request must be rejected. |

## Proposed boundary conditions

- Compatibility baseline for the SDK-enabled package uses Codex CLI/SDK 0.169.0 and OpenCode host/SDK/plugin 2.3.0. Verify a native macOS host and a Linux release container. Capture actual editor/CLI versions in each receipt. Record each client/transport result separately. Another platform or an untested release is unverified.
- Local operation uses an operator-paired session. Shared operation needs a trusted, server-verified identity source and separate admin roles. A subscription remains personal to the authenticated credential owner. Account-bound provider calls require separate consent and a dedicated test profile.
- Proposed consolidated verification budget is 240 minutes, 240 tool invocations, two repairs, and a checkpoint after three attempts without progress. Preserve evidence and consumed budget on retry. No charge, deployment or release authorization is implied.
- The future boundary runner is `bash scripts/verify-codex-console.sh` in the checkout. `build`, `run`, `scenario ID`, `negative ID` and `cleanup` are a specified CLI contract to implement at the whole-phase verification boundary. Capture package, host and fixture hashes, resource cleanup and each real/fixture distinction before claiming acceptance.
