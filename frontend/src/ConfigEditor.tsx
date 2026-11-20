import { useState } from 'react'
import { api, ApiError, asText, errorText, type Session } from './api'
import { Button } from './components/ui/button'

type Field = {
  field: string
  type?: string
  label?: string
  choices?: string[]
}
type Preview = {
  status: string
  id?: string
  changes?: unknown
  activation?: string
  reason?: string
  target?: string
}
const fields: Record<
  string,
  { label: string; options?: string[]; min?: number; max?: number }
> = {
  'server.port': { label: 'Listener port', min: 1, max: 65535 },
  'skills.max_injected': { label: 'Maximum injected skills', min: 0, max: 32 },
  'modes.a2a': { label: 'A2A discovery', options: ['true', 'false'] },
  'memory.enabled': {
    label: 'Memory and retrieval',
    options: ['true', 'false'],
  },
  'hooks.destination': { label: 'Approved webhook destination', options: [] },
  'backend.wire_api': {
    label: 'Backend wire format',
    options: ['responses', 'chat'],
  },
}
function descriptor(value: unknown): Field[] {
  if (Array.isArray(value)) return value.flatMap(descriptor)
  if (typeof value !== 'object' || !value) return []
  if ('fields' in value && Array.isArray(value.fields))
    return value.fields.flatMap(descriptor)
  if (
    'field' in value &&
    typeof value.field === 'string' &&
    Object.hasOwn(fields, value.field)
  )
    return [value as Field]
  return []
}
export function ConfigEditor({
  session,
  action,
  onApplied,
}: {
  session: Session
  action: unknown
  onApplied: () => void
}) {
  const choices = descriptor(action)
  const [field, setField] = useState(choices[0]?.field ?? ''),
    [raw, setRaw] = useState('')
  const [preview, setPreview] = useState<Preview | null>(null),
    [undo, setUndo] = useState(''),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState('')
  const info = fields[field]
  const options =
    choices.find((c) => c.field === field)?.choices ?? info?.options
  if (!info || !choices.length) return null
  const value =
    info.min !== undefined
      ? Number(raw)
      : ['true', 'false'].includes(raw)
        ? raw === 'true'
        : raw
  const valid =
    !!raw &&
    (typeof value !== 'number' ||
      (Number.isInteger(value) && value >= info.min! && value <= info.max!))
  async function run(step: 'preview' | 'apply' | 'undo') {
    setBusy(true)
    setMessage('')
    try {
      const r = await api<Preview>(
        '/admin/config/' + step,
        session,
        'POST',
        step === 'preview'
          ? { field, value }
          : { id: step === 'undo' ? undo : preview?.id },
      )
      if (step === 'preview') {
        setPreview(r)
        setMessage(r.reason ?? '')
      } else {
        setUndo(step === 'apply' ? (r.id ?? preview?.id ?? '') : '')
        setPreview(null)
        setMessage(
          step === 'apply'
            ? 'Saved. ' +
                (r.activation ?? 'Restart the service to activate the change.')
            : 'Previous configuration restored.',
        )
        onApplied()
      }
    } catch (err) {
      if (err instanceof ApiError && [409, 410].includes(err.status)) {
        setPreview(null)
        setMessage('The configuration changed. Preview the current file again.')
      } else setMessage(errorText(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="config-editor">
      <h3>Change configuration</h3>
      <div className="fields">
        {choices.length > 1 && (
          <label>
            Setting
            <select
              value={field}
              onChange={(e) => {
                setField(e.target.value)
                setRaw('')
                setPreview(null)
              }}
            >
              {choices.map((d) => (
                <option key={d.field} value={d.field}>
                  {fields[d.field].label}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          {info.label}
          {options ? (
            <select
              value={raw}
              onChange={(e) => {
                setRaw(e.target.value)
                setPreview(null)
              }}
            >
              <option value="">Select value</option>
              {options.map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          ) : (
            <input
              type="number"
              min={info.min}
              max={info.max}
              value={raw}
              onChange={(e) => {
                setRaw(e.target.value)
                setPreview(null)
              }}
            />
          )}
        </label>
      </div>
      <div className="actions">
        <Button disabled={busy || !valid} onClick={() => run('preview')}>
          Preview change
        </Button>
        {undo && (
          <Button disabled={busy} onClick={() => run('undo')}>
            Undo change
          </Button>
        )}
      </div>
      {preview && (
        <div className="preview">
          <h3>Proposed change</h3>
          {preview.target && <p>Target · {preview.target}</p>}
          <pre>{asText(preview.changes)}</pre>
          <p>{preview.activation}</p>
          {preview.id && preview.status === 'ready' && (
            <Button tone="primary" disabled={busy} onClick={() => run('apply')}>
              Apply change
            </Button>
          )}
        </div>
      )}
      <p role="status" className="note">
        {busy ? 'Contacting the service…' : message}
      </p>
    </div>
  )
}
