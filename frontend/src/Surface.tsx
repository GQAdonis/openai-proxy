import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import {
  A2uiSurface,
  createBinderlessComponentImplementation,
  type ReactCatalogComponent,
} from '@a2ui/react'
import { Catalog, MessageProcessor, type ActionPayload } from '@a2ui/web_core'
import {
  AgentToRendererMessageSchema,
  RendererToAgentMessageSchema,
} from '@a2ui/web_core/v1_0'
import { basicCatalog } from '@a2ui/web_core/catalogs/basic/v1'
import { validOperation, validAction } from 'virtual:console-wire-schemas'
import { Button } from './components/ui/button'

type Dict = Record<string, unknown>
export type SurfaceAction = Pick<
  ActionPayload,
  'name' | 'surfaceId' | 'sourceComponentId' | 'timestamp' | 'context'
>
const actions = new Set(['submit', 'choose'])
function requireValue(test: unknown): asserts test {
  if (!test) throw new Error('Unsupported surface')
}
function object(v: unknown): v is Dict {
  return !!v && typeof v === 'object' && !Array.isArray(v)
}
function only(obj: Dict, keys: string[]) {
  requireValue(Object.keys(obj).every((key) => keys.includes(key)))
}
function safeId(v: unknown): v is string {
  return typeof v === 'string' && /^[\w.-]{1,100}$/.test(v) && !danger.has(v)
}
const danger = new Set(['__proto__', 'prototype', 'constructor'])
function pointer(v: unknown) {
  requireValue(
    typeof v === 'string' &&
      (v === '' || v.startsWith('/')) &&
      v.length <= 256 &&
      !/~(?![01])/.test(v),
  )
  requireValue(
    v.split('/').every((key) => !danger.has(key) && !/^\d{5,}$/.test(key)),
  )
}
function bound(value: unknown, depth = 0) {
  requireValue(depth <= 20)
  if (Array.isArray(value)) {
    requireValue(value.length <= 256)
    value.forEach((item) => bound(item, depth + 1))
  } else if (object(value)) {
    const entries = Object.entries(value)
    requireValue(entries.length <= 128)
    for (const [key, item] of entries) {
      requireValue(!danger.has(key))
      bound(item, depth + 1)
    }
  }
}
function binding(v: unknown) {
  if (typeof v === 'string') return
  requireValue(object(v))
  only(v, ['@path'])
  pointer(v['@path'])
}
const componentKeys: Record<string, string[]> = {
  Text: ['text'],
  Row: ['children'],
  Column: ['children'],
  Card: ['child'],
  Button: ['child', 'action'],
}
function adaptComponent(v: unknown) {
  requireValue(
    object(v) &&
      safeId(v.id) &&
      typeof v.component === 'string' &&
      Object.hasOwn(componentKeys, v.component),
  )
  only(v, [
    'id',
    'component',
    '@accessibility',
    '@catalogId',
    ...componentKeys[v.component],
  ])
  const { '@accessibility': accessibility, '@catalogId': catalogId, ...out } = v
  if (catalogId !== undefined) {
    requireValue(catalogId === basicCatalog.id)
    out.catalogId = catalogId
  }
  if (accessibility !== undefined) {
    requireValue(object(accessibility))
    only(accessibility, ['label', 'description'])
    Object.values(accessibility).forEach(binding)
    out.accessibility = accessibility
  }
  if (out.text !== undefined) binding(out.text)
  if (out.child !== undefined) requireValue(safeId(out.child))
  if (out.children !== undefined)
    requireValue(Array.isArray(out.children) && out.children.every(safeId))
  if (out.component === 'Button') {
    const action = out.action
    requireValue(
      object(action) &&
        object(action.event) &&
        typeof action.event.name === 'string' &&
        actions.has(action.event.name),
    )
    only(action, ['event'])
    only(action.event, ['name', 'context'])
    if (action.event.context !== undefined) {
      requireValue(object(action.event.context))
      for (const val of Object.values(action.event.context)) {
        if (object(val)) binding(val)
        else
          requireValue(
            val === null ||
              ['number', 'boolean', 'string'].includes(typeof val),
          )
      }
    }
  }
  return out
}

