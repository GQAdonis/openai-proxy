import type { Plugin } from "@opencode-ai/plugin"
import { readCodexAuth, type CodexAuth } from "./auth.js"
import { PROXY_BASE_URL, PROXY_MODELS, PROVIDER_ID } from "./config.js"

/**
 * opencode-codex-proxy plugin
 *
 * Tight integration strategy:
 *
 * 1. config hook  — injects the provider + model definitions into opencode's
 *    live config so the proxy appears as a first-class provider named "codex"
 *    without requiring the user to edit opencode.json manually.
 *
 * 2. auth hook    — registers an authentication provider so `/connect` and
 *    `opencode auth login` surface the Codex OAuth flow and store tokens in
 *    opencode's own auth store (~/.local/share/opencode/auth.json), keeping
 *    a single credential source across all tools.
 *
 * 3. shell.env hook — injects CODEX_AUTH_PATH so the Rust proxy binary can
 *    locate the auth credentials written by the auth hook without needing its
 *    own separate configuration.
 *
 * 4. event hook (session.created) — verifies the proxy is running on startup
 *    and prints a friendly toast if it is not.
 */
export const CodexProxyPlugin: Plugin = async (ctx) => {
  // Load Codex credentials at plugin init time (best-effort; may not exist yet).
  let auth: CodexAuth | null = null
  try {
    auth = await readCodexAuth()
  } catch {
    // Will be populated after the user runs `opencode auth login`.
  }

  return {
    // ── 1. config ────────────────────────────────────────────────────────────
    // Mutate the live config object to inject our provider.
    // This runs before opencode finishes loading, so the provider appears in
    // /models and /provider exactly like a built-in one.
    config: async (config) => {
      // Guard: don't clobber an explicit user override.
      if (config.provider?.[PROVIDER_ID]) return

      config.provider ??= {}
      config.provider[PROVIDER_ID] = {
        // @ai-sdk/openai-compatible understands /v1/chat/completions — our proxy speaks that.
        npm: "@ai-sdk/openai-compatible",
        name: "Codex (via proxy)",
        options: {
          baseURL: PROXY_BASE_URL,
          // The proxy reads ~/.codex/auth.json itself; opencode just needs
          // a non-empty apiKey to satisfy the SDK validation layer.
          apiKey: auth?.apiKey ?? "codex-proxy",
        },
        models: Object.fromEntries(
          PROXY_MODELS.map((m) => [
            m.id,
            { name: m.name, limit: { context: m.context, output: m.output } },
          ])
        ),
      }
    },

    // ── 2. auth ──────────────────────────────────────────────────────────────
    // Registers "codex" in opencode's /connect UI and persists tokens to
    // ~/.local/share/opencode/auth.json so a single `opencode auth login`
    // command is the only setup step needed.
    auth: {
      provider: PROVIDER_ID,

      // loader: called by opencode when it needs credentials for this provider.
      // We prefer the ChatGPT OAuth access_token; fall back to api_key.
      loader: async (storedAuth) => {
        // storedAuth is what opencode has in its own auth.json for this provider.
        if (storedAuth?.access) {
          return { apiKey: storedAuth.access }
        }
        // Fall back to the Codex CLI's own auth.json.
        try {
          const codexAuth = await readCodexAuth()
          if (codexAuth.access_token) return { apiKey: codexAuth.access_token }
          if (codexAuth.api_key) return { apiKey: codexAuth.api_key }
        } catch {
          // No auth available — user must connect.
        }
        return null
      },

      methods: [
        // ── OAuth path (ChatGPT Plus / Pro subscription) ──────────────────
        {
          type: "oauth",
          label: "Sign in with ChatGPT (Codex subscription)",
          async authorize() {
            // Delegate to the Codex CLI's own OAuth flow, which writes
            // ~/.codex/auth.json.  After the browser round-trip completes,
            // the callback reads that file and returns the tokens to opencode.
            const { spawnCodexLogin } = await import("./codex-login.js")
            return spawnCodexLogin()
          },
        },
        // ── API key path (standard OpenAI key, no subscription needed) ────
        {
          type: "apiKey" as const,
          label: "OpenAI API key (non-subscription fallback)",
        },
      ],
    },

    // ── 3. shell.env ─────────────────────────────────────────────────────────
    // Inject CODEX_AUTH_PATH, CODEX_PROXY_URL, and CODEX_DEFAULT_MODEL into
    // every shell the agent spawns so scripts and the proxy binary locate
    // credentials and configuration without extra setup.
    "shell.env": async (_input, output) => {
      const authPath = resolveCodexAuthPath()
      if (authPath) {
        output.env.CODEX_AUTH_PATH = authPath
      }
      output.env.CODEX_PROXY_URL = process.env.CODEX_PROXY_URL ?? PROXY_BASE_URL
      const defaultModel = process.env.CODEX_DEFAULT_MODEL
      if (defaultModel) {
        output.env.CODEX_DEFAULT_MODEL = defaultModel
      }
    },

    // ── 4. event (session.created) ───────────────────────────────────────────
    // Warn the user if the proxy is not reachable when a session starts.
    event: async ({ event }) => {
      if (event.type !== "session.created") return
      if (!(await isProxyHealthy())) {
        await ctx.client.tui.toast({
          type: "warning",
          message:
            `Codex proxy not running on ${PROXY_BASE_URL}. ` +
            `Start it with: cargo run --release --manifest-path <path>/openai-proxy/Cargo.toml`,
        })
      }
    },
  }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function resolveCodexAuthPath(): string | null {
  const override = process.env.CODEX_AUTH_PATH
  if (override) return override
  const home = process.env.HOME ?? process.env.USERPROFILE
  return home ? `${home}/.codex/auth.json` : null
}

async function isProxyHealthy(): Promise<boolean> {
  try {
    const res = await fetch(`${PROXY_BASE_URL.replace(/\/v1$/, "")}/health`, {
      signal: AbortSignal.timeout(1500),
    })
    return res.ok
  } catch {
    return false
  }
}

export default CodexProxyPlugin
