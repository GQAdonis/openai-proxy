import assert from "node:assert/strict"
import { subscriptionRequest } from "../../plugin/src/subscription-request.ts"

const url = "http://127.0.0.1:8181/api/v1/drivers/codex/v1/chat/completions"
const headers = { "content-type": "application/json", "content-length": "999" }
const request = body => new Request(url, { method: "POST", headers, body: JSON.stringify(body) })
const base = { model: "gpt-fixture", stream: true, stream_options: { include_usage: true } }
const messages = [{ role: "system", content: "synthetic system" }, { role: "user", content: "synthetic prompt" }]

const projected = await subscriptionRequest(request({ ...base, messages, max_completion_tokens: 2048,
  tools: [{ type: "function", function: { name: "unavailable_tool" } }],
}))
assert.deepEqual(await projected.json(), { ...base, messages })
assert.equal(projected.headers.get("content-length"), null)
assert.equal(projected.redirect, "error")
assert.equal(projected.url, url)

const textParts = [{ type: "text", text: "one" }, { type: "text", text: "\ntwo" }]
const text = await subscriptionRequest(request({ ...base, messages: [{ role: "user", content: textParts }] }))
assert.deepEqual((await text.json()).messages, [{ role: "user", content: "one\ntwo" }])

const invalidMessages = [
  [{ role: "tool", content: "fixture" }],
  [{ role: "assistant", content: "fixture", tool_calls: [{ id: "synthetic" }] }],
  [{ role: "user", content: [{ type: "tool_use", text: "fixture" }] }],
  [{ role: "user", content: [{ type: "image_url", image_url: { url: "https://example.com/synthetic" } }] }],
  [{ role: "user", content: [{ type: "text", text: "fixture", tool_use: {} }] }],
]
for (const rejected of invalidMessages) {
  await assert.rejects(subscriptionRequest(request({ ...base, messages: rejected })), /only text messages/)
}
await assert.rejects(subscriptionRequest(request({ ...base, messages, tool_choice: "required" })), /unsupported options/)
