import { create } from 'zustand'

export type ViewId = 'setup' | 'lyrics' | 'audio' | 'library' | 'art'

export interface NavItem {
  readonly id: ViewId
  readonly label: string
}

/** Rail order is product order: configure, write, generate, keep, decorate. */
export const NAV_ITEMS: readonly NavItem[] = [
  { id: 'setup', label: 'Setup' },
  { id: 'lyrics', label: 'Lyrics' },
  { id: 'audio', label: 'Audio' },
  { id: 'library', label: 'Library' },
  { id: 'art', label: 'Cover Art' },
]

interface NavState {
  activeView: ViewId
  setView: (id: ViewId) => void
}

export const useNavStore = create<NavState>((set) => ({
  activeView: 'setup',
  setView: (id) => set({ activeView: id }),
}))
