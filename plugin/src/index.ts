import type { Plugin, AuthOAuthResult } from "@opencode-ai/plugin"
import { readCodexAuth, type CodexAuth } from "./auth.js"
import { PROXY_BASE_URL, PROXY_MODELS, PROVIDER_ID, type ProxyModel } from "./config.js"

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
 *    and prints a warning toast if it is not.
 */
export const CodexProxyPlugin: Plugin = async (ctx) => {
  // Load Codex credentials at plugin init time (best-effort; may not exist yet).
  let auth: CodexAuth | null = null
  try {
    auth = await readCodexAuth()
  } catch {
    // Will be populated after the user runs `opencode auth login`.
  }

  // Runtime model list — starts as the static fallback, refreshed from
  // GET /v1/models once the proxy health check passes.
  let models: ProxyModel[] = [...PROXY_MODELS]

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
          // Prefer the ChatGPT OAuth access_token, fall back to api_key.
          // The proxy reads ~/.codex/auth.json itself; opencode just needs
          // a non-empty apiKey to satisfy the SDK validation layer.
          apiKey: auth?.access_token ?? auth?.api_key ?? "codex-proxy",
        },
        models: Object.fromEntries(
          models.map((m) => [
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
      // `getAuth` is a function that returns the stored Auth from opencode's
      // own auth store — call it to check for a previously-stored token.
      loader: async (getAuth) => {
        try {
          const stored = await getAuth()
          if (stored && "access" in stored && stored.access) {
            return { apiKey: stored.access }
          }
        } catch {
          // No stored auth — fall through to Codex CLI file.
        }
        // Fall back to the Codex CLI's own auth.json.
        try {
          const codexAuth = await readCodexAuth()
          if (codexAuth.access_token) return { apiKey: codexAuth.access_token }
          if (codexAuth.api_key) return { apiKey: codexAuth.api_key }
        } catch {
          // No auth available — user must connect.
        }
        return {}
      },

      methods: [
        // ── OAuth path (ChatGPT Plus / Pro subscription) ──────────────────
        {
          type: "oauth",
          label: "Sign in with ChatGPT (Codex subscription)",
          async authorize(): Promise<AuthOAuthResult> {
            // Delegate to the Codex CLI's own OAuth flow, which opens the
            // browser and writes ~/.codex/auth.json on completion.
            const { spawnCodexLogin } = await import("./codex-login.js")
            return spawnCodexLogin()
          },
        },
        // ── API key path (standard OpenAI key, no subscription needed) ────
        {
          type: "api" as const,
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
    // Also refresh the model list from /v1/models when the proxy is healthy.
    event: async ({ event }) => {
      if (event.type !== "session.created") return
      const healthy = await isProxyHealthy()
      if (!healthy) {
        await ctx.client.tui.showToast({
          body: {
            variant: "warning",
            message:
              `Codex proxy not running on ${PROXY_BASE_URL}. ` +
              `Start it with: cargo run --release --manifest-path <path>/openai-proxy/Cargo.toml`,
          },
        })
        return
      }
      // Refresh model list from the running proxy so context limits stay current.
      const refreshed = await fetchModelsFromProxy()
      if (refreshed.length > 0) {
        models = refreshed
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

/** Fetch model definitions from the running proxy's /v1/models endpoint. */
async function fetchModelsFromProxy(): Promise<ProxyModel[]> {
  try {
    const res = await fetch(`${PROXY_BASE_URL}/models`, {
      signal: AbortSignal.timeout(2000),
    })
    if (!res.ok) return []
    const json = (await res.json()) as {
      data?: Array<{
        id: string
        context_length?: number
        max_output_tokens?: number
      }>
    }
    if (!Array.isArray(json.data)) return []
    return json.data
      .filter((m) => typeof m.id === "string")
      .map((m) => ({
        id: m.id,
        name: m.id,
        context: m.context_length ?? 128_000,
        output: m.max_output_tokens ?? 16_384,
      }))
  } catch {
    return []
  }
}

export default CodexProxyPlugin
