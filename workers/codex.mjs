import { Codex } from "@openai/codex-sdk"
import { isAbsolute, join } from "node:path"
import { readFile, realpath, unlink } from "node:fs/promises"
import { fileURLToPath } from "node:url"
import { execFileSync } from "node:child_process"
import { isEarlyMetric, receiveMetrics, recordEarlyMetrics } from "./codex-metrics.mjs"

// The pinned SDK exposes native errors as messages. Export only stable, redacted
// categories; an unrecognized message provides no evidence of an entitlement.
function failure(event) {
  const message = String(event.error?.message ?? event.message ?? "").slice(0, 2048)
  let code = "native_failure"
  if (/\b429\b|usage.limit|quota|rate.limit/i.test(message)) code = "subscription_quota"
  else if (/\b401\b|token.*(?:expired|revoked)|unauthorized/i.test(message)) code = "sign_in_required"
  else if (/\b403\b|\binsufficient_entitlement\b|not entitled|permission denied|forbidden/i.test(message)) code = "subscription_denied"
  else if (/model.*(?:unavailable|not found|not supported)|model_not_found/i.test(message)) code = "model_unavailable"
  return { status: "failed", code }
}

export function create(input, send) {
  const controller = new AbortController()
  return {
    cancel: async () => controller.abort(),
    permission: async () => { throw new Error("unsupported_permission") },
    async run() {
      if (!isAbsolute(input.codexHome ?? "") || !isAbsolute(input.codexPath ?? "")) throw new Error("unbound_codex")
      const binary = await realpath(input.codexPath)
      const home = await realpath(input.codexHome)
      const workspace = await realpath(input.workspace)
      // The supervisor gives each conversation a private working directory.
      // Keep Codex auth and history in its approved profile without inheriting
      // the machine user's extra HOME/.agents/skills discovery root.
      const env = { PATH: "/usr/bin:/bin", HOME: process.cwd(), CODEX_HOME: home,
        PROXY_CODEX_BINARY: binary, LANG: "C", NO_COLOR: "1" }
      const earlyMetrics = []
      const versionProbe = performance.now()
      if (execFileSync(binary, ["--version"], {env, timeout:3000, encoding:"utf8", maxBuffer:4096}).trim() !== "codex-cli 0.169.0") {
        throw new Error("codex_version_mismatch")
      }
      if (input.localMetrics === true) {
        earlyMetrics.push({stage:"native_bootstrap", operation:"version_probe",
          count:1, durationMs:performance.now() - versionProbe})
      }
      const disabled = ["shell_tool","unified_exec","experimental_use_unified_exec_tool","apps","plugins","hooks",
        "plugin_hooks","codex_hooks","multi_agent","multi_agent_v2","js_repl","code_mode","code_mode_host","image_generation",
        "in_app_browser","view_image","memory_tool","search_tool","tool_search","apply_patch_freeform","request_permissions_tool"]
      const config = {
        model_provider: "openai", forced_login_method: "chatgpt", allow_login_shell: false,
        features: Object.fromEntries(disabled.map(key => [key, false])),
        ...(input.accountId ? { forced_chatgpt_workspace_id: input.accountId } : {}),
        ...(input.credentialStore ? { cli_auth_credentials_store: input.credentialStore } : {}),
      }
      let metrics
      let spawnTimingPath
      if (input.localMetrics === true) {
        // Diagnostics must not make chat depend on the availability of a local port.
        try { metrics = receiveMetrics() } catch { /* optional diagnostics */ }
        spawnTimingPath = join(process.cwd(), ".codex-spawn-" + crypto.randomUUID())
        env.PROXY_CODEX_SPAWN_TIMING = spawnTimingPath
      }
      if (metrics) {
        // Collect during the turn as well as on shutdown. A short-lived exec
        // process may exit before the exporter would normally wake up.
        env.OTEL_METRIC_EXPORT_INTERVAL = "1000"
        env.OTEL_BSP_MAX_EXPORT_BATCH_SIZE = "64"
        env.OTEL_BSP_SCHEDULE_DELAY = "500"
        config.analytics = { enabled:false }
        config.otel = { exporter:"none", log_user_prompt:false,
          log_agent_responses:false,
          trace_exporter:{"otlp-http":{endpoint:metrics.startupSpansEndpoint, protocol:"json"}},
          metrics_exporter:{"otlp-http":{endpoint:metrics.endpoint, protocol:"json"}} }
      }
      const client = new Codex({codexPathOverride:fileURLToPath(new URL("./codex-exec", import.meta.url)), env, config,
        configOverrides:["mcp_servers={}", "model_providers={}", "hooks={}"]})
      const options = {model:input.model, workingDirectory:workspace, sandboxMode:"read-only",
        approvalPolicy:"never", skipGitRepoCheck:true, networkAccessEnabled:false, webSearchMode:"disabled"}
      const thread = input.nativeId ? client.resumeThread(input.nativeId, options) : client.startThread(options)
      const seen = new Map()
      let completed = false
      let metricsDrain
      let finishDrain
      const drainDeadline = metrics && new Promise(resolve => { finishDrain = resolve })
      async function consume() {
        try {
          metrics?.startExec()
          const {events} = await thread.runStreamed(input.prompt, {signal:controller.signal})
          for await (const event of events) {
            if (completed) continue
            if (event.type === "thread.started") {
              if (input.nativeId && event.thread_id !== input.nativeId) throw new Error("thread_mismatch")
              send("bound", {nativeId:event.thread_id})
            } else if (event.type.startsWith("item.") && event.item.type === "agent_message") {
              const prior = seen.get(event.item.id) ?? ""
              if (!event.item.text.startsWith(prior)) throw new Error("revised_snapshot")
              const delta = event.item.text.slice(prior.length)
              if (delta) send("text", {text:delta})
              seen.set(event.item.id, event.item.text)
            } else if (event.type.startsWith("item.") && ["command_execution","mcp_tool_call","file_change"].includes(event.item.type)) {
              send("tool", {status:event.type === "item.completed" ? "completed" : "observed"})
            } else if (event.type === "turn.completed") {
              const prompt = event.usage?.input_tokens
              const completion = event.usage?.output_tokens
              if (Number.isSafeInteger(prompt) && prompt >= 0 && Number.isSafeInteger(completion) &&
                  completion >= 0 && Number.isSafeInteger(prompt + completion)) {
                send("usage", {prompt_tokens:prompt, completion_tokens:completion, total_tokens:prompt + completion})
              }
              if (!metrics) return "completed"
              completed = true
              // Read through native shutdown so its exporter can flush. A successful
              // turn remains successful even if local metrics shutdown overruns.
              metricsDrain = setTimeout(() => {
                finishDrain("completed")
                controller.abort()
              }, 3000)
            } else if (event.type === "turn.failed" || event.type === "error") return failure(event)
          }
          return completed ? "completed" : controller.signal.aborted ? "cancelled" : "indeterminate"
        } catch (error) {
          if (completed) return "completed"
          if (controller.signal.aborted) return "cancelled"
          throw error
        }
      }
      try {
        return await (metrics ? Promise.race([consume(), drainDeadline]) : consume())
      } finally {
        clearTimeout(metricsDrain)
        metrics?.drain((type, row) => {
          if (isEarlyMetric(row.stage, row.operation)) earlyMetrics.push(row)
          else send(type, row)
        })
        if (earlyMetrics.length) {
          let logDeadline
          const persist = async () => {
            if (spawnTimingPath) {
              try {
                const duration = (await readFile(spawnTimingPath, "utf8")).trim()
                if (/^\d{1,12}$/.test(duration)) {
                  earlyMetrics.push({stage:"native_bootstrap", operation:"launcher_spawn",
                    count:1, durationMs:Number(duration) / 1000})
                }
                await unlink(spawnTimingPath)
              } catch { /* The portable shell launcher does not emit this measurement. */ }
            }
            await recordEarlyMetrics(earlyMetrics)
          }
          try {
            await Promise.race([persist(), new Promise(resolve => {
              logDeadline = setTimeout(resolve, 250)
            })])
          } finally {
            clearTimeout(logDeadline)
          }
        }
      }
    },
  }
}
