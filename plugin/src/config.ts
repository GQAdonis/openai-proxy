/** Provider ID used across opencode config, auth store, and /connect UI. */
export const PROVIDER_ID = "codex"

/**
 * Base URL of the Rust proxy.  The `/v1` suffix is what @ai-sdk/openai-compatible
 * expects — it appends `/chat/completions` automatically.
 */
export const PROXY_BASE_URL =
  process.env.CODEX_PROXY_URL ?? "http://localhost:8080/v1"

/** Shape of a single model entry used by the plugin and config hook. */
export interface ProxyModel {
  id: string
  name: string
  context: number
  output: number
}

/**
 * Static fallback model list. Used when the proxy is not reachable at startup.
 * The event hook refreshes this from GET /v1/models when the proxy is healthy.
 * Context limits here match the Rust catalog in src/models.rs.
 * codex-mini is intentionally excluded from this list.
 */
export const PROXY_MODELS: ProxyModel[] = [
  { id: "gpt-5.5",       name: "GPT-5.5",       context: 1_000_000, output: 32_768 },
  { id: "gpt-5.5-pro",   name: "GPT-5.5 Pro",   context: 1_000_000, output: 32_768 },
  { id: "gpt-5.4",       name: "GPT-5.4",       context:   400_000, output: 32_768 },
  { id: "gpt-5.4-mini",  name: "GPT-5.4 Mini",  context:   200_000, output: 16_384 },
  { id: "gpt-5.4-nano",  name: "GPT-5.4 Nano",  context:   128_000, output:  8_192 },
  { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", context:   400_000, output: 32_768 },
  { id: "gpt-5.3-chat",  name: "GPT-5.3 Chat",  context:   128_000, output: 16_384 },
  { id: "gpt-5.2-chat",  name: "GPT-5.2 Chat",  context:   128_000, output: 16_384 },
]

/** Default model used when none is specified. */
export const DEFAULT_MODEL = "gpt-5.5"
