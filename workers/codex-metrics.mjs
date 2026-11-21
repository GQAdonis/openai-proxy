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
const rows = value => Array.isArray(value) ? value : []

export function isEarlyMetric(stage, operation) {
  return (stage === "native_bootstrap" && ["version_probe", "until_telemetry"].includes(operation)) ||
    (stage === "sqlite_init" && sqlitePhases.has(operation)) ||
    (stage === "cloud_config_fetch" && operation === "all")
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

export function receiveMetrics() {
  const path = "/v1/metrics/" + crypto.randomUUID()
  const latest = new Map()
  let spawnWallMs
  let exporterWallMs
  const server = Bun.serve({
    hostname:"127.0.0.1", port:0, maxRequestBodySize:262_144,
    async fetch(request) {
      if (new URL(request.url).pathname !== path || request.method !== "POST") return new Response(null, {status:404})
      try {
        const body = await request.json()
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
    },
  }
}
