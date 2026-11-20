## 1. Integrate the drivers

[Task model assignments](../../../.kbd-orchestrator/phases/codex-live-auth-opencode-console/plan.md#task-model-assignments)

- [x] 1.1 PLUGIN-1 Replace the V1 export, auth/shell/config hooks and dependency/lock entries with the pinned V2 server entry, guarded provider/catalog registration, scoped Codex destination and post-login inventory reload. Update the project V2 config and installation examples. Exercise unsupported versions without installing a V1 host or downgrading. Prepare the local/packed distribution and current-host discovery checks together with the admin change. Collect quota-denial and separately authorized Pro allowance evidence without persisting secrets.
- [x] 1.2 CHAT-1 Implement the versioned owner-scoped REST contract and supervised native drivers. Persist admission, binding and stream position. Pin the OpenCode worker to `@opencode/sdk@2.3.0`, reject an incompatible SDK before admission, translate events, propagate native stop, recover from restart, and report each driver's runtime availability. Preserve existing direct REST callers.
- [x] 1.3 TOOL-1 Implement capability validation, cycle detection, explicit Codex delegation, native permission enforcement and single execution ownership. Bind decision and delegation IDs to owner, workspace and turn.

## 2. Verification at the phase boundary

The commands below are planned entries in the shared phase verifier. Execute them at B4 against the packaged release and disposable fixtures. Retain redacted native execution, host discovery, durable session, tool-effect and denial evidence for the corresponding phase matrix row. Record actual Pro attribution separately when an owner-authorized subscriber run is available.

| Matrix row | Positive case | Negative control |
|---|---|---|
| PLUGIN-1 | `bash scripts/verify-codex-console.sh scenario PLUGIN-1` | `bash scripts/verify-codex-console.sh negative PLUGIN-1` |
| CHAT-1 | `bash scripts/verify-codex-console.sh scenario CHAT-1` | `bash scripts/verify-codex-console.sh negative CHAT-1` |
| TOOL-1 | `bash scripts/verify-codex-console.sh scenario TOOL-1` | `bash scripts/verify-codex-console.sh negative TOOL-1` |
