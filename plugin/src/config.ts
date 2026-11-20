import { Model, Provider } from "@opencode/plugin"

export const providerID = Provider.ID.make("codex")

export function serviceOrigin(): URL {
  const url = new URL(process.env.PROXY_CONSOLE_URL ?? "http://127.0.0.1:8181")
  const loopback = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)
  if (url.username || url.password || url.pathname !== "/" || url.search || url.hash ||
      !(url.protocol === "https:" || (loopback && url.protocol === "http:"))) {
    throw new Error("Configure a trusted service origin with HTTPS or loopback HTTP")
  }
  return url
}

export function authorization(): string {
  const token = process.env.PROXY_CONSOLE_TOKEN
  if (!token) throw new Error("A scoped service connection is required")
  return `Bearer ${token}`
}

export function request(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers)
  headers.set("authorization", authorization())
  return fetch(new URL(path, serviceOrigin()), {
    ...init, headers, redirect: "error", signal: init.signal ?? AbortSignal.timeout(5000),
  })
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null

function model(entry: unknown) {
  if (!isRecord(entry) || typeof entry.id !== "string" || !/^[a-zA-Z0-9._-]{1,128}$/.test(entry.id)) return []
  const id = Model.ID.make(entry.id)
  const info = Model.Info.default(providerID, id)
  return [{
    ...info, name: entry.id,
    capabilities: { tools: false, input: ["text"], output: ["text"] },
    limit: {
      context: typeof entry.context_length === "number" ? entry.context_length : info.limit.context,
      output: typeof entry.max_output_tokens === "number" ? entry.max_output_tokens : info.limit.output,
    },
  }]
}

export async function inventory() {
  try {
    const response = await request("/api/v1/drivers/codex/v1/models")
    if (!response.ok) return []
    const payload: unknown = await response.json()
    return isRecord(payload) && Array.isArray(payload.data) ? payload.data.flatMap(model) : []
  } catch {
    return []
  }
}
