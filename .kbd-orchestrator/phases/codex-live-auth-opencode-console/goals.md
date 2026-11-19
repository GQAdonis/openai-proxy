# Goals

- A running proxy adopts a completed external codex login for the next eligible request, and manages expired Codex credentials without requiring a service restart.
- Integrate the Codex SDK with an OpenCode 2.x plugin for a consenting user with OpenAI’s highest ChatGPT Pro tier. Use the account’s actual model access and limits.
- Serve React 19 and Vite 8 chat and settings from the Rust binary, built through build.rs, for local developers and shared deployments with authorized administrators.

## Team kickoff

Baseline is setup commit `374ee5e`. The identity, OpenCode extensions and chat experience roles inspected their respective entry points. This brief records candidate acceptance scenarios for assessment. Implementation and runtime verification follow the phase contract.

| Owner | Current behavior | First observable outcome |
|---|---|---|
| Codex identity and proxy runtime | `src/main.rs` loads a credential at startup. `AppState` holds the bearer, account identity and routing fields. HTTP and MCP handlers consult those fields. The TypeScript plugin has its own auth-file handling. | With the service PID unchanged, replace a fixture credential the way an external login does. The next eligible request after reconciliation uses the new bearer, matching account header and configured backend. |
| OpenCode extensions | `plugin/src/index.ts` uses V1 hooks. The installed V2 host is 2.3.0. The plugin’s successful login currently passes bearer data back into the host auth flow. | Load both a local and a packaged V2 plugin in the real host. Preserve user provider overrides, discover the provider/models, and stream a turn via a test double at the Codex SDK boundary. |
| Chat experience and build/release | The Axum service exposes streams and model discovery. The checkout has no chat frontend or Cargo build script. The container build copies Rust sources. | Use the packaged service from an empty working directory to open chat and settings, send and stop a streamed message, and observe authorized refresh status. No frontend dev server is running. |

### Credential and SDK boundaries

- Use one credential owner per identity. Resolve the configured credential path, Codex home and storage mode. For managed ChatGPT login, delegate refresh to the Codex App Server and reload the authoritative store after it commits. Its account-read response contains metadata. The TypeScript Codex SDK handles threads and streamed turns. [Codex App Server](https://developers.openai.com/codex/app-server/) · [Codex SDK](https://developers.openai.com/codex/sdk/)
- Watch the credential file’s parent for atomic replacement. Reconcile after successful plugin login and support a bounded fallback for missed notifications and keyring-only changes. During assessment, specify separately the maximum pickup time for file-backed and keyring-backed login. Exercise an external CLI login and a running container. The current Compose single-file bind mount can retain the old inode after a host rename. Select a mount or notification design that preserves the intended write owner.
- Exercise a burst of requests at expiry, logout or account switch during a slow refresh, a partial write, a revoked grant and an ordinary API key. Require one managed refresh per identity, coherent bearer/account/profile publication and no resurrection of a stale login. Preserve partial stream output and tool effects across auth failures. Include CLI, HTTP, MCP and the optional embedding client when tracing long-lived credential copies.

### OpenCode 2.x and ChatGPT Pro

- Target OpenAI’s highest ChatGPT Pro tier, as confirmed by the project owner. Separate Codex thread/turn execution from OpenCode provider registration. Assign one owner to tool execution per turn so a tool call cannot execute twice. Preserve upstream quota, account and model errors.
- Use documented, user-authorized sign-in paths. Determine and demonstrate which identity and allowance are used by the chosen integration. OpenCode’s documented ChatGPT sign-in and a separately packaged service can have different authorization boundaries. A plan label, including the installed protocol’s `promax` value, cannot establish a specific model entitlement. A shared deployment needs per-user authorization and isolation before exposing subscription-backed chat. [OpenCode providers](https://opencode.ai/docs/providers/#chatgpt) · [OpenAI plan access in other apps](https://help.openai.com/en/articles/20001542-using-your-chatgpt-plan-in-other-apps-and-sites)
- Register the V2 provider through the tagged extension API and keep credentials out of user-visible plugin configuration. Verify consent, permitted access, quota exhaustion and unknown plan labels with fixtures. Distinguish fixture results from an operator-authorized real-account entitlement check. [OpenCode 2.3 plugin API](https://github.com/anomalyco/opencode/tree/v2.3.0/services/site/src/docs/content/build/plugins)

### Chat, settings and packaging

- Build the React 19/Vite 8 app under `frontend/`. Use the product brief and Impeccable for design, UI UX Pro Max for focused interaction research and shadcn/ui for components. Chat should support keyboard navigation, cancellation, partial output and understandable connection errors.
- In local mode an authorized owner can see connection health and request refresh. In shared mode enforce chat-user and administrator privileges at the API. Test a direct unauthorized refresh request as well as the rendered UI. The browser receives redacted state. Scope administrative CORS and session protections separately from the existing permissive API router.
- `build.rs` invokes the locked frontend build and embeds output from Cargo’s build directory. Reviewers should exercise a clean release, incremental rebuild after editing an asset, a failed UI build, and the packaged binary/container without Node or source files. Missing assets and unknown API routes retain their normal errors.

The service reviewer owns cross-surface acceptance evidence once a candidate exists. Prove routine behavior with temporary credential stores, a fake upstream, and a fake clock. Actual provider-plan access remains a distinct verification step.
