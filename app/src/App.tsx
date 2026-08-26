import { useEffect } from 'react'
import type { ReactElement } from 'react'
import { useNavStore } from './state/nav'
import { useConfigStore } from './state/config'
import { NavRail } from './components/NavRail'
import { Setup } from './views/Setup'
import { LyricsStudio } from './views/LyricsStudio'
import { AudioStudio } from './views/AudioStudio'
import { Library } from './views/Library'
import { CoverArt } from './views/CoverArt'

/**
 * Composition root: the rail plus whichever view is active.
 *
 * The switch is deliberately exhaustive with no `default` branch, so adding a
 * sixth `ViewId` without wiring it up fails the build instead of silently
 * rendering nothing.
 */
export function App() {
  const activeView = useNavStore((state) => state.activeView)
  const loadConfig = useConfigStore((state) => state.load)

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  let view: ReactElement
  switch (activeView) {
    case 'setup':
      view = <Setup />
      break
    case 'lyrics':
      view = <LyricsStudio />
      break
    case 'audio':
      view = <AudioStudio />
      break
    case 'library':
      view = <Library />
      break
    case 'art':
      view = <CoverArt />
      break
  }

  return (
    <div className="app-shell">
      <NavRail />
      <main className="content-pane">{view}</main>
    </div>
  )
}
