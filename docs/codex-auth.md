# Codex connection ownership

Each proxy credential provider follows one selected store. HTTP chat, AG-UI, ACP prompts and MCP requests capture the account, bearer, upstream and wire format together. A login change applies to subsequent requests. A signed-out service can stay running while its owner logs in.

## Select the source

For a Codex login, select the credential file with `serve --auth-path` or `CODEX_AUTH_PATH`. Otherwise, the proxy uses `$CODEX_HOME/auth.json`, with `~/.codex` as the default home. A nonstandard filename supports direct forwarding and reconciliation. A complete OAuth credential includes both the bearer and upstream account. The proxy also understands legacy flat files, nested `tokens`, and file-based API keys.

Select a separate environment API key for chat by setting **both** `PROXY_CHAT_AUTH=api_key` and `OPENAI_API_KEY`. The proxy pins the billing class after the first complete credential. Changing the selected billing class requires an operator configuration change and restart. API-key chat uses the Responses API by default, or Chat Completions with `CODEX_WIRE_API=chat`. AG-UI and ACP require a Responses route. When memory is enabled, `PROXY_EMBEDDINGS_API_KEY` is a distinct opt-in to billed embeddings for a dedicated local service. Shared services run memory without that operator key.

| Selected store | Direct forwarding and reconciliation | Managed renewal |
| --- | --- | --- |
| Standard Codex file | Parent-directory events plus periodic reads | Native Codex helper and writable matching home |
| Direct OS keyring | Periodic reads of the selected owner's entry | Native Codex helper and matching home |
| Auto with direct OS keyring | Keyring first, then file on a confirmed absent entry. An inaccessible keyring suspends forwarding. | Same as direct keyring |
| Custom file or externally supplied OAuth | Complete file credentials only | Renewal belongs to the external owner |
| Encrypted secrets keyring or process-local ephemeral store | Direct forwarding unavailable | Use the credential owner's supported native client |
| A single bind-mounted credential file | Unsupported | Use a dedicated directory that exposes atomic replacement |

For container-owned credentials, provision the pinned Codex executable in the image and mount a private writable directory at `CODEX_HOME`. Perform login in that directory. A host operating-system keyring and a container belong to separate credential owners.

## Reconciliation and renewal

The proxy watches the selected credential's parent and reconciles relevant writes, create/remove and rename events after a short debounce. A 20-second timer reopens the watch and checks stores without file notifications. Requests check for missed updates when the last observation is older than 900 milliseconds. Incomplete files and unavailable stores suspend forwarding until a complete authoritative read. Confirmed removal signs the owner out. The plugin and authenticated settings can request an immediate reread.

For a managed OAuth login approaching token expiry, a recognized authentication failure or an authorized manual request, the proxy first rereads the store and coalesces work for the credential generation. It initializes the same owner's Codex App Server over private stdio and requests `account/read` with `refreshToken: true`. Codex performs the grant exchange and persistence. The proxy checks the committed bearer and its expiry or subsequent authenticated use. A success reply containing only account metadata cannot establish renewal.

Managed renewal supports Codex CLI **0.169.0**. The service checks the selected executable's version in an empty private directory and reports `refreshRuntime` with the authorized connection status. Set `PROXY_CODEX_BINARY` to an absolute executable path, or install `codex` on the service's PATH. Direct forwarding can use a currently valid credential independently of helper availability. The helper receives an isolated environment, the selected Codex home, a private temporary directory and a restricted executable search path. On Linux, direct and auto keyring renewal require a same-owner Unix session bus under the private `XDG_RUNTIME_DIR`. Other broker topologies require renewal by the native login owner.

Before requesting a grant exchange, the helper must identify the configured home in its initialization response. The proxy rereads the committed generation immediately before dispatch and retires a helper when the observed owner changes. App Server frames are bounded to 256 KiB and each response wait has a deadline.

An opaque, generation-specific receipt in `$CODEX_HOME/.openai-proxy-refresh/` prevents a second proxy process from repeating a possibly dispatched exchange. A missing helper or pre-dispatch failure backs off for 30 seconds. An uncertain exchange suspends further rotation of that credential. Complete a fresh login if the store cannot establish recovery. An eligible upstream rejection can retry once with a newer credential for the same owner and destination before stream output. Requests carrying tool state are never replayed.

Default destinations are fixed to the matching OpenAI credential class. A custom `CODEX_BACKEND_URL` requires the operator to set `PROXY_BACKEND_CREDENTIAL_CLASS` to `api_key` or `subscription`. Subscription overrides are limited to explicitly trusted loopback endpoints. The HTTP client disables redirects.

## Shared access and diagnostics

Authenticated REST requests use their server-assigned credential provider and memory scope. Each shared principal needs a separate Codex home and workspace. Standalone ACP and MCP processes require a dedicated owner. MCP HTTP listens on loopback. On a multi-user host, set `PROXY_MCP_TOKEN` to a private token of 32–512 bytes and supply it as the local client’s bearer. Operator settings return the connection mode, source, observed state, generation, timestamps, refresh capability and retry delay. A private binding record under the console data directory keeps sessions associated with an owner across restarts. An observed logout or account switch invalidates the prior binding.

The native writer and storage layouts follow [Codex App Server 0.169.0](https://github.com/openai/codex/blob/rust-v0.169.0/codex-rs/app-server/README.md) and the [tagged credential store](https://github.com/openai/codex/blob/rust-v0.169.0/codex-rs/login/src/auth/storage.rs).
