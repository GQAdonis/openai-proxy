---
name: proxy-opencode
description: Maintain the OpenCode 2.x integration and its compatibility with the proxy's provider, login and model catalogue.
---

# OpenCode extensions

Start with `plugin/package.json`, `plugin/src/index.ts`, `plugin/src/config.ts`, `src/cli/setup.rs`, and `opencode.json`. Identify the host binary version and the resolved plugin packages in the lockfile. The repository implements the OpenCode V2 server plugin with `@opencode/plugin`. Use [the setup guide](../../../docs/opencode-setup.md) for installation and scoped credential configuration. Verify discovery and a text turn with the real V2 host and a controlled proxy.

Use the tagged 2.3.0 documentation as the baseline. Inventory the server, TUI, experimental desktop GUI, and typed RPC exports on upgrades. Server plugins register with `Plugin.define({id, setup})` from `@opencode/plugin`. The public V2 settings include `plugins` and `providers`. Resolve the supported API for each old hook independently. Model discovery and auth belong with server provider/integration APIs. Put notifications that require a terminal UI in the TUI part. Declare the appropriate package exports for local discovery and packaged installs.

Use the upstream server reference as the coverage list for provider/model registries, credentials and integrations, agents, commands, tools, skills, MCP, plugin options, storage, references, permissions, VCS/worktrees, websearch, events, transforms and session/shell/tool hooks. Cross-process calls use the typed RPC contract. Track new or experimental facilities by version. For each surface record supported, irrelevant to this proxy, or awaiting a host test. Maintain a separate V1 entry if the project still supports V1 callers.

Acceptance includes local and packed plugin load, preserved user configuration, live account model discovery, unavailability of the proxy, and a completed interactive login reaching the credential owner. The `codex/` route carries text messages and delegates output limits to the native SDK. Keep provider tokens on the server side and avoid creating a second refresh owner in the OpenCode store.

Sources: [OpenCode 2.3.0 plugin reference](https://github.com/anomalyco/opencode/tree/v2.3.0/services/site/src/docs/content/build/plugins), [V1 migration](https://opencode.ai/docs/build/plugins/migrate-v1/), [V2 agent format](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/agents.mdx).
