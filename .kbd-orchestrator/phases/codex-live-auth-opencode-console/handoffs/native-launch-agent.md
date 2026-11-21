# macOS launch agent deployment

On November 20, 2026, the operator chose the existing macOS launch agent as the deployment target. The replacement uses the accepted source commit `885a17a` under contract `124083a9`. A locked arm64 Cargo release build embeds the console. The versioned runtime directory contains Bun 1.4.4, the production worker dependencies, Codex CLI 0.169.0 and OpenCode SDK 2.3.0. [Deployment receipt](native-launch-agent.json)

The executable remains at `~/.local/bin/openai-proxy`. The job remains `dev.prometheusags.openai-proxy` in the logged-in user’s GUI domain. The replacement keeps the existing Codex home and the HTTP listener on port 8181. Its MCP HTTP listener binds to loopback on port 8081. The launch agent uses a dedicated console workspace and private console state directory. The OpenCode chat driver requires an approved scoped provider.

- Console at [http://127.0.0.1:8181/ui/](http://127.0.0.1:8181/ui/).
- Settings at [http://127.0.0.1:8181/ui/settings](http://127.0.0.1:8181/ui/settings).
- MCP at `http://127.0.0.1:8081/mcp`.
- To pair the local console, use the one-time code in `~/Library/Application Support/oproxy/console/pairing-token`.

Native smoke confirmed both embedded pages and their hashed script, local-admin access, denial of anonymous settings access, denial of a foreign origin, the existing managed credential, Codex SDK readiness and all four MCP tools. The first cutover restored the prior job and returned HTTP 200 after a listener assertion compared text with different address formatting. The successful retry verified the numeric listener addresses.

## Rollback

Run the retained restore script from the same macOS login session. It checks the installed executable digest, stops this launch agent, atomically restores the previous executable and plist, bootstraps the original job and verifies `/health`.

```bash
/bin/bash "$HOME/Library/Application Support/OpenAI Proxy/rollbacks/20261120T212001Z/restore.sh"
```

The backup also contains the exact previous executable and plist. Codex credentials and console data stay on disk.

## Chat selector update

The active macOS executable now builds from commit `78e19bb`. The chat selects from the active driver's model list and submits the model shown in the form. A different driver or model uses a new conversation on the next send while preserving the visible transcript and draft. The local service still requires a scoped provider to enable OpenCode chat. [Hotfix deployment receipt](native-chat-selector-hotfix.json)

To roll back this update to the first native release, run the restore script for the active executable.

```bash
/bin/bash "$HOME/Library/Application Support/OpenAI Proxy/rollbacks/20261120T213502Z/restore.sh"
```

## Native model catalog update

The active macOS executable builds from `5305903`. On November 21, 2026, the live `/v1/models` response matched all 10 model IDs in the active Codex picker. The packaged regression exercised streaming and non-streaming dispatch, shared-principal isolation, credential replacement, and static fallback with synthetic peers.

To restore the previous console selector release from this deployment, run the restore script for the active executable.

```bash
/bin/bash "$HOME/Library/Application Support/OpenAI Proxy/rollbacks/20261121T100634Z/restore.sh"
```
