# Operating the embedded console

Open `/ui/` on the proxy's HTTP listener. Chat runs on the selected native driver. Settings shows the signed-in user's connection and the operations their role permits. ACP and MCP stdio processes use their own launch commands. Use the HTTP service for the browser console.

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

1. Open `http://localhost:8181/ui/`. The startup log identifies the private `pairing-token` file. Read the current value from that file and enter it on the sign-in screen.
2. Pairing consumes the code. Codes expire after five minutes. Settings → My connection shows the observed credential state, generation, and available driver controls. Use **Check for new login** after completing Codex login for the service owner. **Refresh connection** requests a managed renewal when the service offers it.
3. Select an available driver and model, then send a message. **Stop reply** requests native cancellation. An interrupted stream retains the transcript and can reconnect to the admitted turn. If its replay window expires, **Read saved turn** retrieves the durable outcome. An uncertain outcome warrants checking the saved turn before submitting another message.

The browser receives an eight-hour HttpOnly, SameSite=Strict session cookie and sends a CSRF token on mutations. Sign out ends that session. Local integrations use a separate service token, supplied through `PROXY_CONSOLE_TOKEN` or initialized as the private `service-token` file under the console data directory. Upstream OAuth and API credentials belong in the selected server-side store. [Credential source selection and renewal](codex-auth.md)

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

The native release embeds the Vite output in the Rust binary. After preparing the pinned frontend dependencies, build with `cargo build --release --locked`. Once built, the binary serves the console from any working directory. Keep the operational workspace explicit when launching from a different directory. [Frontend build prerequisites](../frontend/README.md)

| Docker target | Contents |
| --- | --- |
| `runtime` | Rust service with embedded HTML, scripts, styles and fonts. Driver availability depends on separately provisioned native runtime files. |
| `sdk-runtime` (default) | Adds Bun, both pinned SDK packages, the sibling OpenCode plugin, and the platform-specific Codex CLI. Provision the user login and OpenCode provider at deployment. |

Compose publishes the HTTP listener on loopback. It mounts the whole Codex home at `/home/oproxy/.codex`, service data at `/home/oproxy/.local/share/oproxy`, and a separate workspace at `/home/oproxy/workspace`. Mounting the directory lets the service observe atomic login replacement and persist managed renewal. `OPROXY_CODEX_HOME` can select a dedicated private host directory accessible to the container's service user. If the external port changes, also set `PROXY_CONSOLE_ORIGIN` to the new public origin.

**Verification status.** This guide describes the implemented configuration and source contracts. Release builds, container launches, real SDK turns, client discovery, and browser acceptance remain pending at the signed verification boundary.
