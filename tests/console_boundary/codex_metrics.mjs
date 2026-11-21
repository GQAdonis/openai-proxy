import assert from "node:assert/strict"
import { mkdtemp, readFile, rm, stat, symlink } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { receiveMetrics, recordEarlyMetrics } from "../../workers/codex-metrics.mjs"

const privateFixture = "discard-this-synthetic-prompt-and-credential"
const label = (key, stringValue) => ({key, value:{stringValue}})
const histogram = (name, count, sum, ...attributes) => ({
  name, unit:"ms", histogram:{aggregationTemporality:1, dataPoints:[{
    count:String(count), sum, attributes:[...attributes, label("secret", privateFixture)],
    exemplars:[{filteredAttributes:[label("prompt", privateFixture)]}],
  }]},
})
const batch = metrics => JSON.stringify({resourceMetrics:[{
  resource:{attributes:[label("account", privateFixture)]},
  scopeMetrics:[{scope:{name:privateFixture}, metrics}],
}]})
const receiver = receiveMetrics()
receiver.startExec()
const rows = []
try {
  const invalid = await fetch(receiver.endpoint.replace(/\/[^/]+$/, "/other"), {method:"POST", body:"{}"})
  assert.equal(invalid.status, 404)
  for (const metrics of [
    [histogram("codex.startup.phase.duration_ms", 1, 15, label("phase", "thread_start_create_thread")),
      histogram("codex.startup.phase.duration_ms", 1, 55, label("phase", privateFixture)),
      histogram(privateFixture, 1, 100),
      {name:"codex.process.start", sum:{dataPoints:[{
        startTimeUnixNano:String(BigInt(Date.now()) * 1_000_000n),
        attributes:[label("secret", privateFixture)],
      }]}},
      histogram("codex.sqlite.init.duration_ms", 1, 30, label("phase", "backfill_gate")),
      histogram("codex.sqlite.init.duration_ms", 1, 88, label("phase", privateFixture)),
      histogram("codex.auth.selection.duration", 1, 2, label("operation", "model"))],
    [histogram("codex.auth.selection.duration", 2, 3, label("operation", "model")),
      histogram("codex.remote_models.fetch_update.duration_ms", 1, 25)],
  ]) {
    const response = await fetch(receiver.endpoint, {
      method:"POST", body:batch(metrics), headers:{"content-type":"application/json"},
    })
    assert.equal(response.status, 200)
    assert.deepEqual(await response.json(), {})
  }
} finally {
  receiver.drain((type, row) => rows.push({type, ...row}))
}
const bootstrap = rows.shift()
assert.equal(bootstrap.stage, "native_bootstrap")
assert.equal(bootstrap.operation, "until_telemetry")
assert.equal(bootstrap.count, 1)
assert(bootstrap.durationMs >= 0)
assert.deepEqual(rows, [
  {type:"diagnostic_metrics", stage:"startup", operation:"thread_start_create_thread", count:1, durationMs:15},
  {type:"diagnostic_metrics", stage:"sqlite_init", operation:"backfill_gate", count:1, durationMs:30},
  {type:"diagnostic_metrics", stage:"auth_selection", operation:"model", count:3, durationMs:5},
  {type:"diagnostic_metrics", stage:"model_fetch", operation:"all", count:1, durationMs:25},
])
assert(!JSON.stringify(rows).includes(privateFixture))
const folder = await mkdtemp(join(tmpdir(), "codex-startup-metrics-"))
try {
  await recordEarlyMetrics([
    {...bootstrap, extra:privateFixture},
    {stage:"sqlite_init", operation:privateFixture, count:1, durationMs:30},
  ], folder)
  const file = join(folder, "codex-startup.metrics.jsonl")
  const retained = await readFile(file, "utf8")
  assert(!retained.includes(privateFixture))
  assert.deepEqual(JSON.parse(retained).metrics, [{
    stage:bootstrap.stage, operation:bootstrap.operation, count:1, durationMs:bootstrap.durationMs,
  }])
  assert.equal((await stat(file)).mode & 0o077, 0)
  await rm(file)
  await symlink(join(folder, "unexpected"), file)
  await recordEarlyMetrics([bootstrap], folder)
  await assert.rejects(stat(join(folder, "unexpected")), {code:"ENOENT"})
} finally {
  await rm(folder, {recursive:true, force:true})
}
console.log("Codex metrics retained only allowlisted stages and numbers")
