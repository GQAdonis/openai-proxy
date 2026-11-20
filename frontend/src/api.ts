export type Session = {
  role: string
  mode: string
  workspace: string
  csrfToken: string
}
export type Driver = {
  id: string
  available: boolean
  reason?: string
  controls?: string[]
  retrySafe?: boolean
  models?: string[]
}
export type Connection = {
  status: string
  generation?: number
  mode: string
  capabilities: { refresh?: boolean; reload?: boolean }
  drivers: Driver[]
  models: string[]
  workspace: string
  defaultModel?: string
  defaultDriver?: string
  lastRefresh?: string
  lastReconciled?: number
  retryAfterSeconds?: number
  source?: string
  reason?: string
}
export type Area = {
  id: string
  title: string
  value: unknown
  source: string
  scope: string
  activation: string
  availability: string
  action?: unknown
}
export type TurnDetails = {
  id: string
  state: string
  text?: string
  safeRetry?: boolean
  sequence?: number
  permissions?: Record<string, string>
  surfaces?: TurnEvent[]
  reason?: string
}
export type TurnEvent = {
  type: string
  state?: string
  text?: string
  reason?: string
  safeRetry?: boolean
  label?: string
  code?: string
  status?: string
  messageId?: string
  content?: { a2ui_operations?: unknown }
  requestId?: string
  name?: string
}
export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message)
  }
}
export async function problem(response: Response) {
  let message = response.statusText || 'Request failed'
  try {
    const body = await response.json()
    const detail =
      body.message ?? body.error?.message ?? body.error ?? body.reason
    if (typeof detail === 'string') message = detail.slice(0, 400)
  } catch {
    /* A proxy or empty body can supply only the status. */
  }
  return new ApiError(response.status, message)
}
export async function api<T>(
  path: string,
  session?: Session,
  method = 'GET',
  body?: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const headers: Record<string, string> = { Accept: 'application/json' }
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  if (session && method !== 'GET') headers['X-CSRF-Token'] = session.csrfToken
  const response = await fetch('/api/v1' + path, {
    method,
    headers,
    credentials: 'same-origin',
    cache: 'no-store',
    signal,
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!response.ok) {
    if (response.status === 401 && session)
      window.dispatchEvent(new Event('console-session-expired'))
    throw await problem(response)
  }
  return response.status === 204
    ? (undefined as T)
    : ((await response.json()) as T)
}
export function turnPath(conversation: string, turn?: string) {
  const base = '/conversations/' + encodeURIComponent(conversation) + '/turns'
  return turn ? base + '/' + encodeURIComponent(turn) : base
}
export function errorText(error: unknown) {
  return error instanceof Error
    ? error.message
    : 'The service could not complete this request.'
}
export function asText(input: unknown): string {
  if (input === undefined || input === null) return 'Unavailable'
  return typeof input === 'string' ? input : JSON.stringify(input, null, 2)
}
export function canAdmin(session: Session) {
  return [
    'localowner',
    'serviceadmin',
    'administrator',
    'admin',
    'owner',
  ].includes(session.role.toLowerCase().replace(/[-_ ]/g, ''))
}
// This reader keeps the HTTP status (including an expired replay window) and sends the
// last accepted ID on every reconnect. No POST is involved in resuming delivery.
export async function readEvents(
  path: string,
  cursor: number,
  signal: AbortSignal,
  accept: (id: number, event: TurnEvent) => void,
) {
  const response = await fetch('/api/v1' + path + '/events', {
    credentials: 'same-origin',
    cache: 'no-store',
    signal,
    headers: {
      Accept: 'text/event-stream',
      ...(cursor ? { 'Last-Event-ID': String(cursor) } : {}),
    },
  })
  if (!response.ok) throw await problem(response)
  if (
    !response.headers.get('Content-Type')?.startsWith('text/event-stream') ||
    !response.body
  )
    throw new Error('The service did not open an event stream.')
  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader()
  let buffer = ''
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) return
      buffer += value
      let boundary: RegExpExecArray | null
      while ((boundary = /\r?\n\r?\n/.exec(buffer))) {
        const block = buffer.slice(0, boundary.index)
        buffer = buffer.slice(boundary.index + boundary[0].length)
        const lines = block.split(/\r?\n/)
        const data = lines
          .filter((line) => line.startsWith('data:'))
          .map((line) => line.slice(5).trimStart())
          .join('\n')
        if (!data) continue
        const id = Number(
          lines.find((line) => line.startsWith('id:'))?.slice(3),
        )
        if (!Number.isSafeInteger(id) || id < 1)
          throw new Error('The service returned an invalid replay cursor.')
        if (id <= cursor) continue
        const event: TurnEvent = JSON.parse(data)
        accept(id, event)
        cursor = id
        if (event.type === 'terminal') return
      }
      if (buffer.length > 128 * 1024)
        throw new Error('The service event exceeded the supported size.')
    }
  } finally {
    await reader.cancel().catch(() => {})
    reader.releaseLock()
  }
}