// Preserve the Basic catalog's component signatures. Render plain text and the actual
// native button ourselves so accessibility bindings land on the interactive element.
const entries = Object.keys(componentKeys).map((name) => {
  const entry = basicCatalog.components.get(name)!
  if (name !== 'Button' && name !== 'Text') return entry
  return createBinderlessComponentImplementation(
    entry,
    ({ context, buildChild }) => {
      const props = context.componentModel.properties
      const resolve = (v: unknown) => {
        if (v === undefined) return undefined
        const resolved = context.dataContext.resolveDynamicValue<unknown>(v)
        return typeof resolved === 'string' ? resolved : undefined
      }
      const attrs = {
        'aria-label': resolve(props.accessibility?.label),
        'aria-description': resolve(props.accessibility?.description),
      }
      if (name === 'Text') return <span {...attrs}>{resolve(props.text)}</span>
      return (
        <Button
          type="button"
          tone={props.variant === 'primary' ? 'primary' : 'outline'}
          {...attrs}
          data-surface-control={context.componentModel.id}
          onClick={(event) => {
            if (event.nativeEvent.isTrusted)
              void context.dispatchAction(props.action)
          }}
        >
          {buildChild(props.child)}
        </Button>
      )
    },
  )
})
const catalog = new Catalog<ReactCatalogComponent>(
  basicCatalog.id,
  '1.0',
  entries,
  [],
)
function candidate(input: unknown, onAction: (act: SurfaceAction) => void) {
  requireValue(Array.isArray(input) && input.length > 0 && input.length <= 64)
  bound(input)
  requireValue(
    new TextEncoder().encode(JSON.stringify(input)).length <= 64 * 1024,
  )
  const existing = new Map<string, Map<string, Dict>>(),
    seen = new Set<string>()
  const messages = input.map((original) => {
    requireValue(
      validOperation(original) &&
        object(original) &&
        original.version === '1.0',
    )
    const op = Object.keys(original).find((k) => k !== 'version')!
    requireValue(
      [
        'createSurface',
        'updateComponents',
        'updateDataModel',
        'deleteSurface',
      ].includes(op),
    )
    const args = original[op]
    requireValue(object(args) && safeId(args.surfaceId))
    const id = args.surfaceId
    if (op === 'createSurface') {
      requireValue(
        !seen.has(id) && seen.size < 8 && args.catalogId === basicCatalog.id,
      )
      only(args, [
        'surfaceId',
        'catalogId',
        'components',
        'dataModel',
        'sendDataModel',
      ])
      requireValue(!args.sendDataModel)
      seen.add(id)
      existing.set(id, new Map())
    } else requireValue(existing.has(id))
    if (op === 'deleteSurface') existing.delete(id)
    if (op === 'updateDataModel' && args.path !== undefined) pointer(args.path)
    let adjusted = args
    if ('components' in args) {
      requireValue(Array.isArray(args.components))
      const components = args.components.map(adaptComponent)
      const current = existing.get(id)!
      for (const entry of components) {
        current.set(entry.id as string, entry)
        requireValue(current.size <= 128)
      }
      adjusted = { ...args, components }
    }
    return AgentToRendererMessageSchema.parse({
      version: '1.0',
      [op]: adjusted,
    })
  })
  for (const components of existing.values()) {
    const visited = new Set<string>()
    function visit(id: string, depth: number) {
      const entry = components.get(id)
      requireValue(entry && !visited.has(id) && depth <= 20)
      visited.add(id)
      for (const child of (entry.children ??
        (entry.child ? [entry.child] : [])) as string[])
        visit(child, depth + 1)
    }
    visit('root', 0)
    requireValue(visited.size === components.size)
  }
  const processor = new MessageProcessor<ReactCatalogComponent>(
    [catalog],
    (value) => {
      const { name, surfaceId, sourceComponentId, timestamp, context } = value
      const action = { name, surfaceId, sourceComponentId, timestamp, context }
      const wire = { version: '1.0', action }
      if (
        actions.has(name) &&
        validAction(wire) &&
        RendererToAgentMessageSchema.safeParse(wire).success &&
        JSON.stringify(context).length < 4096 &&
        existing.get(surfaceId)?.get(sourceComponentId)?.component === 'Button'
      )
        onAction(action)
    },
    { version: '1.0' },
  )
  try {
    processor.processMessages(messages)
    return processor
  } catch (error) {
    processor.dispose()
    throw error
  }
}
export function Surface({
  operations,
  onAction,
  disabled,
}: {
  operations: unknown
  disabled: boolean
  onAction: (act: SurfaceAction) => void
}) {
  const [processor, setProcessor] =
      useState<MessageProcessor<ReactCatalogComponent> | null>(null),
    [error, setError] = useState('')
  const wrapper = useRef<HTMLDivElement>(null),
    focus = useRef<string | null>(null),
    callback = useRef(onAction)
  callback.current = onAction
  useEffect(() => {
    try {
      const next = candidate(operations, (a) => callback.current(a))
      focus.current = wrapper.current?.contains(document.activeElement)
        ? (document.activeElement?.getAttribute('data-surface-control') ?? '')
        : null
      setProcessor(next)
      setError('')
    } catch {
      setError('This interactive response could not be displayed.')
    }
  }, [operations])
  useEffect(() => () => processor?.dispose(), [processor])
  useLayoutEffect(() => {
    if (focus.current === null) return
    const el = [
      ...wrapper.current!.querySelectorAll<HTMLElement>(
        '[data-surface-control]',
      ),
    ].find((e) => e.dataset.surfaceControl === focus.current)
    ;(el ?? wrapper.current)?.focus()
    focus.current = null
  }, [processor])
  return (
    <div ref={wrapper} tabIndex={-1} className="surface">
      <fieldset disabled={disabled} aria-label="Interactive response">
        {error && <p role="status">{error}</p>}
        {processor &&
          (processor.model.surfacesMap.size ? (
            [...processor.model.surfacesMap.values()].map((s) => (
              <A2uiSurface key={s.id} surface={s} />
            ))
          ) : (
            <p role="status">Interactive response closed.</p>
          ))}
      </fieldset>
    </div>
  )
}
