import { useEffect, useState } from 'react'
import { appVersion, isTauri } from '../bridge/shell'
import { NavIcon } from './NavIcon'
import { NAV_ITEMS, useNavStore } from '../state/nav'

export function NavRail() {
  const { activeView, setView } = useNavStore()
  const [version, setVersion] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    appVersion()
      .then(setVersion)
      .catch((err: unknown) => setVersion(String(err)))
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
        {!isTauri() && (
          <span className="nav-version muted">browser preview</span>
        )}
        {isTauri() && version !== null && (
          <span className="nav-version muted">v{version}</span>
        )}
      </div>
    </nav>
  )
}
