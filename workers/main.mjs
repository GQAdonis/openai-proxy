import { fileURLToPath } from "node:url"
import { dirname, isAbsolute, join } from "node:path"
import { existsSync, readFileSync } from "node:fs"

const driver = process.argv[2]
const versions = { codex: ["@openai/codex-sdk", "0.169.0"], opencode: ["@opencode/sdk", "2.3.0"] }
const send = (type, fields = {}) => process.stdout.write(JSON.stringify({ v: 1, type, ...fields }) + "\n")
let active
let ended = false

function terminal(status, code) {
  if (ended) return
  ended = true
  send("terminal", { status, ...(code ? {code} : {}) })
  process.stdin.destroy()
  process.exitCode = status === "failed" ? 1 : 0
}
function checkPackage(name, version) {
  let dir = dirname(fileURLToPath(import.meta.resolve(name)))
  while (!existsSync(join(dir, "package.json")) && dirname(dir) !== dir) dir = dirname(dir)
  const metadata = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"))
  if (metadata.name !== name || metadata.version !== version) throw new Error("incompatible_sdk")
}
async function dispatch(command, adapter) {
  if (ended) return
  if (command.v !== 1) throw new Error("protocol")
  if (command.type === "start") {
    if (active || typeof command.prompt !== "string" || !command.prompt || command.prompt.length > 65536 ||
        typeof command.workspace !== "string" || !isAbsolute(command.workspace) ||
        typeof command.model !== "string" || !command.model || command.model.length > 256) throw new Error("invalid_start")
    active = adapter.create(command, send)
    active.run().then(result => {
      if (typeof result === "string") terminal(result)
      else terminal(result.status, result.code)
    }, () => terminal("failed", "native_failure"))
  } else if (command.type === "cancel" && active) {
    await active.cancel()
  } else if (command.type === "permission" && active && ["allow", "deny"].includes(command.decision)) {
    await active.permission(command.requestId, command.decision)
  } else { throw new Error("unexpected_command") }
}

try {
  if (!Object.hasOwn(versions, driver)) throw new Error("unknown_driver")
  const [name, version] = versions[driver]
  checkPackage(name, version)
  const adapter = driver === "codex" ? await import("./codex.mjs") : await import("./opencode.mjs")
  send("ready", {driver, version, permissions: driver === "opencode"})
  let pending = Buffer.alloc(0)
  for await (const chunk of process.stdin) {
    if (ended) break
    pending = Buffer.concat([pending, chunk])
    let cut
    while ((cut = pending.indexOf(10)) !== -1) {
      if (cut > 262144) throw new Error("frame_too_large")
      const line = pending.subarray(0, cut).toString("utf8")
      pending = pending.subarray(cut + 1)
      await dispatch(JSON.parse(line), adapter)
    }
    if (pending.length > 262144) throw new Error("frame_too_large")
  }
  if (!ended && active) await active.cancel()
} catch {
  await active?.cancel().catch(() => undefined)
  terminal("failed", "worker_protocol_or_dependency")
}
