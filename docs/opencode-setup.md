# OpenCode 2.x setup

The native plugin serves the authenticated service user's Codex subscription through the dedicated `/api/v1/drivers/codex/v1` endpoint. The service validates the user's account and model access on each turn. The console's selected chat driver is independent. A Pro usage assertion requires a consenting subscriber's provider-side allowance record correlated with the turn.

## Load the plugin

The release baseline is OpenCode 2.3.0. Later stable releases below 3.0 can use the V2 interface after a host capability check. A later release requires its own discovery and turn evidence for certification. The server plugin runs in terminal and headless hosts.

Build the server entry before loading a checkout or packing a tarball.

```sh
cd plugin
bun install --frozen-lockfile
bun run build
bun pm pack
```

In a checkout, the project configuration points to `./plugin`. To install the plugin globally, provide a resolved installation directory and preview the user configuration.

```sh
export OPENAI_PROXY_PLUGIN_DIR=/absolute/path/to/plugin
openai-proxy setup opencode --global --plugin --dry-run
openai-proxy setup opencode --global --plugin
```

The resulting configuration contains the absolute plugin directory in `plugins`. You can also configure an installed versioned package or tarball through the host's package manager. Select `codex/<model>` in the host. A static provider is available through `setup opencode` without the plugin flag.

Provide a personal service credential in `PROXY_CONSOLE_TOKEN` to the OpenCode server process through its local environment or secret manager. Use `PROXY_CONSOLE_URL` to select the trusted origin. The default is `http://127.0.0.1:8181`. Remote origins require HTTPS. For a local interactive login from the V2 host, configure absolute `OPENAI_PROXY_LOGIN_HOME` and `OPENAI_PROXY_LOGIN_BINARY` paths that match the service's credential owner. The host's login command waits for the authoritative generation and stores a non-secret completion marker. For an external login, the owner runs `codex login` in its assigned Codex home. Run `/codex-reconcile` in OpenCode after the login completes to notify the service. The plugin refreshes its catalogue when the service reports a generation change. A failed or denied model list disables selection. An unavailable service returns an empty model inventory.

The provider declares text-only models. A tool-capable OpenCode session can invoke `delegate_codex` when the supervisor has bound its parent session, message and workspace and the user approves. The tool returns one text result. Native SDK tool events remain observations of actions performed by the native driver.

## Provision local clients

The CLI and authenticated administration interface use the same service-host writer. A preview reports the target, client version, owned change, export and expiry-bound identifier. Apply checks file identity, current content and effective configuration again. Save the receipt ID for undo.

```sh
openai-proxy setup opencode --project --dry-run
openai-proxy setup opencode --project
openai-proxy setup opencode --undo <receipt-id>
openai-proxy setup mcp --opencode --global --stdio
openai-proxy setup mcp --claude --codex --vscode --project --stdio
openai-proxy setup acp --zed --global
openai-proxy setup acp --jetbrains --global
openai-proxy setup acp --generic
```

Native V2 registration uses `plugins`, static models use `providers` and OpenCode MCP registrations use `mcp.servers`. A malformed, mixed or legacy effective configuration produces a blocked preview with a clean V2 export. Existing model overrides, JSONC comments and TOML formatting are preserved.

| Client | Configuration | Transport |
|---|---|---|
| OpenCode 2.x | `opencode.json(c)` in the project or global OpenCode directory | MCP stdio or HTTP |
| Claude Code | Project `.mcp.json` or user `.claude.json` | MCP stdio or HTTP |
| Codex | Project `.codex/config.toml` or the service user's Codex home | MCP stdio or HTTP |
| Zed | Project `.zed/settings.json` or user `.config/zed/settings.json` | ACP stdio and MCP stdio or HTTP |
| VS Code / Copilot | Workspace `.vscode/mcp.json` or default user profile `mcp.json` | MCP stdio or HTTP |
| JetBrains AI Assistant | User `.jetbrains/acp.json` | ACP stdio |

When the service configures a dedicated MCP bearer, supported HTTP client entries reference `PROXY_MCP_TOKEN` in the client environment. Configure Zed HTTP authentication in the client or select stdio.

## SDK workers

The supervisor launches a private child process for each admitted turn. Install Bun, the matching Codex executable and the locked worker production dependencies. Keep the `workers` directory and the sibling `plugin/src/delegation.ts` in the same relative layout. The Codex adapter selects the owner-bound home and checks the exact 0.169.0 executable. The OpenCode adapter uses the in-process 2.3.0 host, a durable per-conversation database and an explicitly configured upstream. Both drivers report native completion or interruption through a bounded private pipe.

The V2 implementation uses provider and integration transforms, scoped model inventory, command registration, permission assertion, tool registration and a request hook. TUI, desktop GUI, RPC, VCS, worktrees, agents, MCP extensions, skills and web search are outside the plugin's feature set. The actual OpenCode host remains the discovery and behavior oracle for each release.

[OpenCode 2.3.0 plugin parts](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/build/plugins/index.mdx) · [Server plugin APIs](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/build/plugins/server.mdx) · [OpenCode configuration layers](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/config.mdx) · [JetBrains ACP](https://www.jetbrains.com/help/ai-assistant/acp.html) · [Zed external agents](https://zed.dev/docs/ai/external-agents)
