# Operating the KnowMe console

Open `/ui/` on the proxy's HTTP listener for chat and `/ui/settings` for connection and administration controls. Chat runs on the selected Codex or OpenCode SDK driver. Settings shows the signed-in user's connection and the operations their role permits. ACP and MCP stdio processes use their own launch commands.

## Start a local console

Choose an existing workspace and provision an owner-only directory for console data. Keep the Codex account home, workspace, and console data in separate directories. The service account needs to read the workspace, follow login updates in the account home, and write the console data. The console directory contains private conversation records, pairing information, and configuration receipts.

```sh
install -d -m 0700 "$HOME/.local/share/oproxy/console" "$HOME/proxy-workspace"
export PROXY_CONSOLE_WORKSPACE="$HOME/proxy-workspace"
export PROXY_CONSOLE_DATA="$HOME/.local/share/oproxy/console"
export CODEX_HOME="$HOME/.codex"
export PROXY_CONSOLE_ORIGIN=http://localhost:8181
openai-proxy serve --host 127.0.0.1 --port 8181
```

Open `http://localhost:8181/ui/`. The startup log identifies the private `pairing-token` file for paired mode. Enter its current code on the sign-in screen. Pairing consumes the code, and unused codes expire after five minutes.

### Automatic sessions on a private workstation

Set `PROXY_CONSOLE_LOCAL_AUTO_SESSION=1` before starting the service on `127.0.0.1` or `::1`. Open the console directly on that listener. The browser signs in as the local administrator without an access code. A configured browser origin must use loopback and the listener's port. Use this setting on a trusted workstation without a reverse proxy. Shared deployments always require principal tokens.

The browser receives an eight-hour HttpOnly, SameSite=Strict session cookie and sends a CSRF token on mutations. Local integrations use a separate service token, supplied through `PROXY_CONSOLE_TOKEN` or initialized as the private `service-token` file under the console data directory. Upstream OAuth and API credentials remain in their server-side stores. [Credential source selection and renewal](codex-auth.md)

## Chat and connection settings

Run `codex login` for the service's approved Codex home. The service watches the credential store and reconciles changes while it runs. **Settings → My connection** shows the current credential state, generation, and available drivers. **Check for new login** requests an immediate store reread. **Refresh connection** requests managed renewal when the connection supports it.

Choose a driver and a model from that driver's list. Codex uses the authorized account's native picker, and OpenCode uses its scoped upstream. Changing the driver or model starts a new native conversation on the next send while preserving the visible transcript and draft. If a driver is unavailable, check its runtime and provider status in Settings. For `model_unavailable`, refresh the connection status and choose a model currently listed by the driver.

