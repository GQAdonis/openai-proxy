# OpenCode 2.x setup

There are two OpenCode integrations. The external V2 plugin lets an OpenCode host use the signed-in owner's ChatGPT subscription through the proxy's Codex SDK route. The KnowMe console's OpenCode chat driver uses the in-process OpenCode SDK with its own configured upstream. Set up the path you want to use.

## Load the plugin

The verified external host baseline is OpenCode 2.3.0. The plugin accepts stable 2.x hosts from 2.3.0 onward. Verify discovery and a real turn when changing the host version. The server plugin runs in terminal and headless hosts.

Build the server entry before loading a checkout or packing a tarball.

```sh
cd plugin
bun install --frozen-lockfile
bun run build
bun pm pack
```

In this checkout, `opencode.json` adds `./plugin` to the V2 `plugins` array. To install it globally, provide an absolute path to a built plugin and preview the user configuration before applying it. [OpenCode V2 plugin configuration](https://opencode.ai/v2/docs/build/plugins)

```sh
export OPENAI_PROXY_PLUGIN_DIR=/absolute/path/to/plugin
openai-proxy setup opencode --global --plugin --dry-run
openai-proxy setup opencode --global --plugin
```

The resulting configuration lists the absolute plugin directory in `plugins`. You can also use an installed versioned plugin package. A static subscription provider is available through `setup opencode` without the plugin flag. It uses the same scoped service token and its provider ID is `openai-proxy`.

## Credentials and models

Set `PROXY_CONSOLE_URL` to the service's origin in the OpenCode server environment. The default is `http://127.0.0.1:8181`, and remote origins require HTTPS. Supply the owner's service token as `PROXY_CONSOLE_TOKEN` or set `PROXY_CONSOLE_TOKEN_FILE` to the absolute path of an owner-only regular file owned by the OpenCode process user. A local browser session and its access code are separate from this service credential. In shared mode, use the token for the principal that owns the approved Codex home. [Console session and integration tokens](console.md#start-a-local-console)

For the local OpenCode background service on macOS or Linux, use the private `service-token` file in the running proxy's console data directory. Pass its path to the managed environment. An explicit `PROXY_CONSOLE_TOKEN` takes precedence. The plugin reads the file again when it makes a request.

```sh
opencode service set env PROXY_CONSOLE_TOKEN_FILE "/absolute/path/to/console/service-token"
opencode service start
opencode models
```

The plugin reads `/api/v1/drivers/codex/v1/models` and sends subscription turns to `/api/v1/drivers/codex/v1/chat/completions`. Select one of the `codex/<model>` IDs returned for that principal. The service checks access at use time. Model availability follows the signed-in owner's Codex picker. The plugin checks connection changes periodically and reloads its catalogue when the credential generation changes. An unavailable or denied inventory yields no selectable proxy models. ChatGPT Pro attribution requires the subscriber's provider-side allowance record for the turn.

For interactive login in a local V2 host, set `OPENAI_PROXY_LOGIN_HOME` and `OPENAI_PROXY_LOGIN_BINARY` to absolute paths for the service owner's Codex profile and native executable. The host login command waits for the service to observe the new credentials. You can also run `codex login` in the assigned profile directly. The service watches that store, and `/codex-reconcile` in OpenCode requests an immediate refresh after login. [Codex credential operation](codex-auth.md)

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

The console supervisor launches a private child process for each admitted turn. Keep the installed `workers/` and `plugin/` directories as siblings because the OpenCode worker imports `plugin/src/delegation.ts`. The Codex adapter uses the owner-bound home and checks for native Codex 0.169.0. The OpenCode adapter uses the in-process SDK at 2.3.0, a durable per-conversation database, and an explicitly configured upstream. After configuring the scoped provider, select **OpenCode** in the console and choose a listed model. [Native runtime and provider setup](console.md#install-the-native-drivers)

The external V2 plugin registers providers, integration methods, commands, a request hook, and a permission-gated delegation tool. The console worker also exposes its own approved tools in scoped sessions. [Structured surface rendering](ag-ui.md#console-ag-ui-stream-and-a2ui-10)

[OpenCode 2.3.0 plugin parts](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/build/plugins/index.mdx) · [Server plugin APIs](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/build/plugins/server.mdx) · [OpenCode configuration layers](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/config.mdx) · [JetBrains ACP](https://www.jetbrains.com/help/ai-assistant/acp.html) · [Zed external agents](https://zed.dev/docs/ai/external-agents)
