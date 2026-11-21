import assert from "node:assert/strict"
import { receiveMetrics } from "../../workers/codex-metrics.mjs"

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
const rows = []
try {
  const invalid = await fetch(receiver.endpoint.replace(/\/[^/]+$/, "/other"), {method:"POST", body:"{}"})
  assert.equal(invalid.status, 404)
  for (const metrics of [
    [histogram("codex.startup.phase.duration_ms", 1, 15, label("phase", "thread_start_create_thread")),
      histogram("codex.startup.phase.duration_ms", 1, 55, label("phase", privateFixture)),
      histogram(privateFixture, 1, 100),
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
assert.deepEqual(rows, [
  {type:"diagnostic_metrics", stage:"startup", operation:"thread_start_create_thread", count:1, durationMs:15},
  {type:"diagnostic_metrics", stage:"auth_selection", operation:"model", count:3, durationMs:5},
  {type:"diagnostic_metrics", stage:"model_fetch", operation:"all", count:1, durationMs:25},
])
assert(!JSON.stringify(rows).includes(privateFixture))
console.log("Codex metrics retained only allowlisted stages and numbers")
