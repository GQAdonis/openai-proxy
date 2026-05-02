/** Provider ID used across opencode config, auth store, and /connect UI. */
export const PROVIDER_ID = "codex"

/**
 * Base URL of the Rust proxy.  The `/v1` suffix is what @ai-sdk/openai-compatible
 * expects — it appends `/chat/completions` automatically.
 */
export const PROXY_BASE_URL =
  process.env.CODEX_PROXY_URL ?? "http://localhost:8080/v1"

/** Models surfaced in opencode's /models picker. */
export const PROXY_MODELS = [
  {
    id: "gpt-5.3-codex",
    name: "GPT-5.3 Codex",
    context: 128_000,
    output: 16_384,
  },
  {
    id: "codex-mini",
    name: "Codex Mini",
    context: 128_000,
    output: 16_384,
  },
  {
    id: "gpt-4o",
    name: "GPT-4o (via Codex proxy)",
    context: 128_000,
    output: 16_384,
  },
] as const
