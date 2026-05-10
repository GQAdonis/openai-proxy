import { spawn } from "node:child_process"
import { readCodexAuth } from "./auth.js"
import type { AuthOAuthResult } from "@opencode-ai/plugin"

/**
 * Spawns `codex login` in a subprocess, waits for it to complete, then reads
 * the resulting ~/.codex/auth.json and returns the tokens in the shape opencode's
 * OAuth callback expects.
 *
 * This delegates the entire browser-based OAuth flow to the Codex CLI so we
 * never have to manage client_id / PKCE ourselves.
 */
export async function spawnCodexLogin(): Promise<AuthOAuthResult> {
  return {
    // No browser redirect needed — the Codex CLI handles the OAuth browser
    // round-trip itself when its subprocess runs.
    url: "about:blank",
    instructions: "The Codex CLI will open your browser automatically.",
    method: "auto",
    async callback() {
      // Run `codex login` interactively — the CLI opens the browser and handles
      // the localhost OAuth callback automatically.
      await runCodexLogin()

      // After the CLI exits successfully, auth.json is written.
      const auth = await readCodexAuth()

      const token = auth.access_token ?? auth.api_key
      if (!token) {
        return { type: "failed" as const }
      }

      return {
        type: "success" as const,
        access: token,
        refresh: auth.access_token ? "" : token,
        // Codex tokens last ~1 hour; set a conservative expiry so opencode
        // knows to re-check before the token expires.
        expires: Date.now() + 55 * 60 * 1000,
      }
    },
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
