## 1. Effective configuration and provisioning

[Task model assignments](../../../.kbd-orchestrator/phases/codex-live-auth-opencode-console/plan.md#task-model-assignments)

- [x] 1.1 ADMIN-1 Implement the chat-readable `GET /api/v1/me/connection` contract, scoped admin authentication, redacted effective-setting descriptors, source precedence and paired local/shared authorization. Populate all 16 settings areas in the embedded settings shell.
- [x] 1.2 SETUP-1 Implement V2-only OpenCode global/project discovery and the shared CLI/UI version and schema check. Replace V1-shaped configuration writes, preserve unsupported/mixed targets untouched, and provide V2 export, JSONC/TOML-preserving preview/apply/undo and checked concurrency using candidate 010.
- [x] 1.3 ACP-1 Implement Zed and JetBrains ACP launch adapters, generic export, protocol stdout isolation, actual stream/error reporting and truthful capabilities using candidate 011.
- [x] 1.4 MCP-1 Provision the five named MCP clients by version/transport. Reuse SETUP-1's V2 guard for OpenCode and write `mcp.servers.<name>`. Preserve four tools and check the existing wire baseline before introducing candidate 012.
- [x] 1.5 FEATURE-1 Connect effective skills, tool import, hook, discovery, memory and packaging settings to their real consumers with per-owner authorization and correct activation status.

## 2. Whole-phase verification

- [ ] 2.1 After production implementation of all four changes, contribute the settings/client cases to the single phase boundary runner. Exercise both deployment modes and frozen positive and negative controls, retaining originals and cleanup receipts.
