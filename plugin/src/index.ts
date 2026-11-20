import { fileURLToPath } from "node:url"
import { isAbsolute } from "node:path"
import { Integration, Plugin, Provider } from "@opencode/plugin"
import { authorization, inventory, providerID, request, serviceOrigin } from "./config"
import { registerDelegation } from "./delegation"

export default Plugin.define({
  id: "openai-proxy.codex",
  async setup(ctx) {
    const version = /^(\d+)\.(\d+)\.(\d+)(?:\+[^-]+)?$/.exec(ctx.app.version)
    if (!version || Number(version[1]) !== 2 || Number(version[2]) < 3) {
      throw new Error("This provider requires stable OpenCode >=2.3.0 and <3.0.0")
    }
    const origin = serviceOrigin()
    const prefix = "/api/v1/drivers/codex/v1"
    const baseURL = new URL(prefix, origin).href
    const integrationID = Integration.ID.make("openai-proxy")
    const connection = () => ctx.integration.connection.active(integrationID)
    let models = await inventory()
    let activeConnection = await connection()

    await ctx.provider.transform(editor => {
      if (editor.get(providerID)) return
      editor.add({
        info: { ...Provider.Info.empty(providerID), integrationID, name: "Codex via openai-proxy",
          activation: "enabled", package: "@opencode/ai/providers/openai-compatible",
          settings: { baseURL, apiKey: "proxy-scoped" } },
        models, ...(activeConnection ? { sourceConnection: activeConnection } : {}),
      })
    })
    await ctx.integration.transform(editor => {
      editor.method.update({integrationID, method:{type:"env", names:["PROXY_CONSOLE_TOKEN"]}})
      if (["localhost","127.0.0.1","[::1]"].includes(origin.hostname) &&
          isAbsolute(process.env.OPENAI_PROXY_LOGIN_HOME ?? "") && isAbsolute(process.env.OPENAI_PROXY_LOGIN_BINARY ?? "")) {
        editor.method.update({ integrationID, method:{type:"command",id:"codex-login",
          label:"Sign in with the local Codex owner",
          command:[process.execPath,fileURLToPath(new URL("./login.mjs",import.meta.url))]}})
      }
    })
    await ctx.session.hook("http.request", event => {
      const url = new URL(event.request.url)
      if (url.origin === origin.origin && url.pathname.startsWith(prefix + "/")) {
        event.request = new Request(event.request, { redirect: "error" })
        event.request.headers.set("authorization", authorization())
      }
    }, { providerID })
    let lastGeneration = ""
    let pending: Promise<void> | undefined
    const refresh = () => pending ??= (async () => {
      const response = await request("/api/v1/me/connection")
      const state = response.ok ? await response.json() : {}
      const generation = JSON.stringify([response.status, state.generation, state.status])
      if (generation !== lastGeneration || !models.some(model => model.enabled)) {
        activeConnection = await connection()
        models = await inventory()
        await ctx.provider.reload()
        lastGeneration = generation
      }
    })().finally(() => { pending = undefined })
    await ctx.command.transform(editor => {
      editor.add({ name: "codex-reconcile", description: "Refresh this connection after signing in with Codex.",
        execute: async () => {
          const response = await request("/api/v1/me/connection/reload", { method: "POST", headers: {"content-type":"application/json"}, body:"{}" })
          if (!response.ok) throw new Error("Codex connection reload was rejected")
          await refresh()
        },
      })
    })
    await registerDelegation(ctx, async (call, signal) => {
      const response = await request("/api/v1/drivers/codex/delegate", {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(call), signal,
      })
      if (!response.ok) throw new Error("The service rejected this delegation")
      const body = await response.json()
      if (typeof body.text !== "string") throw new Error("Invalid delegation response")
      return body.text
    })
    const timer = setInterval(() => { void refresh().catch(() => undefined) }, 5000)
    timer.unref()
    return () => clearInterval(timer)
  },
})
