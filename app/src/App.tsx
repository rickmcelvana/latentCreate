import { useNavStore } from './state/nav'
import { NavRail } from './components/NavRail'
import { Setup } from './views/Setup'
import { LyricsStudio } from './views/LyricsStudio'
import { AudioStudio } from './views/AudioStudio'
import { Library } from './views/Library'
import { CoverArt } from './views/CoverArt'

export function App() {
  const { activeView } = useNavStore()

  let view: JSX.Element
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
