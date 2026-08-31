import { create } from 'zustand'
import {
  addAlbumTrack,
  createAlbum,
  listAlbums,
  removeAlbumTrack,
  renameAlbum,
  reorderAlbum,
} from '../bridge/albums'
import type { AlbumList } from '../bridge/projects'
import type { TrackRow } from './library'

/** One track entry inside an opened album, joined against the library. */
export interface AlbumEntry {
  trackId: string
  /**
   * The track's display name, or `null` when the id is not in the loaded
   * library -- a track that was deleted after being added. `null` renders as
   * "Missing track"; the entry is kept, never dropped (the T-403 trap).
   */
  name: string | null
}

/** One album ready to render: its name and its entries in order. */
export interface AlbumRow {
  name: string
  entries: AlbumEntry[]
}

/**
 * Join every album's track ids against the loaded library rows.
 *
 * The join is deliberately here, not in the view: an id with no row must
 * still render (as missing), and that decision belongs in a tested function
 * (WORKFLOW section 4). The library tracks come from `useLibraryStore`,
 * which the view already loads for the selected project.
 */
export function albumRows(albums: AlbumList[], tracks: TrackRow[]): AlbumRow[] {
  return albums.map((album) => ({
    name: album.name,
    entries: album.tracks.map((trackId) => ({
      trackId,
      name: tracks.find((track) => track.id === trackId)?.name ?? null,
    })),
  }))
}

/**
 * Move one track id up or down within an album's order. Pure so the move
 * buttons are testable: returns a new order array, or the same array when the
 * id is unknown or the move would fall off either end.
 */
export function moveTrackId(
  trackIds: string[],
  trackId: string,
  direction: 'up' | 'down',
): string[] {
  const index = trackIds.indexOf(trackId)
  if (index === -1) return trackIds
  const target = direction === 'up' ? index - 1 : index + 1
  if (target < 0 || target >= trackIds.length) return trackIds
  const next = [...trackIds]
  next[index] = next[target]!
  next[target] = trackId
  return next
}

interface AlbumsState {
  /** The selected project's albums, as the backend reported them. */
  albums: AlbumList[]
  /** The album currently opened in the view, by name; `null` when none is. */
  open: string | null
  loading: boolean
  error: string | null
  load: () => Promise<void>
  /** Create an album. Resolves `true` on success. */
  create: (name: string) => Promise<boolean>
  /** Rename an album. Resolves `true` on success. */
  rename: (from: string, to: string) => Promise<boolean>
  /** Add a track to an album. Resolves `true` on success. */
  addTrack: (album: string, trackId: string) => Promise<boolean>
  /** Remove a track from an album. Resolves `true` on success. */
  removeTrack: (album: string, trackId: string) => Promise<boolean>
  /** Move one track up or down and persist the new order. */
  move: (album: string, trackId: string, direction: 'up' | 'down') => Promise<boolean>
  /** Open or close an album in the view. */
  openAlbum: (name: string | null) => void
}

export const useAlbumsStore = create<AlbumsState>((set, get) => ({
  albums: [],
  open: null,
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null })
    try {
      const albums = await listAlbums()
      set({ albums, loading: false })
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err), loading: false })
    }
  },

  create: async (name) => {
    const trimmed = name.trim()
    if (trimmed === '') {
      set({ error: 'Name the album before creating it.' })
      return false
    }
    try {
      const albums = await createAlbum(trimmed)
      set({ albums, error: null })
      return true
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err) })
      return false
    }
  },

  rename: async (from, to) => {
    const trimmed = to.trim()
    if (trimmed === '') {
      set({ error: 'Name the album before renaming it.' })
      return false
    }
    try {
      const albums = await renameAlbum(from, trimmed)
      // The open album is tracked by name, so a rename of it must follow it.
      const open = get().open === from ? trimmed : get().open
      set({ albums, open, error: null })
      return true
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err) })
      return false
    }
  },

  addTrack: async (album, trackId) => {
    try {
      const albums = await addAlbumTrack(album, trackId)
      set({ albums, error: null })
      return true
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err) })
      return false
    }
  },

  removeTrack: async (album, trackId) => {
    try {
      const albums = await removeAlbumTrack(album, trackId)
      set({ albums, error: null })
      return true
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err) })
      return false
    }
  },

  move: async (album, trackId, direction) => {
    const current = get().albums.find((entry) => entry.name === album)
    if (current === undefined) {
      set({ error: `No album named "${album}" is loaded.` })
      return false
    }
    const next = moveTrackId(current.tracks, trackId, direction)
    if (next === current.tracks) return true
    try {
      const albums = await reorderAlbum(album, next)
      set({ albums, error: null })
      return true
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err) })
      return false
    }
  },

  openAlbum: (name) => set({ open: name }),
}))
