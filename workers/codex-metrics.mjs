import { constants } from "node:fs"
import { open } from "node:fs/promises"
import { join } from "node:path"

// Keep this allowlist independent of Codex's resource, scope and exemplar data.
const histograms = new Map([
  ["codex.startup.phase.duration_ms", "startup"],
  ["codex.auth.selection.duration", "auth_selection"],
  ["codex.auth_storage.duration", "auth_storage"],
  ["codex.auth_storage.refresh_persist.duration", "auth_refresh_persist"],
  ["codex.remote_models.load_cache.duration_ms", "model_cache_load"],
  ["codex.remote_models.fetch_update.duration_ms", "model_fetch"],
  ["codex.sqlite.init.duration_ms", "sqlite_init"],
  ["codex.cloud_config_bundle.fetch.duration_ms", "cloud_config_fetch"],
])
const startupPhases = new Set(["thread_start_create_thread", "thread_start_total"])
const storageOperations = new Set(["load", "save", "delete"])
const sqlitePhases = new Set([
  "open_state", "migrate_state", "open_logs", "migrate_logs", "open_goals", "migrate_goals",
  "open_memories", "migrate_memories", "open_queue", "migrate_queue",
  "open_thread_history", "migrate_thread_history", "ensure_backfill_state", "post_init_query", "backfill_gate",
])
const threadSpans = new Map([
  ["app_server.thread_start.create_thread", "create_thread"],
  ["thread_spawn", "thread_spawn"],
  ["session_init", "session_init"],
  ["session_init.thread_persistence", "thread_persistence"],
  ["session_init.state_db", "state_db"],
  ["session_init.auth_mcp", "auth_mcp"],
  ["session_init.plugin_skill_warmup", "plugin_skill_warmup"],
  ["session_init.thread_name_lookup", "thread_name_lookup"],
  ["session_init.network_proxy", "network_proxy"],
  ["session_init.mcp_manager_init", "mcp_manager_init"],
  ["session_init.required_mcp_wait", "required_mcp_wait"],
  ["agents_md.refresh", "agents_md_refresh"],
  ["plugins_for_config", "plugin_discovery"],
  ["skills_for_config", "skill_discovery"],
  ["list_models", "model_discovery"],
])
const threadPhases = new Set(threadSpans.values())
const rows = value => Array.isArray(value) ? value : []

export function isEarlyMetric(stage, operation) {
  return (stage === "native_bootstrap" && ["version_probe", "until_telemetry", "launcher_spawn"].includes(operation)) ||
    (stage === "sqlite_init" && sqlitePhases.has(operation)) ||
    (stage === "cloud_config_fetch" && operation === "all") ||
    (stage === "thread_init" && threadPhases.has(operation))
}

export async function recordEarlyMetrics(records, directory = process.cwd()) {
  const metrics = records.flatMap(({stage, operation, count, durationMs}) =>
    isEarlyMetric(stage, operation) && Number.isInteger(count) && count > 0 && count <= 100_000 &&
    Number.isFinite(durationMs) && durationMs >= 0 && durationMs <= 3_600_000
      ? [{stage, operation, count, durationMs}] : [])
  if (!metrics.length) return
  try {
    const flags = constants.O_APPEND | constants.O_CREAT | constants.O_WRONLY | constants.O_NOFOLLOW |
      constants.O_NONBLOCK
    const file = await open(join(directory, "codex-startup.metrics.jsonl"), flags, 0o600)
    try {
      const metadata = await file.stat()
      if (metadata.isFile() && (metadata.mode & 0o077) === 0) {
        await file.writeFile(JSON.stringify({recordedAt:Date.now(), metrics}) + "\n")
      }
    } finally {
      await file.close()
    }
  } catch {
    // A local diagnostic sink must not change the result of a chat.
  }
}

export function readMetrics(body) {
  const result = []
  for (const resource of rows(body?.resourceMetrics)) {
    for (const scope of rows(resource?.scopeMetrics)) {
      for (const metric of rows(scope?.metrics)) {
        const stage = histograms.get(metric?.name)
        if (!stage) continue
        for (const point of rows(metric.histogram?.dataPoints)) {
          const attributes = rows(point?.attributes)
          const label = name => attributes.find(entry => entry?.key === name)?.value?.stringValue
          let operation = "all"
          if (stage === "startup") {
            operation = label("phase")
            if (!startupPhases.has(operation)) continue
          } else if (stage === "sqlite_init") {
            operation = label("phase")
            if (!sqlitePhases.has(operation)) continue
          } else if (stage === "auth_storage") {
            const storage = label("operation")
            if (storageOperations.has(storage)) operation = storage
          } else if (stage === "auth_selection" && label("operation") === "model") {
            operation = "model"
          }
          const rawCount = point?.count
          if (typeof rawCount !== "number" && !(typeof rawCount === "string" && /^\d{1,6}$/.test(rawCount))) continue
          const count = Number(rawCount)
          const durationMs = point?.sum
          if (!Number.isInteger(count) || count < 1 || count > 100_000 ||
              !Number.isFinite(durationMs) || durationMs < 0 || durationMs > 3_600_000) continue
          result.push({stage, operation, count, durationMs,
            delta:metric.histogram.aggregationTemporality === 1})
        }
      }
    }
  }
  return result
}

