import { useEffect, useState } from 'react'
import { appVersion, isTauri } from './bridge/shell'

/**
 * Shell placeholder. T-002 replaces the body with the real nav rail and the
 * five views; this exists so the scaffold is verifiably wired end to end.
 */
export function App() {
  const [version, setVersion] = useState<string | null>(null)
  const [bridgeError, setBridgeError] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    appVersion()
      .then(setVersion)
      .catch((err: unknown) => setBridgeError(String(err)))
  }, [])

  return (
    <div className="app-shell">
      <nav className="nav-rail">
        <div className="nav-brand">
          latent<span className="nav-brand-accent">Create</span>
        </div>
      </nav>
      <main className="content-pane">
        <h1 className="view-title">Scaffold</h1>
        <p className="view-subtitle">
          Phase 0. The nav rail and views arrive in T-002.
        </p>
        <div className="panel">
          {!isTauri() && (
            <span className="status-pill status-pill-warn">
              Browser preview - Tauri bridge unavailable
            </span>
          )}
          {bridgeError && (
            <span className="status-pill status-pill-warn">
              Bridge error: {bridgeError}
            </span>
          )}
          {version && (
            <span className="status-pill status-pill-ok">
              Rust shell v{version}
            </span>
          )}
        </div>
      </main>
    </div>
  )
}