**Stop reply** requests native cancellation. An interrupted stream retains the transcript and can reconnect to the admitted turn. If its replay window expires, **Read saved turn** retrieves the durable outcome. Check the saved turn before reissuing a prompt whose outcome is uncertain. The console renders validated A2UI 1.0 surfaces and routes an approved surface action back to its conversation. [Structured chat stream](ag-ui.md#console-ag-ui-stream-and-a2ui-10)

## Diagnose Codex startup

To diagnose Codex startup on a private workstation, set `PROXY_CODEX_LOCAL_METRICS=1` with the loopback listener. Look for `Codex local duration` alongside `Console native turn timing` in the service log. The worker adds an owner-only `codex-startup.metrics.jsonl` in the conversation's private directory under the console data directory's `workers/` folder. It records native launch, SQLite initialization, and timings for skill discovery, persistence, instruction refresh, and MCP setup. The worker derives the thread timings in memory from allowlisted native spans received on loopback. It discards raw traces, span events, and attributes. Codex log export is disabled. The service log includes total native thread creation, credential resolution and storage, and model-cache and fetch timings. A cancelled or failed process may exit before its final metrics flush.

The console runs Codex with the conversation's private directory as its OS home and the approved Codex profile as `CODEX_HOME`. This keeps the machine user's separate `~/.agents/skills` directory out of console startup. The approved profile and workspace remain the sources for Codex state and project context.

## Update the macOS LaunchAgent

These commands operate on an existing versioned user LaunchAgent with label `dev.prometheusags.openai-proxy`. Run them from the checkout as that macOS user. The existing listener, credential wrapper, and console data settings stay in use. A user LaunchAgent dedicated to the loopback console can use `ProcessType` set to `Interactive` for browser-initiated chat. Reload the job to apply that setting.

Run a full update with the pinned Node and npm versions on `PATH`.

```sh
python3 scripts/deploy-macos-workers.py --full
```

The updater builds the committed Rust service and embedded console, then stages the locked SDK workers, platform-native Codex CLI, and OpenCode plugin using the installed Bun runtime. It points `~/.local/bin/openai-proxy` at the versioned executable. The updater preserves the verified native launcher inode with a hard link and probes that launcher before stopping the job. To upgrade the launcher deliberately, compile `workers/codex-exec.c` and pass `--launcher /absolute/path/to/codex-exec` with the executable on the same filesystem.

The updater saves `release-receipt.json` beside the prior binary and plist in the private rollback directory before cutover. On success it reports HTTP health and the receipt path in `receiptPath`. If startup fails, it restores the previous executable and plist and restarts the old service. With no flags, `python3 scripts/deploy-macos-workers.py` updates only the worker source and keeps the installed proxy binary, worker dependencies, and OpenCode plugin. [Deployment script](https://github.com/GQAdonis/openai-proxy/blob/main/scripts/deploy-macos-workers.py)

### Restore the previous release

Pass the exact `receiptPath` returned by the full deployment.

```sh
python3 scripts/deploy-macos-workers.py --rollback "/absolute/path/to/release-receipt.json"
```

The receipt must match the active full release. Before stopping it, rollback checks both binaries and plist snapshots against their digests and verifies the saved Codex launcher inode, digest, and startup. It restores the previous executable and plist. If the previous service fails its health check, rollback restores the release that was active when the command started. Older receipts require a matching verified `release-receipt.json` alongside their backups.

## Install the native drivers

For a source deployment, keep `workers/` and `plugin/` as siblings in a trusted installation directory. Use the checked-in worker entrypoints and lockfiles, preserve the executable bit on `workers/codex-exec`, and install the platform-native Codex CLI **0.169.0**. From that installation root, prepare the dependencies and point the service at absolute paths.

```sh
(cd workers && bun install --frozen-lockfile --production)
(cd plugin && bun install --frozen-lockfile --production)
export PROXY_SDK_WORKER="$PWD/workers/main.mjs"
export PROXY_SDK_BUN=/absolute/path/to/bun
export PROXY_CODEX_BINARY=/absolute/path/to/native/codex
```

Use an actual native Codex executable for `PROXY_CODEX_BINARY`. The Codex worker checks its CLI version and uses the service owner's managed ChatGPT login, read-only workspace access, and disabled shell and network tools. Its SDK is pinned to `@openai/codex-sdk@0.169.0`. For an installation without an explicit worker path the service looks under `../lib/openai-proxy/workers/main.mjs` relative to the proxy executable's directory.

OpenCode uses `@opencode/sdk@2.3.0` and `@opencode/plugin@2.3.0`. Assign it a compatible HTTPS upstream and a model the upstream serves. A local owner supplies `PROXY_OPENCODE_PROVIDER` as a JSON object such as `{"id":"team-models","baseURL":"https://models.example.org/v1"}`. If that upstream requires a bearer, supply its dedicated `apiKey` through protected service configuration. A shared deployment attaches this object to each entitled principal. Use a provider ID containing letters, digits, or hyphens. Reserve `codex` and `openai-proxy` for the proxy. The upstream must be independent of the proxy's own listener and conversation routes. Plain HTTP is accepted for a loopback upstream. The OpenCode worker can request native permission for a scoped workspace read or Codex delegation. The browser displays the requested operation for the conversation owner to allow or deny.

Driver status reports whether the required deployment files and configuration are present. Completing an actual prompt also depends on a valid login or upstream credential, network reachability, and model access. The SDK image carries the pinned CLI and Bun runtime. [OpenCode client setup](opencode-setup.md)

## Shared deployment and origins

Set `PROXY_CONSOLE_ORIGIN` to the browser's exact HTTPS origin, including its external port when nonstandard. Omit the path and trailing slash. Terminate TLS at the deployment's reverse proxy and route both `/ui/` and `/api/v1/` to the same service. An HTTPS origin enables the cookie's Secure attribute. Without an explicit origin the browser session flow requires a loopback origin matching the HTTP Host header.

Point `PROXY_CONSOLE_PRINCIPALS` at a protected JSON file containing an array of principals. Its presence selects shared mode. Give each actor a distinct high-entropy service token of 32–512 bytes and put the lowercase hexadecimal SHA-256 digest of the exact token bytes in `tokenSha256`. Distribute the original service token to that actor through your private provisioning channel. Supply unique IDs, existing absolute workspace and Codex home paths, and a role of `chat` or `admin`. Isolate all owners' workspaces and homes, including their on-disk permissions. The service process needs access to these paths. A principal's optional `openCodeProvider` binds its own upstream and credential.

```json
[
  {
    "id": "developer-one",
    "role": "chat",
    "tokenSha256": "<replace with this user's 64-character hexadecimal digest>",
    "workspace": "/srv/proxy/workspaces/developer-one",
    "codexHome": "/srv/proxy/accounts/developer-one",
    "openCodeProvider": {
      "id": "team-models",
      "baseURL": "https://models.example.org/v1"
    }
  }
]
```

Add a separately provisioned `admin` entry for service management. Restart the service after changing principal configuration. Shared users sign in with their own service token. The session determines the actor and approved workspace on every conversation and administrative request. The workspace selector's `default` label refers to that user's assigned directory.

## Settings and client setup

Every signed-in user can inspect **My connection** and export a portable client descriptor through **Client setup → My machine**. Administrators also see 16 service areas covering deployment, service configuration, identity, chat models, native drivers, OpenCode, machine configuration, ACP, MCP, tool imports, AG-UI/A2UI, A2A, skills, webhooks, memory, and process topology. Each area reports its effective values, source, scope, availability, and activation timing.

| Operation | Effect |
| --- | --- |
| Save chat defaults | Updates the default driver or model for new conversations. |
| Preview a configuration change | Displays the candidate for a writable setting. Supported fields include the listener port, backend wire format, skill injection limit, A2A enablement, memory enablement when compiled, and approved webhook destination aliases. Higher-precedence settings can mask a file value. |
| Apply or undo a configuration change | Uses the preview or receipt ID and checks the current file revision. Restart the service when the UI reports restart activation. A conflict requires a fresh preview. |
| Preview client setup on the service host | Shows the integration, project/global scope and redacted changes. Apply updates that approved host target. Undo uses the receipt and checks the installed revision. Reload the affected client to activate its registration. |
| Export setup for my machine | Downloads a descriptor for the visitor to install in their chosen local client. |

Client integrations cover the OpenCode V2 native plugin or static provider, ACP launch descriptors for Zed and JetBrains, and MCP registrations for Codex CLI, Claude Code, OpenCode V2, Zed, and VS Code/Copilot. Generic ACP and MCP descriptors support manual setup. MCP registration needs the chosen transport to be available. A host receipt records the configuration change. Confirm client discovery and a real operation in the intended client to establish runtime compatibility.

## Build and container targets

The native release embeds the Vite output in the Rust binary. Use Node 26.5.0 and npm 11.16.0. Run `npm ci --prefix frontend --include=dev`, then build with `cargo build --release --locked`. The binary serves the console from any working directory. Keep the operational workspace explicit when launching from a different directory. [Frontend build prerequisites](https://github.com/GQAdonis/openai-proxy/blob/main/frontend/README.md)

| Docker target | Contents |
| --- | --- |
| `runtime` | Rust service with embedded HTML, scripts, styles and fonts. Driver availability depends on separately provisioned native runtime files. |
| `sdk-runtime` (default) | Adds Bun, both pinned SDK packages, the sibling OpenCode plugin, and the platform-specific Codex CLI. Provision the user login and OpenCode provider at deployment. |

Compose publishes the HTTP listener on loopback. It mounts the whole Codex home at `/home/oproxy/.codex`, service data at `/home/oproxy/.local/share/oproxy`, and a separate workspace at `/home/oproxy/workspace`. Mounting the directory lets the service observe atomic login replacement and persist managed renewal. `OPROXY_CODEX_HOME` can select a dedicated private host directory accessible to the container's service user. If the external port changes, also set `PROXY_CONSOLE_ORIGIN` to the new public origin.

The retained native and OrbStack receipts record the verified source, runtime, and smoke checks for those releases. Driver and model access on another install depend on that install's credentials and provider. [Native rollout evidence](https://github.com/GQAdonis/openai-proxy/blob/main/.kbd-orchestrator/phases/codex-live-auth-opencode-console/handoffs/native-launch-agent.md) · [OrbStack staging runbook](https://github.com/GQAdonis/openai-proxy/blob/main/.kbd-orchestrator/phases/codex-live-auth-opencode-console/handoffs/staging-runbook.md)
