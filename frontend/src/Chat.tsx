import { useEffect, useRef, useState, type FormEvent } from 'react'
import {
  api,
  ApiError,
  errorText,
  readEvents,
  turnPath,
  type Connection,
  type Session,
  type TurnDetails,
} from './api'
import { Button } from './components/ui/button'
import { Surface, type SurfaceAction } from './Surface'
import { project } from './projection'

type Entry = {
  key: string
  thread: string
  turn: string
  prompt: string
  text: string
  state: string
  safeRetry: boolean
  surfaces: Record<string, unknown>
  reason?: string
  notice?: string
  permission?: string
}
const terminal = new Set(['completed', 'cancelled', 'failed', 'indeterminate'])
function outcome(row: Entry) {
  if (row.state === 'failed')
    return row.text
      ? 'Reply interrupted after partial output'
      : 'Failed before replying'
  const labels: Record<string, string> = {
    admitting: 'Sending',
    admitted: 'Waiting for a reply',
    running: 'Replying',
    reconnecting: 'Reconnecting',
    disconnected: 'Connection interrupted',
    expired: 'Replay window expired',
    cancelling: 'Stop requested',
    cancelled: 'Stopped',
    completed: 'Complete',
    indeterminate: 'Outcome uncertain',
  }
  return labels[row.state] ?? row.state
}
export function Chat({
  session,
  connection,
}: {
  session: Session
  connection: Connection
}) {
  const [entries, setEntries] = useState<Entry[]>([]),
    [thread, setThread] = useState(''),
    [draft, setDraft] = useState('')
  const available = connection.drivers.filter((d) => d.available)
  const [driver, setDriver] = useState(
    connection.defaultDriver ||
      available[0]?.id ||
      connection.drivers[0]?.id ||
      '',
  )
  const [model, setModel] = useState(
    connection.defaultModel || connection.models[0] || '',
  )
  const [instructions, setInstructions] = useState(''),
    [temperature, setTemperature] = useState(''),
    [maxTokens, setMaxTokens] = useState('')
  const [busy, setBusy] = useState(false),
    [alert, setAlert] = useState(''),
    [announcement, setAnnouncement] = useState(''),
    [latest, setLatest] = useState(true)
  const restSurfaces = useRef(new Map<string, Set<string>>())
  const pending = useRef(false),
    list = useRef<HTMLDivElement>(null),
    composer = useRef<HTMLTextAreaElement>(null)
  const stick = useRef(true),
    alive = useRef(true),
    controllers = useRef(new Set<AbortController>()),
    cursors = useRef(new Map<string, number>())
  const actionKeys = useRef(
    new Map<
      string,
      { id: string; pending: boolean; done: boolean; action: SurfaceAction }
    >(),
  )
  const selected = connection.drivers.find((d) => d.id === driver)
  const controls = selected?.controls ?? []
  useEffect(() => {
    alive.current = true
    const set = controllers.current
    return () => {
      alive.current = false
      for (const c of set) c.abort()
    }
  }, [])
  useEffect(() => {
    if (stick.current && list.current)
      list.current.scrollTop = list.current.scrollHeight
  }, [entries])
  function controller() {
    const c = new AbortController()
    controllers.current.add(c)
    return c
  }
  function change(key: string, update: (e: Entry) => Entry) {
    if (alive.current)
      setEntries((rows) => rows.map((e) => (e.key === key ? update(e) : e)))
  }
  function locked(value: boolean) {
    pending.current = value
    if (alive.current) setBusy(value)
  }
  function finish(text: string) {
    locked(false)
    if (alive.current) {
      setAnnouncement(text)
      if (!document.querySelector('main.settings'))
        composer.current?.focus({ preventScroll: true })
    }
  }
  function addSurface(key: string, id: string, operations: unknown) {
    if (!id || id.length > 128 || operations === undefined) return
    change(key, (row) => {
      if (
        !Object.hasOwn(row.surfaces, id) &&
        Object.keys(row.surfaces).length >= 16
      )
        return row
      if (JSON.stringify(row.surfaces[id]) === JSON.stringify(operations))
        return row
      return { ...row, surfaces: { ...row.surfaces, [id]: operations } }
    })
  }
  async function stream(key: string, conversation: string, turn: string) {
    const c = controller(),
      path = turnPath(conversation, turn)
    let completed = false
    try {
      for (let attempt = 0; attempt < 3 && !completed; attempt++) {
        try {
          await readEvents(
            path,
            cursors.current.get(key) ?? 0,
            c.signal,
            (id, event) => {
              if (event.type === 'text')
                change(key, (row) => ({
                  ...row,
                  state: row.state === 'cancelling' ? row.state : 'running',
                  text: row.text + (event.text ?? ''),
                }))
              else if (event.type === 'surface' && event.messageId) {
                restSurfaces.current.get(key)?.add(event.messageId)
                addSurface(key, event.messageId, event.content?.a2ui_operations)
              } else if (event.type === 'status' && event.state)
                change(key, (row) => ({ ...row, state: event.state! }))
              else if (event.type === 'permission')
                change(key, (row) => ({
                  ...row,
                  permission: event.requestId,
                  notice: event.label ?? 'The driver requests permission.',
                }))
              else if (event.type === 'tool')
                change(key, (row) => ({
                  ...row,
                  notice: event.name
                    ? 'Tool · ' + event.name
                    : 'Tool ' + (event.status || 'activity'),
                }))
              else if (event.type === 'terminal') {
                const state = event.state ?? 'indeterminate'
                change(key, (row) => ({
                  ...row,
                  state,
                  reason: event.reason ?? event.code,
                  safeRetry: event.safeRetry === true,
                  permission: undefined,
                }))
                completed = true
                finish(
                  state === 'completed'
                    ? 'Reply complete.'
                    : 'Turn ' + state + '.',
                )
              }
              cursors.current.set(key, id)
            },
          )
        } catch (err) {
          if (c.signal.aborted) return
          if (err instanceof ApiError && [401, 403].includes(err.status)) {
            window.dispatchEvent(new Event('console-session-expired'))
            return
          }
          if (err instanceof ApiError && [409, 410].includes(err.status)) {
            change(key, (row) => ({
              ...row,
              state: 'expired',
              reason: 'Read the saved turn to recover its latest outcome.',
            }))
            setAnnouncement('Replay window expired.')
            return
          }
          if (attempt === 2) throw err
        }
        if (!completed && !c.signal.aborted && attempt < 2) {
          change(key, (row) => ({ ...row, state: 'reconnecting' }))
          await new Promise((resolve) =>
            setTimeout(resolve, 500 * (attempt + 1)),
          )
        }
      }
      if (!completed && !c.signal.aborted)
        throw new Error('The event stream ended before a terminal outcome.')
    } catch (err) {
      if (!c.signal.aborted)
        change(key, (row) => ({
          ...row,
          state: 'disconnected',
          reason: errorText(err),
        }))
    } finally {
      controllers.current.delete(c)
    }
  }
  async function send(prompt: string, previous?: Entry) {
    if (pending.current || !prompt.trim() || !selected?.available) return
    const key = previous?.key ?? crypto.randomUUID()
    let conversation = previous?.thread || thread
    locked(true)
    setAlert('')
    setAnnouncement('Sending message.')
    try {
      if (!conversation) {
        const result = await api<{ id: string }>(
          '/conversations',
          session,
          'POST',
          { driver, model, workspace: session.workspace },
        )
        conversation = result.id
        setThread(result.id)
      }
      if (!previous)
        setEntries((rows) => [
          ...rows,
          {
            key,
            thread: conversation,
            turn: '',
            prompt,
            text: '',
            state: 'admitting',
            safeRetry: false,
            surfaces: {},
          },
        ])
      const turn = await api<TurnDetails>(
        turnPath(conversation),
        session,
        'POST',
        {
          prompt,
          idempotencyKey: key,
          ...(instructions && controls.includes('instructions')
            ? { instructions }
            : {}),
          ...(temperature && controls.includes('temperature')
            ? { temperature: Number(temperature) }
            : {}),
          ...(maxTokens && controls.includes('maxTokens')
            ? { maxTokens: Number(maxTokens) }
            : {}),
        },
      )
      if (!alive.current) return
      change(key, (row) => ({
        ...row,
        turn: turn.id,
        state: turn.state,
        reason: undefined,
      }))
      setDraft('')
      stick.current = true
      setLatest(true)
      // AG-UI projects the admitted turn. REST owns text, replay and terminal state.
      // Prefer REST snapshots once they arrive so a slower projection cannot roll one back.
      const projection = controller()
      const observed = new Set<string>()
      restSurfaces.current.set(key, observed)
      void project(session, conversation, turn.id, projection.signal, {
        surface: (id, operations) => {
          if (!observed.has(id)) addSurface(key, id, operations)
        },
      })
        .catch(() => {})
        .finally(() => controllers.current.delete(projection))
      await stream(key, conversation, turn.id)
    } catch (err) {
      if (!alive.current) return
      setAlert(errorText(err))
      change(key, (row) => ({
        ...row,
        state: 'admission-unknown',
        reason: 'Check admission using the original request.',
      }))
      locked(false)
    }
  }
  async function stop(row: Entry) {
    try {
      await api(turnPath(row.thread, row.turn) + '/cancel', session, 'POST', {})
      change(row.key, (r) =>
        terminal.has(r.state) ? r : { ...r, state: 'cancelling' },
      )
      setAnnouncement('Stop requested.')
    } catch (err) {
      setAlert(errorText(err))
    }
  }
  async function recover(row: Entry) {
    try {
      const result = await api<TurnDetails>(
        turnPath(row.thread, row.turn),
        session,
      )
      if (result.sequence !== undefined)
        cursors.current.set(row.key, result.sequence)
      for (const snapshot of result.surfaces ?? [])
        addSurface(
          row.key,
          snapshot.messageId ?? '',
          snapshot.content?.a2ui_operations,
        )
      const permission = Object.entries(result.permissions ?? {})[0]
      change(row.key, (r) => ({
        ...r,
        permission: permission?.[0],
        notice: permission?.[1],
        state: result.state,
        text: result.text ?? r.text,
        reason: result.reason,
        safeRetry: result.safeRetry === true,
      }))
      if (terminal.has(result.state)) finish('Saved turn ' + result.state + '.')
      else if (result.sequence !== undefined || row.state !== 'expired')
        await stream(row.key, row.thread, row.turn)
      else
        change(row.key, (r) => ({
          ...r,
          state: 'expired',
          reason:
            'The turn is still active. Read the saved turn again for its outcome.',
        }))
    } catch (err) {
      setAlert(errorText(err))
    }
  }
  async function decide(row: Entry, decision: 'allow' | 'deny') {
    try {
      await api(
        turnPath(row.thread, row.turn) +
          '/permissions/' +
          encodeURIComponent(row.permission!),
        session,
        'POST',
        { decision },
      )
      change(row.key, (r) => ({
        ...r,
        permission: undefined,
        notice: 'Permission decision sent.',
      }))
    } catch (err) {
      setAlert(errorText(err))
    }
  }
  async function act(row: Entry, message: string, action: SurfaceAction) {
    const identity = JSON.stringify([
      row.thread,
      row.turn,
      message,
      action.surfaceId,
      action.sourceComponentId,
      action.name,
      action.context,
    ])
    const prior = actionKeys.current.get(identity)
    if (prior?.pending || prior?.done || pending.current) return
    const attempt = prior ?? {
      id: crypto.randomUUID(),
      pending: false,
      done: false,
      action,
    }
    attempt.pending = true
    actionKeys.current.set(identity, attempt)
    locked(true)
    setAlert('')
    setAnnouncement('Sending action.')
    const c = controller(),
      key = attempt.id
    try {
      await project(
        session,
        row.thread,
        row.turn,
        c.signal,
        {
          start(turn) {
            setEntries((rows) => [
              ...rows.filter((r) => r.key !== key),
              {
                key,
                thread: row.thread,
                turn,
                prompt: 'Action · ' + action.name,
                text: '',
                state: 'running',
                safeRetry: false,
                surfaces: {},
              },
            ])
          },
          text(delta) {
            change(key, (r) => ({ ...r, text: r.text + delta }))
          },
          surface: (id, ops) => addSurface(key, id, ops),
          finish(ok, reason) {
            attempt.done = ok
            if (ok)
              change(row.key, (r) => ({ ...r, notice: 'Action received.' }))
            change(key, (r) => ({
              ...r,
              state: ok ? 'completed' : 'failed',
              reason,
            }))
            setAnnouncement(ok ? 'Action completed.' : 'Action failed.')
          },
        },
        { userAction: attempt.action, idempotencyKey: attempt.id },
      )
    } catch (err) {
      if (!c.signal.aborted) {
        setAlert(errorText(err))
        change(key, (r) => ({ ...r, state: 'indeterminate' }))
      }
    } finally {
      attempt.pending = false
      controllers.current.delete(c)
      locked(false)
    }
  }
  function newChat() {
    setThread('')
    setEntries([])
    setDraft('')
    setAlert('')
    cursors.current.clear()
    restSurfaces.current.clear()
    actionKeys.current.clear()
    composer.current?.focus()
  }
  function submit(e: FormEvent) {
    e.preventDefault()
    void send(draft.trim())
  }
  const current = entries.at(-1)
  return (
    <main className="chat">
      <div className="pagehead">
        <div>
          <h1>Conversation</h1>
          <p className="subtle">{session.workspace} workspace</p>
        </div>
        <Button disabled={busy || !entries.length} onClick={newChat}>
          New chat
        </Button>
      </div>
      <div className="chat-layout">
        <aside className="rail" aria-label="Conversation settings">
          <h2>Session</h2>
          <label htmlFor="driver">Driver</label>
          <select
            id="driver"
            value={driver}
            disabled={!!thread || busy}
            onChange={(e) => setDriver(e.target.value)}
          >
            {connection.drivers.map((d) => (
              <option key={d.id} value={d.id} disabled={!d.available}>
                {d.id}
                {!d.available ? ' (unavailable)' : ''}
              </option>
            ))}
          </select>
          {!selected?.available && (
            <p role="status">
              {selected?.reason || 'No chat driver is available.'}
            </p>
          )}
          <label htmlFor="model">Model</label>
          <select
            id="model"
            value={model}
            disabled={!!thread || busy}
            onChange={(e) => setModel(e.target.value)}
          >
            {connection.models.map((m) => (
              <option key={m}>{m}</option>
            ))}
          </select>
          {controls.some((v) =>
            ['instructions', 'temperature', 'maxTokens'].includes(v),
          ) && (
            <details>
              <summary>Turn options</summary>
              {controls.includes('instructions') && (
                <label>
                  System instructions
                  <textarea
                    value={instructions}
                    onChange={(e) => setInstructions(e.target.value)}
                  />
                </label>
              )}
              {controls.includes('temperature') && (
                <label>
                  Temperature
                  <input
                    type="number"
                    min="0"
                    max="2"
                    step=".1"
                    value={temperature}
                    onChange={(e) => setTemperature(e.target.value)}
                  />
                </label>
              )}
              {controls.includes('maxTokens') && (
                <label>
                  Output limit
                  <input
                    type="number"
                    min="1"
                    value={maxTokens}
                    onChange={(e) => setMaxTokens(e.target.value)}
                  />
                </label>
              )}
            </details>
          )}
          <div className="connection-line">
            <span className="dot" />
            {connection.status}
          </div>
        </aside>
        <section className="conversation" aria-label="Messages">
          <div
            className="transcript"
            ref={list}
            role="region"
            aria-label="Conversation transcript"
            tabIndex={0}
            onScroll={(e) => {
              const el = e.currentTarget
              stick.current =
                el.scrollHeight - el.scrollTop - el.clientHeight < 56
              setLatest(stick.current)
            }}
          >
            {!entries.length && (
              <div className="empty">
                <div className="empty-mark" aria-hidden="true">
                  /
                </div>
                <h2>Start a conversation</h2>
                <p>
                  Write a message to{' '}
                  {selected?.available ? driver : 'your chat driver'}.
                </p>
              </div>
            )}
            {entries.map((row) => (
              <article className="turn" key={row.key}>
                <div className="speaker">You</div>
                <p className="message">{row.prompt}</p>
                <div className="speaker assistant">Assistant</div>
                {row.text && <p className="message">{row.text}</p>}
                {Object.entries(row.surfaces).map(([id, ops]) => (
                  <Surface
                    key={id}
                    disabled={busy}
                    operations={ops}
                    onAction={(a) => void act(row, id, a)}
                  />
                ))}
                <div className="turn-state">
                  {outcome(row)}
                  {row.reason && ' · ' + row.reason}
                </div>
                {row.notice && <p className="note">{row.notice}</p>}
                {row.permission && (
                  <div className="actions">
                    <Button onClick={() => decide(row, 'deny')}>Deny</Button>
                    <Button onClick={() => decide(row, 'allow')}>
                      Allow once
                    </Button>
                  </div>
                )}
                {row.state === 'admission-unknown' && (
                  <Button disabled={busy} onClick={() => send(row.prompt, row)}>
                    Check admission
                  </Button>
                )}
                {['expired', 'disconnected'].includes(row.state) && (
                  <Button onClick={() => recover(row)}>
                    {row.state === 'expired' ? 'Read saved turn' : 'Reconnect'}
                  </Button>
                )}
                {row.state === 'failed' && row.safeRetry && (
                  <Button disabled={busy} onClick={() => send(row.prompt)}>
                    Retry message
                  </Button>
                )}
              </article>
            ))}
          </div>
          {!latest && (
            <Button
              className="jump"
              onClick={() => {
                if (list.current)
                  list.current.scrollTop = list.current.scrollHeight
              }}
            >
              Jump to latest
            </Button>
          )}
          {alert && (
            <p className="error" role="alert">
              {alert}
            </p>
          )}
          <form className="composer" onSubmit={submit}>
            <label htmlFor="message">Message</label>
            <textarea
              id="message"
              ref={composer}
              rows={2}
              value={draft}
              placeholder="Write a message…"
              maxLength={65536}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
                  e.preventDefault()
                  void send(draft.trim())
                }
              }}
            />
            <div className="actions">
              <span className="shortcut">Ctrl / ⌘ + Enter</span>
              {busy && current?.turn && !terminal.has(current.state) ? (
                <Button
                  type="button"
                  disabled={current.state === 'cancelling'}
                  onClick={() => stop(current)}
                >
                  Stop reply
                </Button>
              ) : (
                <Button
                  tone="primary"
                  disabled={busy || !draft.trim() || !selected?.available}
                >
                  Send message
                </Button>
              )}
            </div>
          </form>
          <div className="sr-only" role="status">
            {announcement}
          </div>
        </section>
      </div>
    </main>
  )
}