function exporterStart(body) {
  for (const resource of rows(body?.resourceMetrics)) {
    for (const scope of rows(resource?.scopeMetrics)) {
      for (const metric of rows(scope?.metrics)) {
        if (metric?.name !== "codex.process.start") continue
        for (const point of rows(metric.sum?.dataPoints)) {
          const started = point?.startTimeUnixNano
          if (typeof started === "string" && /^\d{16,20}$/.test(started)) {
            return Number(BigInt(started) / 1_000_000n)
          }
        }
      }
    }
  }
}

function traceDurations(body) {
  const result = []
  for (const resource of rows(body?.resourceSpans)) {
    for (const scope of rows(resource?.scopeSpans)) {
      for (const span of rows(scope?.spans)) {
        const operation = threadSpans.get(span?.name)
        if (!operation || typeof span.startTimeUnixNano !== "string" ||
            typeof span.endTimeUnixNano !== "string" ||
            !/^\d{16,20}$/.test(span.startTimeUnixNano) ||
            !/^\d{16,20}$/.test(span.endTimeUnixNano)) continue
        const duration = BigInt(span.endTimeUnixNano) - BigInt(span.startTimeUnixNano)
        if (duration >= 0n && duration <= 3_600_000_000_000n) {
          result.push({stage:"thread_init", operation, count:1, durationMs:Number(duration) / 1_000_000})
        }
      }
    }
  }
  return result
}

export function receiveMetrics() {
  const path = "/v1/metrics/" + crypto.randomUUID()
  const spansPath = "/v1/traces/" + crypto.randomUUID()
  const latest = new Map()
  const threadTimings = new Map()
  let spawnWallMs
  let exporterWallMs
  const server = Bun.serve({
    hostname:"127.0.0.1", port:0, maxRequestBodySize:1_048_576,
    async fetch(request) {
      const pathname = new URL(request.url).pathname
      if ((pathname !== path && pathname !== spansPath) || request.method !== "POST") return new Response(null, {status:404})
      try {
        const body = await request.json()
        if (pathname === spansPath) {
          for (const row of traceDurations(body)) {
            const previous = threadTimings.get(row.operation)
            if (previous) {
              previous.count += 1
              previous.durationMs += row.durationMs
            } else {
              threadTimings.set(row.operation, row)
            }
          }
          return Response.json({})
        }
        exporterWallMs ??= exporterStart(body)
        const batch = new Map()
        for (const row of readMetrics(body)) {
          const key = row.stage + ":" + row.operation
          const current = batch.get(key)
          if (current) {
            current.count += row.count
            current.durationMs += row.durationMs
          } else {
            batch.set(key, row)
          }
        }
        for (const [key, row] of batch) {
          const previous = row.delta ? latest.get(key) : null
          latest.set(key, {
            stage:row.stage, operation:row.operation,
            count:row.count + (previous?.count ?? 0),
            durationMs:row.durationMs + (previous?.durationMs ?? 0),
          })
        }
        return Response.json({})
      } catch {
        return new Response(null, {status:400})
      }
    },
    error() {
      return new Response(null, {status:500})
    },
  })
  return {
    endpoint:"http://127.0.0.1:" + server.port + path,
    startupSpansEndpoint:"http://127.0.0.1:" + server.port + spansPath,
    startExec() {
      spawnWallMs = Date.now()
    },
    drain(send) {
      server.stop(true)
      const durationMs = exporterWallMs - spawnWallMs
      if (durationMs >= 0 && durationMs <= 3_600_000) {
        send("diagnostic_metrics", {stage:"native_bootstrap", operation:"until_telemetry", count:1, durationMs})
      }
      for (const row of latest.values()) send("diagnostic_metrics", row)
      for (const row of threadTimings.values()) send("diagnostic_metrics", row)
    },
  }
}
