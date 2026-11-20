import { spawn } from "node:child_process"
import { isAbsolute } from "node:path"
import { realpath } from "node:fs/promises"

// OpenCode interprets a successful command's stdout as a credential. Reserve it for a
// public completion marker. Only Codex accesses or persists the upstream login.
async function login() {
  const origin = new URL(process.env.PROXY_CONSOLE_URL ?? "http://127.0.0.1:8181")
  const token = process.env.PROXY_CONSOLE_TOKEN
  const home = process.env.OPENAI_PROXY_LOGIN_HOME
  const binary = process.env.OPENAI_PROXY_LOGIN_BINARY
  if (!["127.0.0.1","[::1]","localhost"].includes(origin.hostname) || !["http:","https:"].includes(origin.protocol) ||
      origin.username || origin.password || origin.pathname !== "/" || origin.search || origin.hash ||
      !token || !isAbsolute(home ?? "") || !isAbsolute(binary ?? "")) throw new Error("unbound_login")
  async function connection(reload) {
    const response = await fetch(new URL("/api/v1/me/connection" + (reload ? "/reload" : ""),origin), {
      method:reload ? "POST" : "GET", redirect:"error", signal:AbortSignal.timeout(5000),
      headers:{authorization:"Bearer " + token,"content-type":"application/json"},
      ...(reload ? {body:"{}"} : {}),
    })
    if (!response.ok) throw new Error("service_rejected")
    return response.json()
  }
  const before = await connection(false)
  const executable = await realpath(binary)
  const child = spawn(executable,["login"],{stdio:["ignore",2,2],env:{
    CODEX_HOME:await realpath(home),HOME:process.env.HOME,PATH:"/usr/local/bin:/usr/bin:/bin",
    ...(process.env.DISPLAY ? {DISPLAY:process.env.DISPLAY} : {}),
  }})
  const stop = () => child.kill("SIGTERM")
  process.once("SIGTERM",stop)
  process.once("SIGINT",stop)
  try {
    const code = await new Promise((resolve,reject) => {child.once("error",reject);child.once("exit",resolve)})
    if (code !== 0) throw new Error("login_incomplete")
    await connection(true)
    const after = await connection(false)
    if (after.status !== "ready" || after.generation <= before.generation) throw new Error("login_unconfirmed")
    process.stdout.write("openai-proxy-owner-login-confirmed\n")
  } finally {
    process.off("SIGINT",stop)
    process.off("SIGTERM",stop)
  }
}
login().catch(() => {process.stderr.write("Codex owner login could not be confirmed.\n");process.exitCode=1})
