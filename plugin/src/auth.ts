import { readFile } from "node:fs/promises"
import { homedir } from "node:os"
import { join } from "node:path"

/** Shape of ~/.codex/auth.json written by `codex login`. */
export interface CodexAuth {
  /** ChatGPT OAuth access token (ChatGPT Plus / Pro path). */
  access_token?: string
  /** ChatGPT account UUID — required alongside access_token. */
  account_id?: string
  /** Standard OpenAI API key (fallback path). */
  api_key?: string
}

/**
 * Reads ~/.codex/auth.json (or CODEX_AUTH_PATH override).
 * Throws if the file does not exist or is malformed.
 */
export async function readCodexAuth(
  pathOverride?: string
): Promise<CodexAuth> {
  const authPath =
    pathOverride ??
    process.env.CODEX_AUTH_PATH ??
    join(homedir(), ".codex", "auth.json")

  const content = await readFile(authPath, "utf8")
  return JSON.parse(content) as CodexAuth
}

/**
 * Returns true if the auth object has enough credentials to call the Codex backend.
 */
export function isAuthenticated(auth: CodexAuth): boolean {
  return (
    (!!auth.access_token && !!auth.account_id) || !!auth.api_key
  )
}
