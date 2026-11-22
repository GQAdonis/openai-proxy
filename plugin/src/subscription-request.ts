const roles = new Set(["system", "developer", "user", "assistant"])
const requestFields = new Set(["model", "messages", "stream", "stream_options", "tools", "max_completion_tokens"])

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function textOnly(): never {
  throw new Error("The Codex subscription provider supports only text messages")
}

function textContent(content: unknown): string {
  if (typeof content === "string") return content
  if (!Array.isArray(content) || !content.length) return textOnly()
  return content.map((part: unknown) => {
    if (!isRecord(part) || Object.keys(part).length !== 2 || part.type !== "text" || typeof part.text !== "string") {
      return textOnly()
    }
    return part.text
  }).join("")
}

function textMessage(message: unknown) {
  if (!isRecord(message) || Object.keys(message).length !== 2 || typeof message.role !== "string" || !roles.has(message.role)) {
    return textOnly()
  }
  return { role: message.role, content: textContent(message.content) }
}

export async function subscriptionRequest(request: Request): Promise<Request> {
  let body: unknown
  try {
    body = await request.json()
  } catch {
    throw new Error("The Codex subscription provider requires a JSON request")
  }
  if (!isRecord(body) || Object.keys(body).some(key => !requestFields.has(key)) ||
      typeof body.model !== "string" || !Array.isArray(body.messages) ||
      (body.tools !== undefined && !Array.isArray(body.tools))) {
    throw new Error("The Codex subscription provider received unsupported options")
  }
  // Offers are not tool calls. The text-only endpoint cannot offer local tools,
  // and the native Codex SDK controls its own output budget.
  const { tools: _offers, max_completion_tokens: _hostBudget, messages, ...options } = body
  const headers = new Headers(request.headers)
  headers.delete("content-length")
  return new Request(request, {
    headers, body: JSON.stringify({ ...options, messages: messages.map(textMessage) }), redirect: "error",
  })
}
