import { useEffect, useState } from 'react'
import { appVersion, isTauri } from '../bridge/shell'
import { NavIcon } from './NavIcon'
import { NAV_ITEMS, useNavStore } from '../state/nav'

/**
 * Left navigation rail: brand, one button per destination, and a footer that
 * reports the Rust shell version -- the standing proof that the Tauri boundary
 * round-trips (T-001).
 */
export function NavRail() {
  const activeView = useNavStore((state) => state.activeView)
  const setView = useNavStore((state) => state.setView)
  const [version, setVersion] = useState<string | null>(null)
  const [bridgeError, setBridgeError] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    appVersion()
      .then(setVersion)
      .catch((err: unknown) => setBridgeError(String(err)))
  }, [])

  return (
    <nav className="nav-rail">
      <div className="nav-brand">
        latent<span className="nav-brand-accent">Create</span>
      </div>

      {NAV_ITEMS.map((item) => {
        const isActive = activeView === item.id
        return (
          <button
            key={item.id}
            type="button"
            className={`nav-item${isActive ? ' nav-item-active' : ''}`}
            aria-current={isActive ? 'page' : undefined}
            onClick={() => setView(item.id)}
          >
            <NavIcon view={item.id} />
            {item.label}
          </button>
        )
      })}

      <div className="nav-rail-footer">
        {!isTauri() && <span className="nav-version muted">browser preview</span>}
        {bridgeError !== null && (
          <span className="nav-version nav-version-error" title={bridgeError}>
            bridge unavailable
          </span>
        )}
        {version !== null && <span className="nav-version muted">v{version}</span>}
      </div>
    </nav>
  )
}
