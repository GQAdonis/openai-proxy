---
slug: /
title: OpenAI Proxy documentation
---

OpenAI Proxy provides a local KnowMe chat and settings console, OpenAI-compatible HTTP endpoints, and a native OpenCode 2.x plugin for models available through Codex.

- [Operate and deploy the KnowMe console](console.md), including macOS LaunchAgent updates and receipt-verified rollback.
- [Configure the OpenCode 2.x plugin](opencode-setup.md) to discover and use Codex models.
- [Manage Codex credentials](codex-auth.md) and check login reload or refresh status.
- [Configure the proxy](config.md) or consult the [ACP](acp.md), [AG-UI](ag-ui.md), and [A2A](a2a.md) references.

Build the documentation locally from the repository root with `npm ci --prefix website` and `npm run build --prefix website`. The documentation site and the Markdown guides use the same source files.
