import { spawn } from "node:child_process"
import { readCodexAuth } from "./auth.js"

/**
 * Spawns `codex login` in a subprocess, waits for it to complete, then reads
 * the resulting ~/.codex/auth.json and returns the tokens in the shape opencode's
 * OAuth callback expects.
 *
 * This delegates the entire browser-based OAuth flow to the Codex CLI so we
 * never have to manage client_id / PKCE ourselves.
 */
export async function spawnCodexLogin(): Promise<{
  url?: string
  method?: "code"
  callback?: (code: string) => Promise<{
    type: "success"
    access: string
    refresh?: string
    expires?: number
  }>
}> {
  // Run `codex login` interactively — the CLI opens the browser and handles
  // the localhost OAuth callback automatically.
  await runCodexLogin()

  // After the CLI exits successfully, auth.json is written.
  const auth = await readCodexAuth()

  const token = auth.access_token ?? auth.api_key
  if (!token) {
    throw new Error(
      "codex login completed but no token found in ~/.codex/auth.json"
    )
  }

  // Return in the opencode OAuth success shape so it persists the token.
  return {
    // No URL redirect needed — we already ran the login via subprocess.
    // Returning a no-op callback tells opencode the auth is complete.
    method: "code",
    callback: async () => ({
      type: "success" as const,
      access: token,
      refresh: undefined,
      // Codex tokens last ~1 hour; set a conservative expiry so opencode
      // knows to re-check.
      expires: Date.now() + 55 * 60 * 1000,
    }),
  }
}

function runCodexLogin(): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn("codex", ["login"], {
      stdio: "inherit", // let the CLI handle TTY interaction directly
      shell: false,
    })

    child.on("close", (code) => {
      if (code === 0) {
        resolve()
      } else {
        reject(new Error(`codex login exited with code ${code}`))
      }
    })

    child.on("error", (err) => {
      reject(
        new Error(
          `Failed to spawn 'codex login': ${err.message}. ` +
            `Is the Codex CLI installed? Run: npm i -g @openai/codex`
        )
      )
    })
  })
}
