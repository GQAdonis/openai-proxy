import { OpenCode } from "@opencode/sdk"
import { Plugin } from "@opencode/plugin"
import { dirname, isAbsolute, join, relative, sep } from "node:path"
import { mkdir, open, realpath, lstat, mkdtemp, rmdir } from "node:fs/promises"
import { constants } from "node:fs"
import { userInfo } from "node:os"
import { registerDelegation } from "../plugin/src/delegation.ts"

function trustedURL(value) {
  const url = new URL(value)
  if (url.username || url.password || url.search || url.hash ||
      !(url.protocol === "https:" || (["127.0.0.1","[::1]","localhost"].includes(url.hostname) && url.protocol === "http:"))) {
    throw new Error("invalid_endpoint")
  }
  return url
}

export function create(input, send) {
  let host, session
  let stopping = false
  const requests = new Set()
  const eventsAbort = new AbortController()
  async function cancel() {
    stopping = true
    if (session && host) await host.sessions.interrupt({sessionID:session.id, resume:false})
  }
  return {
    cancel,
    async permission(id, decision) {
      if (!session || !requests.delete(id)) throw new Error("unknown_permission")
      await host.permission.reply({sessionID:session.id, requestID:id, decision:decision === "allow" ? "once" : "reject"})
    },
    async run() {
      if (!isAbsolute(input.database ?? "") || !input.provider || !/^[a-zA-Z0-9._-]{1,100}$/.test(input.provider.id) ||
          input.provider.id === "codex") throw new Error("invalid_provider")
      const upstream = trustedURL(input.provider.baseURL)
      const model = { providerID:input.provider.id, id:input.model.replace(input.provider.id + "/", "") }
      if (model.id.includes("/") || /\/api\/v1\/drivers\/|\/api\/v1\/conversations/.test(upstream.pathname)) throw new Error("route_cycle")
      const workspace = await realpath(input.workspace)
      const privateDir = dirname(input.database)
      await mkdir(privateDir, {recursive:true, mode:0o700})
      const catalog = { [model.providerID]: {name:"Configured upstream", package:"@opencode/ai/providers/openai-compatible",
        settings:{baseURL:upstream.href, apiKey:"worker-scoped"},
        models:{ [model.id]:{name:model.id, capabilities:{tools:true, input:["text"],output:["text"]}} } } }
      const allowed = ["workspace_read", "render_surface"]
      if (input.delegateURL && input.delegateToken) allowed.push("delegate_codex")
      const isolated = Plugin.define({id:"openai-proxy.worker-policy", async setup(ctx) {
        await ctx.tool.transform(editor => {
          for (const tool of editor.list()) editor.remove(tool.id)
          editor.add({ name:"workspace_read", description:"Read one text file in the approved workspace after owner approval.",
            input:{type:"object",properties:{path:{type:"string"}},required:["path"],additionalProperties:false},
            execute:async ({path}, context) => {
              if (typeof path !== "string" || isAbsolute(path)) throw new Error("workspace_path_required")
              const resolved = await realpath(join(workspace,path))
              const inside = relative(workspace,resolved)
              if (!inside || inside === ".." || inside.startsWith(".." + sep) || isAbsolute(inside)) throw new Error("outside_workspace")
              await ctx.permission.assert({sessionID:context.sessionID,agent:context.agent,action:"workspace_read",
                resources:[inside],save:[],source:{type:"tool",messageID:context.messageID,id:context.id}})
              const file = await open(resolved, constants.O_RDONLY | constants.O_NOFOLLOW)
              try {
                const info = await file.stat()
                if (!info.isFile() || info.size > 65536) throw new Error("unsupported_file")
                const buffer = Buffer.alloc(65536)
                const {bytesRead} = await file.read(buffer,0,buffer.length,0)
                return {content:buffer.subarray(0,bytesRead).toString("utf8")}
              } finally { await file.close() }
            }})
          editor.add({ name:"render_surface",description:"Present registered A2UI operations for server validation.",
            input:{type:"object",properties:{a2ui_operations:{type:"array",maxItems:64,items:{type:"object"}}},required:["a2ui_operations"],additionalProperties:false},
            execute:async (content,context) => {
              if (context.sessionID !== session?.id || !Array.isArray(content.a2ui_operations) ||
                  Buffer.byteLength(JSON.stringify(content)) > 32768) throw new Error("invalid_surface")
              send("surface",{messageId:context.messageID,content:{a2ui_operations:content.a2ui_operations}})
              return {content:"Submitted surface for validation."}
            }})
        })
        if (allowed.includes("delegate_codex")) await registerDelegation(ctx, async (call, signal) => {
          if (call.parentSessionId !== session?.id) throw new Error("session_mismatch")
          const endpoint = trustedURL(input.delegateURL)
          if (endpoint.pathname !== "/api/v1/drivers/codex/delegate") throw new Error("invalid_delegation_endpoint")
          send("tool",{status:"delegation_authorized",parentMessageId:call.parentMessageId,delegationId:call.delegationId})
          const response = await fetch(endpoint,{method:"POST",redirect:"error",signal,
            headers:{"content-type":"application/json",authorization:"Bearer " + input.delegateToken},body:JSON.stringify(call)})
          if (!response.ok) throw new Error("delegation_denied")
          const body = await response.json()
          if (typeof body.text !== "string") throw new Error("invalid_delegation")
          return body.text
        })
      }})
      // The server owns provider identity. Drop platform update, discovery and redirected traffic.
      const requestModel = async (request, init) => {
        const req = new Request(request, init)
        const url = new URL(req.url)
        if (url.origin !== upstream.origin || url.pathname !== upstream.pathname.replace(/\/$/,"") + "/chat/completions" ||
            url.search || req.method !== "POST") throw new Error("unapproved_request")
        req.headers.delete("authorization")
        if (input.provider.apiKey) req.headers.set("authorization","Bearer " + input.provider.apiKey)
        return fetch(req,{redirect:"error"})
      }
      const rules = [{action:"*",resource:"*",effect:"deny"},
        ...allowed.map(action=>({action,resource:"*",effect:action === "render_surface" ? "allow" : "ask"}))]
      const managed = process.platform === "darwin"
        ? ["/Library/Application Support/opencode","/Library/Managed Preferences/ai.opencode.managed.plist",
          join("/Library/Managed Preferences",userInfo().username,"ai.opencode.managed.plist")]
        : ["/etc/opencode"]
      for (const path of managed) {
        try { await lstat(path); throw new Error("system_opencode_configuration_present") }
        catch (error) { if (error.code !== "ENOENT") throw error }
      }
      const cfgDir = await mkdtemp(join(privateDir, "host-config-"))
      try {
        host = await OpenCode.create({database:{path:input.database},events:{persist:true},plugins:[isolated],
          config:{project:false,claude:false,directory:cfgDir,content:JSON.stringify({plugins:[],providers:catalog,model:model.providerID + "/" + model.id})},
          fetch:requestModel})
        if (stopping) return "cancelled"
        session = input.nativeId ? await host.sessions.get({sessionID:input.nativeId}) :
          await host.sessions.create({location:{directory:workspace},model,permissions:rules})
        if (session.location.directory !== workspace || session.model?.providerID !== model.providerID ||
            session.model?.id !== model.id) throw new Error("session_binding_mismatch")
        send("bound",{nativeId:session.id})
        const stream = host.events.subscribe({}, {signal:eventsAbort.signal})
        let prompted = false
        for await (const event of stream) {
          if (event.type === "server.connected" && !prompted) {
            if (stopping) return "cancelled"
            prompted = true
            await host.sessions.prompt({sessionID:session.id,text:input.prompt})
            if (stopping) await cancel()
            continue
          }
          if (!prompted || event.data?.sessionID !== session.id) continue
          const data = event.data
          switch (event.type) {
            case "session.text.delta": send("text",{text:data.delta}); break
            case "session.tool.called":
            case "session.tool.success":
            case "session.tool.failed":
              send("tool",{status:{"session.tool.called":"called","session.tool.success":"completed","session.tool.failed":"failed"}[event.type],
                parentMessageId:data.assistantMessageID,delegationId:data.id})
              break
            case "permission.asked":
              if (!allowed.includes(data.action)) { await host.permission.reply({sessionID:session.id,requestID:data.id,decision:"reject"}); break }
              requests.add(data.id)
              send("permission",{requestId:data.id,label:data.action})
              break
            case "session.execution.succeeded": return "completed"
            case "session.execution.failed": return "failed"
            case "session.execution.interrupted": return "cancelled"
          }
        }
        return "indeterminate"
      } finally {
        requests.clear()
        eventsAbort.abort()
        await host?.close()
        await rmdir(cfgDir).catch(() => undefined)
      }
    },
  }
}
