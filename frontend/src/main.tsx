import { createRoot } from 'react-dom/client'
import { useCallback, useEffect, useState, type FormEvent } from 'react'
import '@fontsource-variable/inter/wght.css'
import '@fontsource-variable/space-grotesk/wght.css'
import '@fontsource-variable/roboto/wght.css'
import '@fontsource-variable/jetbrains-mono/wght.css'
import './styles.css'
import { api, ApiError, errorText, type Connection, type Session } from './api'
import { Button } from './components/ui/button'
import { KnowMeLogo } from './components/KnowMeLogo'
import { Chat } from './Chat'
import { Settings } from './Settings'

function Pair({ onReady }: { onReady: (session: Session) => void }) {
  const [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  async function pair(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const input = event.currentTarget.elements.namedItem(
      'token',
    ) as HTMLInputElement
    const token = input.value.trim()
    if (!token || busy) return
    setBusy(true)
    setError('')
    input.value = ''
    try {
      onReady(await api<Session>('/session', undefined, 'POST', { token }))
    } catch (err) {
      setError(errorText(err))
      input.focus()
    } finally {
      setBusy(false)
    }
  }
  return (
    <main className="entry">
      <form onSubmit={pair}>
        <h1>Connect to the console</h1>
        <p>
          Enter the pairing code from the file named in the service log or the
          access token issued by your administrator.
        </p>
        <label htmlFor="token">Console access code</label>
        <input
          id="token"
          name="token"
          type="password"
          autoComplete="off"
          autoFocus
          required
          maxLength={8192}
        />
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <Button tone="primary" disabled={busy}>
          {busy ? 'Connecting…' : 'Connect'}
        </Button>
      </form>
    </main>
  )
}
function App() {
  const [session, setSession] = useState<Session | null | undefined>()
  const [connection, setConnection] = useState<Connection | null>(null),
    [error, setError] = useState('')
  const [settings, setSettings] = useState(
    location.pathname.replace(/\/$/, '') === '/ui/settings',
  )
  const reset = useCallback(() => {
    setSession(null)
    setConnection(null)
    setError('')
  }, [])
  useEffect(() => {
    const controller = new AbortController()
    api<Session>('/session', undefined, 'GET', undefined, controller.signal)
      .then(setSession)
      .catch((err) => {
        if (controller.signal.aborted) return
        if (err instanceof ApiError && [401, 403].includes(err.status)) reset()
        else setError(errorText(err))
      })
    const pop = () =>
      setSettings(location.pathname.replace(/\/$/, '') === '/ui/settings')
    window.addEventListener('popstate', pop)
    window.addEventListener('console-session-expired', reset)
    return () => {
      controller.abort()
      window.removeEventListener('popstate', pop)
      window.removeEventListener('console-session-expired', reset)
    }
  }, [reset])
  const reload = useCallback(
    async (signal?: AbortSignal) => {
      if (!session) return
      const result = await api<Connection>(
        '/me/connection',
        session,
        'GET',
        undefined,
        signal,
      )
      if (!signal?.aborted) {
        setConnection(result)
        setError('')
      }
    },
    [session],
  )
  useEffect(() => {
    const c = new AbortController()
    reload(c.signal).catch((err) => {
      if (!c.signal.aborted) setError(errorText(err))
    })
    return () => c.abort()
  }, [reload])
  function navigate(next: boolean) {
    if (settings === next) return
    history.pushState(null, '', next ? '/ui/settings' : '/ui/')
    setSettings(next)
  }
  return (
    <>
      <a className="skip" href="#workspace">
        Skip to workspace
      </a>
      <header className="mast">
        <span className="wordmark">
          <KnowMeLogo />
          <span className="brand-name">
            Know<span className="brand-accent">Me</span>
          </span>{' '}
          <span className="brand-context">Proxy</span>
        </span>
        {session && (
          <>
            <nav aria-label="Workspace">
              <a
                href="/ui/"
                aria-current={!settings ? 'page' : undefined}
                onClick={(e) => {
                  e.preventDefault()
                  navigate(false)
                }}
              >
                Chat
              </a>
              <a
                href="/ui/settings"
                aria-current={settings ? 'page' : undefined}
                onClick={(e) => {
                  e.preventDefault()
                  navigate(true)
                }}
              >
                Settings
              </a>
            </nav>
            <Button
              tone="quiet"
              className="signout"
              onClick={async () => {
                try {
                  await api('/session', session, 'DELETE')
                  reset()
                } catch (err) {
                  setError(errorText(err))
                }
              }}
            >
              Sign out
            </Button>
          </>
        )}
      </header>
      <div id="workspace" tabIndex={-1}>
        {error && (
          <div role="alert" className="banner error">
            {error}{' '}
            <Button
              onClick={() => {
                if (session) reload().catch((e) => setError(errorText(e)))
                else location.reload()
              }}
            >
              Try again
            </Button>
          </div>
        )}
        {session === null ? (
          <Pair onReady={setSession} />
        ) : session && connection ? (
          <>
            <div hidden={settings}>
              <Chat
                key={session.csrfToken}
                session={session}
                connection={connection}
              />
            </div>
            {settings && (
              <Settings
                session={session}
                connection={connection}
                reload={reload}
              />
            )}
          </>
        ) : (
          !error && (
            <p role="status" className="banner">
              Connecting to the service…
            </p>
          )
        )}
      </div>
    </>
  )
}
createRoot(document.getElementById('root')!).render(<App />)
