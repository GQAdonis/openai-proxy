# Analyze decision log

**2026-11-19 · codex-live-auth-opencode-console · assessment baseline `d16c262`**

These are engineering recommendations for Spec. Earlier operator decisions establish both deployment modes, the highest ChatGPT Pro tier, both SDK chat drivers and rendered, validated A2UI. The proposed versions, timing budgets and runtime packaging await contract adoption.

| Decision | Basis and alternative considered | Evidence needed for delivery |
|---|---|---|
| Let Codex own managed renewal and observe the committed login per owner. Treat external login and token expiry as separate triggers. | Candidates 001, 002, 013. A proxy-owned grant exchange races CLI and SDK rotation. Route native managed storage to its owner; watch the containing directory for supported file stores. | Same PID, valid generation and account/profile at an independent upstream. Run concurrent expiry, logout/relogin, missed notification, keyring and container rename cases. |
| Register a pinned V2 plugin and route its subscription turns explicitly to Codex. | Candidates 003, 004, 014. Migrate V1 package and configuration. The native OpenCode authorization flow is an entitlement reference. | Actual V2 local and packed plugin loads, global install outside checkout, same-owner SDK turn and redacted evidence of supported Pro attribution. |
| Use a supervised SDK worker with separate Codex and OpenCode drivers. | Candidates 003, 005, 006. OpenCode's in-process SDK underlies this driver; the remote client remains a separate option. Driver and tool ownership are explicit for every turn. | Correct admission/stream/terminal sequence, session recovery, native cancellation, one authorized tool effect, a denied effect and cross-user isolation. |
| Embed the web build in Rust and provision agent runtimes independently. | Candidate 007. Use Cargo outputs, locked React/Vite inputs and a release asset table. The browser never imports the server SDKs. | Clean and incremental builds, deliberate frontend failure, asset/API routing and real SDK turn from a packaged distribution. |
| Use the upstream AG-UI activity contract and released v0.9 A2UI React renderer. | Candidates 008–009. A typed snapshot supports safe replay. Text interpretation, arbitrary catalogs and a v1 renderer import are excluded from the proposed compatibility profile. | Real browser reads SSE, renders and updates a surface, handles deletion/replay and authorizes one action. Malformed/foreign input leaves chat usable. |
| Share an authenticated capability/settings model across web and CLI; provision editors and tools with version-specific adapters. | Candidates 010–012, the existing Axum/ACP paths and the full settings inventory. Preserve formatting and ownership on each write. Migrate MCP wire handling after client parity. | Scoped preview/apply/reapply/undo, existing config retention, client-initiated ACP prompt and MCP tool discovery/call, effective status in both deployments. |

See [analysis](analysis.md), the [machine-readable candidates](library-candidates.json) and the [committed assessment](assessment.md) for source evidence, contract mapping and open decisions.
