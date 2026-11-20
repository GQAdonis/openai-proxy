import { HttpAgent } from '@ag-ui/client'
import { EventSchema } from '@ag-ui/core/schemas'
import type { Session } from './api'
import type { SurfaceAction } from './Surface'

export type ProjectionHandlers = {
  surface: (message: string, operations: unknown) => void
  start?: (id: string) => void
  text?: (delta: string) => void
  finish?: (ok: boolean, message?: string) => void
}
export async function project(
  session: Session,
  threadId: string,
  runId: string,
  signal: AbortSignal,
  handlers: ProjectionHandlers,
  action?: { userAction: SurfaceAction; idempotencyKey: string },
) {
  const agent = new HttpAgent({
    url: '/api/v1/ag-ui/stream',
    threadId,
    headers: { 'X-CSRF-Token': session.csrfToken },
  })
  let terminal = false,
    started = false
  function abort() {
    agent.abortRun()
  }
  signal.throwIfAborted()
  signal.addEventListener('abort', abort, { once: true })
  try {
    await agent.runAgent(
      { runId, forwardedProps: action ? { a2uiAction: action } : {} },
      {
        onEvent({ event: raw }) {
          const event = EventSchema.parse(raw)
          if (event.type === 'RUN_STARTED') {
            if (
              started ||
              event.threadId !== threadId ||
              (!action && event.runId !== runId)
            )
              throw new Error('Unexpected conversation projection.')
            started = true
            handlers.start?.(event.runId)
          } else if (event.type === 'RUN_ERROR') {
            terminal = true
            handlers.finish?.(false, event.message)
          } else if (!started || terminal)
            throw new Error('Unexpected event order.')
          else if (
            event.type === 'ACTIVITY_SNAPSHOT' &&
            event.activityType === 'a2ui-surface' &&
            event.replace === true
          )
            handlers.surface(event.messageId, event.content.a2ui_operations)
          else if (event.type === 'TEXT_MESSAGE_CONTENT')
            handlers.text?.(event.delta)
          else if (event.type === 'RUN_FINISHED') {
            terminal = true
            handlers.finish?.(true)
          }
        },
      },
    )
    if (!terminal && !signal.aborted)
      throw new Error('The activity stream was interrupted.')
  } finally {
    signal.removeEventListener('abort', abort)
  }
}
