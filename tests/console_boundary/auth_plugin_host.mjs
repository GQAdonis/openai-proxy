// Local host and installed plugin. The subprocess receives a disposable HOME.
import {join} from "node:path"
import {rename, readFile} from "node:fs/promises"
import {pathToFileURL} from "node:url"

const [packages, project, mode, localPackage] = process.argv.slice(2)
const originalFetch = globalThis.fetch
const origin = new URL(process.env.PROXY_CONSOLE_URL).origin
const allowedFetch = (arg, init) => {
  const url = new URL(arg instanceof Request ? arg.url : arg)
  if (url.origin !== origin) throw new Error("fixture rejected non-service fetch")
  return originalFetch(arg, {...init, redirect:"error"})
}
globalThis.fetch = allowedFetch

async function main() {
  if (mode === "version-guard") {
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
  const dir = join(packages, "node_modules/@opencode/sdk")
  const meta = JSON.parse(await readFile(join(dir, "package.json"), "utf8"))
  if (meta.name !== "@opencode/sdk" || meta.version !== "2.3.0") throw new Error("wrong host package")
  const {OpenCode} = await import(pathToFileURL(join(dir,meta.exports["."].import)))
  const host = await OpenCode.create({
    database:{path:join(project,"host.sqlite")}, config:{directory:join(project,"host-config"),project:true,claude:false},
    fetch:allowedFetch
  })
  try {
    const location = {directory:project}
    const inventory = async () => (await host.model.list({location})).data.filter(x => x.providerID === "codex")
    const provider = (await host.provider.get({providerID:"codex",location})).data
    if (mode === "override") {
      if (provider.name !== "Operator fixture") throw new Error("provider override lost")
      return {preservedOverride:true}
    }
    if (provider.name !== "Codex via openai-proxy" || provider.settings.baseURL !== origin + "/api/v1/drivers/codex/v1") {
      throw new Error("host returned wrong provider")
    }
    const models = await inventory()
    if (!models.some(x=>x.id === "gpt-5.5" || x.modelID === "gpt-5.5")) throw new Error("fixture model missing")
    const home = process.env.CODEX_HOME
    await rename(join(home, "next-model"), join(home, "catalogue"))
    await rename(join(home, "next-generation"), join(home, "auth.json"))
    for (let n=0;n<60;n++) {
      if ((await inventory()).some(x=>x.id === "gpt-5.4" || x.modelID === "gpt-5.4")) {
        return {hostVersion:meta.version, provider:"codex", changedModel:"gpt-5.4"}
      }
      await new Promise(resolve => setTimeout(resolve,250))
    }
    throw new Error("host picker did not reflect committed generation")
  } finally {
    await host.close()
  }
}
main().then(result => console.log(JSON.stringify(result))).catch(() => {process.exitCode = 1})
