// Local host and installed plugin. The subprocess receives a disposable HOME.
import {join} from "node:path"
import {rename, readFile, realpath} from "node:fs/promises"
import {pathToFileURL} from "node:url"

const [packages, project, mode, localPackage, expectedSource] = process.argv.slice(2)
const originalFetch = globalThis.fetch
const origin = new URL(process.env.PROXY_CONSOLE_URL).origin
const allowedFetch = (arg, init) => {
  const url = new URL(arg instanceof Request ? arg.url : arg)
  if (url.origin !== origin) throw new Error("fixture rejected non-service fetch")
  return originalFetch(arg, {...init, redirect:"error"})
}
globalThis.fetch = allowedFetch

// Emit only fixed diagnostic labels. SDK errors may contain paths or request headers.
function enter(stage) {
  console.log("KBD_PLUGIN_STAGE " + stage)
}
function category(error) {
  const message = String(error?.message ?? error)
  if (/requires stable OpenCode/.test(message)) return "version-guard"
  if (/fixture rejected non-service fetch/.test(message)) return "egress-policy"
  if (/module not found|cannot find (?:module|package)|cannot resolve/i.test(message)) return "dependency"
  if (/location plugins did not finish activating/i.test(message)) return "activation-timeout"
  if (/provider not found/i.test(message)) return "provider-missing"
  if (/fixture model missing|did not reflect committed generation/.test(message)) return "model-mismatch"
  if (/plugin source differs|unexpected plugin identity/.test(message)) return "plugin-mismatch"
  if (/override lost|host returned wrong provider/.test(message)) return "provider-mismatch"
  return "host-error"
}

async function main() {
  if (mode === "version-guard") {
    enter("version-guard")
    const entry = JSON.parse(await readFile(join(localPackage, "package.json"), "utf8")).exports["./server"]
    const {default: plugin} = await import(pathToFileURL(join(localPackage,entry)))
    for (const version of ["1.12.0", "2.2.9", "3.0.0", "2.3.0-beta.1"]) {
      let denied = false
      try { await plugin.setup({app: {version}}) }
      catch (e) { denied = /requires stable OpenCode/.test(e.message) }
      if (!denied) throw new Error("version guard failed")
    }
    return {controlledVersionsRejected:4}
  }
  enter("host-import")
  const dir = join(packages, "node_modules/@opencode/sdk")
  const meta = JSON.parse(await readFile(join(dir, "package.json"), "utf8"))
  if (meta.name !== "@opencode/sdk" || meta.version !== "2.3.0") throw new Error("wrong host package")
  const {OpenCode} = await import(pathToFileURL(join(dir,meta.exports["."].import)))
  enter("host-create")
  const host = await OpenCode.create({
    app:{version:meta.version}, database:{path:join(project,"host.sqlite")},
    config:{directory:join(project,"host-config"),project:true,claude:false}, fetch:allowedFetch
  })
  try {
    const location = {directory:project}
    const inventory = async () => (await host.model.list({location})).data.filter(x => x.providerID === "codex")
    enter("activation-barrier")
    await host.provider.list({location})
    enter("plugin-source")
    const expectedPath = await realpath(expectedSource)
    const registrations = (await host.plugin.list({location})).data
    const matches = []
    for (const registration of registrations) {
      if (registration.source.type === "local" && await realpath(registration.source.path) === expectedPath) {
        matches.push(registration)
      }
    }
    if (matches.length !== 1) throw new Error("plugin source differs")
    const [registration] = matches
    enter("plugin-activation")
    if (registration.state.status === "failed") throw new Error(registration.state.error)
    if (registration.id !== "openai-proxy.codex" || registration.state.status !== "active" || !registration.features.server) {
      throw new Error("unexpected plugin identity")
    }
    const observation = {hostVersion:meta.version, activePlugin:registration.id, configuredSourceMatched:true}
    enter("provider-lookup")
    const provider = (await host.provider.get({providerID:"codex",location})).data
    if (mode === "override") {
      if (provider.name !== "Operator fixture") throw new Error("provider override lost")
      return {...observation, preservedOverride:true}
    }
    if (provider.name !== "Codex via openai-proxy" || provider.settings?.baseURL !== origin + "/api/v1/drivers/codex/v1") {
      throw new Error("host returned wrong provider")
    }
    enter("initial-inventory")
    const models = await inventory()
    if (!models.some(x=>x.id === "gpt-5.5" || x.modelID === "gpt-5.5")) throw new Error("fixture model missing")
    enter("generation-change")
    const home = process.env.CODEX_HOME
    await rename(join(home, "next-model"), join(home, "catalogue"))
    await rename(join(home, "next-generation"), join(home, "auth.json"))
    enter("refreshed-inventory")
    for (let n=0;n<60;n++) {
      if ((await inventory()).some(x=>x.id === "gpt-5.4" || x.modelID === "gpt-5.4")) {
        return {...observation, provider:"codex", changedModel:"gpt-5.4"}
      }
      await new Promise(resolve => setTimeout(resolve,250))
    }
    throw new Error("host picker did not reflect committed generation")
  } finally {
    await host.close().catch(error => {
      enter("host-close")
      throw error
    })
  }
}

main().then(result => console.log(JSON.stringify(result))).catch(error => {
  console.log("KBD_PLUGIN_FAILURE " + category(error))
  process.exitCode = 1
})
