import { useEffect, useState, type ReactNode } from 'react'
import {
  api,
  ApiError,
  asText,
  canAdmin,
  errorText,
  type Area,
  type Connection,
  type Session,
} from './api'
import { ConfigEditor } from './ConfigEditor'
import { Button } from './components/ui/button'

function title(key: string) {
  return key
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/[_-]/g, ' ')
    .replace(/^./, (v) => v.toUpperCase())
}
function detail(value: unknown): ReactNode {
  if (value === null || value === undefined)
    return <span className="subtle">None</span>
  if (Array.isArray(value)) {
    if (!value.length) return <span className="subtle">None</span>
    const list = (
      <ul className="values">
        {value.map((v, i) => (
          <li key={i}>{detail(v)}</li>
        ))}
      </ul>
    )
    return value.length > 8 ? (
      <details>
        <summary>{value.length} entries</summary>
        {list}
      </details>
    ) : (
      list
    )
  }
  if (typeof value === 'object')
    return (
      <dl className="nested-facts">
        {Object.entries(value).map(([key, v]) => (
          <div key={key}>
            <dt>{title(key)}</dt>
            <dd>{detail(v)}</dd>
          </div>
        ))}
      </dl>
    )
  return typeof value === 'boolean' ? (value ? 'Yes' : 'No') : String(value)
}
function time(value: string | number | undefined) {
  if (value === undefined || value === null) return undefined
  const date = new Date(
    /^\d+$/.test(String(value)) ? Number(value) * 1000 : value,
  )
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString()
}
const integrations = [
  ['opencode_plugin', 'OpenCode · Native plugin'],
  ['opencode_provider', 'OpenCode · Static provider'],
  ['zed_acp', 'Zed · ACP agent'],
  ['jetbrains_acp', 'JetBrains · ACP agent'],
  ['generic_acp', 'Generic ACP descriptor'],
  ['codex_mcp', 'Codex CLI · MCP server'],
  ['claude_mcp', 'Claude Code · MCP server'],
  ['opencode_mcp', 'OpenCode · MCP server'],
  ['zed_mcp', 'Zed · MCP server'],
  ['vscode_mcp', 'VS Code / Copilot · MCP server'],
  ['generic_mcp', 'Generic MCP descriptor'],
]
type Preview = {
  id?: string
  status: string
  reason?: string
  changes?: unknown
  export?: unknown
  target?: string
  version?: string
  activation?: string
}
function download(content: unknown, name: string) {
  const text =
    typeof content === 'string' ? content : JSON.stringify(content, null, 2)
  const url = URL.createObjectURL(
    new Blob([text], { type: 'text/plain;charset=utf-8' }),
  )
  const link = document.createElement('a')
  link.href = url
  link.download = name + '-setup.txt'
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1500)
}
function Setup({ session, owner }: { session: Session; owner: boolean }) {
  const [integration, setIntegration] = useState('opencode_plugin'),
    [scope, setScope] = useState('project'),
    [transport, setTransport] = useState('stdio')
  const [destination, setDestination] = useState(owner ? 'service' : 'visitor')
  const [preview, setPreview] = useState<Preview | null>(null),
    [undo, setUndo] = useState(''),
    [message, setMessage] = useState(''),
    [busy, setBusy] = useState(false)
  function invalidate() {
    setPreview(null)
    setMessage('')
  }
  async function request(step: 'preview' | 'apply' | 'undo' | 'export') {
    if (busy) return
    setBusy(true)
    setMessage('')
    try {
      const body =
        step === 'apply'
          ? { id: preview?.id }
          : step === 'undo'
            ? { id: undo }
            : {
                integration,
                scope,
                ...(integration.endsWith('_mcp') ? { transport } : {}),
              }
      const result = await api<Preview>(
        (destination === 'visitor' ? '/me/setup/' : '/admin/setup/') + step,
        session,
        'POST',
        body,
      )
      if (step === 'apply') {
        setUndo(result.id ?? preview?.id ?? '')
        setPreview(null)
        setMessage(
          'Configuration applied. Reload the target client to pick up the change.',
        )
      } else if (step === 'undo') {
        setUndo('')
        setPreview(null)
        setMessage('Previous configuration restored.')
      } else {
        setPreview(result)
        if (step === 'export' && result.export !== undefined)
          download(result.export, integration)
      }
    } catch (err) {
      if (err instanceof ApiError && [409, 410].includes(err.status)) {
        setPreview(null)
        setMessage('Configuration changed. Create a new preview to continue.')
      } else setMessage(errorText(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <section className="settings-section">
      <h2>Client setup</h2>
      <p>Preview the changes for an editor or local client.</p>
      <div className="fields">
        <label>
          Client integration
          <select
            disabled={busy}
            value={integration}
            onChange={(e) => {
              setIntegration(e.target.value)
              invalidate()
            }}
          >
            {integrations.map(([key, label]) => (
              <option value={key} key={key}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Target machine
          <select
            disabled={busy}
            value={destination}
            onChange={(e) => {
              setDestination(e.target.value)
              invalidate()
            }}
          >
            {owner && <option value="service">Service host</option>}
            <option value="visitor">My machine · export</option>
          </select>
        </label>
        <label>
          Configuration scope
          <select
            value={scope}
            disabled={busy}
            onChange={(e) => {
              setScope(e.target.value)
              invalidate()
            }}
          >
            <option value="project">Project</option>
            <option value="global">Global</option>
          </select>
        </label>
        {integration.endsWith('_mcp') && (
          <label>
            Transport
            <select
              disabled={busy}
              value={transport}
              onChange={(e) => {
                setTransport(e.target.value)
                invalidate()
              }}
            >
              <option value="stdio">Standard I/O</option>
              <option value="http">Streamable HTTP</option>
            </select>
          </label>
        )}
      </div>
      <div className="actions">
        <Button
          disabled={busy}
          onClick={() =>
            request(destination === 'visitor' ? 'export' : 'preview')
          }
        >
          {destination === 'visitor' ? 'Export setup' : 'Preview changes'}
        </Button>
        {undo && (
          <Button disabled={busy} onClick={() => request('undo')}>
            Undo last change
          </Button>
        )}
      </div>
      {preview && (
        <div className="preview">
          <h3>
            {preview.status === 'ready'
              ? 'Proposed changes'
              : 'Setup ' + preview.status}
          </h3>
          {preview.target && (
            <p>
              Target · <code>{preview.target}</code>
            </p>
          )}
          {preview.version && <p>Client version · {preview.version}</p>}
          {preview.reason && <p>{preview.reason}</p>}
          <pre>{asText(preview.changes ?? preview.export)}</pre>
          {preview.activation && <p>{preview.activation}</p>}
          <div className="actions">
            {destination === 'service' &&
              preview.status === 'ready' &&
              preview.id && (
                <Button
                  disabled={busy}
                  tone="primary"
                  onClick={() => request('apply')}
                >
                  Apply preview
                </Button>
              )}
            {preview.export !== undefined && (
              <Button onClick={() => download(preview.export, integration)}>
                Download export
              </Button>
            )}
          </div>
        </div>
      )}
      <p role="status" className="note">
        {busy ? 'Contacting the service…' : message}
      </p>
    </section>
  )
}
export function Settings({
  session,
  connection,
  reload,
}: {
  session: Session
  connection: Connection
  reload: () => Promise<void>
}) {
  const admin = canAdmin(session)
  const [areas, setAreas] = useState<Area[] | null>(null),
    [active, setActive] = useState('connection'),
    [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false),
    [filter, setFilter] = useState(''),
    [attempt, setAttempt] = useState(0)
  const [model, setModel] = useState(connection.defaultModel || ''),
    [driver, setDriver] = useState(connection.defaultDriver || '')
  useEffect(() => {
    if (!admin) return
    const c = new AbortController()
    api<{ areas: Area[] }>(
      '/admin/settings',
      session,
      'GET',
      undefined,
      c.signal,
    )
      .then((r) => setAreas(r.areas))
      .catch((err) => {
        if (!c.signal.aborted) setMessage(errorText(err))
      })
    return () => c.abort()
  }, [session, admin, attempt])
  async function manage(kind: 'refresh' | 'reload' | 'defaults') {
    setBusy(true)
    setMessage(kind === 'refresh' ? 'Refreshing the connection…' : 'Updating…')
    try {
      if (kind === 'defaults')
        await api('/admin/settings', session, 'PATCH', {
          ...(model ? { defaultModel: model } : {}),
          ...(driver ? { defaultDriver: driver } : {}),
        })
      else
        await api(
          '/me/connection/' + kind,
          session,
          'POST',
          kind === 'refresh' ? { generation: connection.generation } : {},
        )
      await reload()
      setAttempt((v) => v + 1)
      setMessage(
        kind === 'defaults'
          ? 'Defaults saved.'
          : kind === 'reload'
            ? 'Connection reloaded.'
            : 'Refresh requested.',
      )
    } catch (err) {
      setMessage(errorText(err))
    } finally {
      setBusy(false)
    }
  }
  const area = areas?.find((a) => a.id === active)
  const items = [
    { id: 'connection', title: 'My connection' },
    ...(admin ? [{ id: 'defaults', title: 'Chat defaults' }] : []),
    { id: 'setup', title: 'Client setup' },
    ...(areas ?? []).map((a) => ({ id: 'area/' + a.id, title: a.title })),
  ]
  const descriptor = active.startsWith('area/')
    ? areas?.find((a) => a.id === active.slice(5))
    : area
  return (
    <main className="settings">
      <div className="pagehead">
        <div>
          <h1>Settings</h1>
          <p className="subtle">
            {session.mode} deployment · {session.role}
          </p>
        </div>
        <Button
          onClick={async () => {
            try {
              await reload()
              setAttempt((v) => v + 1)
              setMessage('Settings updated.')
            } catch (err) {
              setMessage(errorText(err))
            }
          }}
        >
          Reload status
        </Button>
      </div>
      <div className="settings-layout">
        <aside className="settings-nav" aria-label="Settings sections">
          <label htmlFor="filter">Find a setting</label>
          <input
            type="search"
            id="filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter sections…"
          />
          <nav aria-label="Settings areas">
            {items
              .filter((a) =>
                a.title.toLowerCase().includes(filter.toLowerCase()),
              )
              .map((a) => (
                <button
                  key={a.id}
                  type="button"
                  aria-current={active === a.id ? 'page' : undefined}
                  onClick={() => setActive(a.id)}
                >
                  {a.title}
                </button>
              ))}
          </nav>
          {admin && areas === null && !message && (
            <p role="status">Loading service settings…</p>
          )}
        </aside>
        <div className="settings-content">
          {active === 'connection' && (
            <section className="settings-section">
              <h2>My connection</h2>
              <dl className="facts">
                {Object.entries({
                  Status: connection.status,
                  Workspace: connection.workspace,
                  Generation: connection.generation,
                  'Credential source': connection.source,
                  'Last refresh': time(connection.lastRefresh),
                  'Last checked': time(connection.lastReconciled),
                  'Retry after': connection.retryAfterSeconds
                    ? connection.retryAfterSeconds + ' seconds'
                    : undefined,
                  Reason: connection.reason,
                })
                  .filter(([, v]) => v !== undefined)
                  .map(([k, v]) => (
                    <div key={k}>
                      <dt>{k}</dt>
                      <dd>{asText(v)}</dd>
                    </div>
                  ))}
              </dl>
              <h3>Chat drivers</h3>
              <div className="driver-list">
                {connection.drivers.map((d) => (
                  <div key={d.id}>
                    <strong>{d.id}</strong>
                    <span>
                      {d.available ? 'Available' : d.reason || 'Unavailable'}
                    </span>
                  </div>
                ))}
              </div>
              <div className="actions">
                {connection.capabilities.refresh && (
                  <Button disabled={busy} onClick={() => manage('refresh')}>
                    Refresh connection
                  </Button>
                )}
                {connection.capabilities.reload && (
                  <Button disabled={busy} onClick={() => manage('reload')}>
                    Check for new login
                  </Button>
                )}
              </div>
            </section>
          )}
          {active === 'defaults' && admin && (
            <section className="settings-section">
              <h2>Chat defaults</h2>
              <p>Choose the defaults for new conversations.</p>
              <div className="fields">
                <label>
                  Default driver
                  <select
                    value={driver}
                    onChange={(e) => setDriver(e.target.value)}
                  >
                    <option value="">Keep current driver</option>
                    {connection.drivers.map((d) => (
                      <option key={d.id} disabled={!d.available}>
                        {d.id}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Default model
                  <select
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                  >
                    <option value="">Keep current model</option>
                    {connection.models.map((m) => (
                      <option key={m}>{m}</option>
                    ))}
                  </select>
                </label>
              </div>
              <Button
                tone="primary"
                disabled={busy || (!model && !driver)}
                onClick={() => manage('defaults')}
              >
                Save defaults
              </Button>
            </section>
          )}
          {active === 'setup' && <Setup session={session} owner={admin} />}
          {descriptor && (
            <section className="settings-section">
              <h2>{descriptor.title}</h2>
              <p className="status-line">{asText(descriptor.availability)}</p>
              <dl className="facts">
                {Object.entries({
                  'Effective value': descriptor.value,
                  Source: descriptor.source,
                  Scope: descriptor.scope,
                  'Takes effect': descriptor.activation,
                })
                  .filter(([, v]) => v !== undefined)
                  .map(([k, v]) => (
                    <div key={k}>
                      <dt>{k}</dt>
                      <dd>{detail(v)}</dd>
                    </div>
                  ))}
              </dl>
              {admin && (
                <ConfigEditor
                  key={descriptor.id}
                  session={session}
                  action={descriptor.action}
                  onApplied={() => setAttempt((v) => v + 1)}
                />
              )}
              {admin &&
                /opencode|acp|mcp|machine|setup|client/i.test(
                  descriptor.id,
                ) && (
                  <Button onClick={() => setActive('setup')}>
                    Open client setup
                  </Button>
                )}
            </section>
          )}
          {message && (
            <p role="status" className="banner">
              {message}
            </p>
          )}
        </div>
      </div>
    </main>
  )
}
