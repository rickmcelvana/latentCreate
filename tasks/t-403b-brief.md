# T-403b: album lists -- the frontend store (`bridge/albums.ts` + `state/albums.ts`)

**Depends:** T-403a (the `albums_*` commands) | **Crate/dir:** app/src
**Files to create/modify:**
- `app/src/bridge/albums.ts` (new)
- `app/src/state/albums.ts` (new)
- `app/src/state/albums.test.ts` (new)

## Goal

The typed bridge over the T-403a commands and a zustand store that owns the album view's data:
the album list, the opened album, and every mutation -- create, rename, add track, remove track,
reorder (via move-up/move-down that computes the full new order). All decisions live in pure
functions so the view (T-403c) renders, never decides.

## Design decisions (from T-403a, restated for the frontend)

- Albums are name-addressed; the store keys albums by `name`, and the opened album is tracked by
  name too. A rename of the open album follows it.
- **The missing-track rule is frontend-rendered, not backend-hidden.** `library` keeps a deleted
  track's id in the album (deletion is the only legitimate source of a dangling id, T-403a). So
  `albumRows` joins each album's ids against the loaded `TrackRow`s and renders an entry whose
  track is gone with `name: null` -- the view shows "Missing track" rather than dropping the row
  (the T-403 trap).
- Every mutation action returns `Promise<boolean>`; the backend returns the refreshed full album
  list, which the store adopts verbatim. On a failed write the store keeps the last good list and
  surfaces the error.

## Spec

### `app/src/bridge/albums.ts` (new)

```ts
import { invoke } from '@tauri-apps/api/core'
import type { AlbumList } from './projects'

/** List every album in the selected project. */
export async function listAlbums(): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('albums_list')
}

/** Create an album. Returns the project's refreshed album list. */
export async function createAlbum(name: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_create', { name })
}

/** Rename an album. Returns the refreshed album list. */
export async function renameAlbum(from: string, to: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_rename', { from, to })
}

/** Add a track to an album. Returns the refreshed album list. */
export async function addAlbumTrack(album: string, trackId: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_add_track', { album, trackId })
}

/** Remove a track from an album. Returns the refreshed album list. */
export async function removeAlbumTrack(album: string, trackId: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_remove_track', { album, trackId })
}

/** Set an album's full track order. Returns the refreshed album list. */
export async function reorderAlbum(album: string, trackIds: string[]): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_reorder', { album, trackIds })
}
```

`AlbumList` is already mirrored in `app/src/bridge/projects.ts` (`{ name, tracks: string[] }`) --
import it, do not redeclare.

### `app/src/state/albums.ts` (new)

```ts
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
```

### `app/src/state/albums.test.ts` (new)

Follow the house pattern: mock the bridge module, test pure functions first, then the store
actions. Reference:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AlbumList } from '../bridge/projects'
import type { TrackRow } from './library'
import { albumRows, moveTrackId, useAlbumsStore } from './albums'

const mockListAlbums = vi.fn()
const mockCreateAlbum = vi.fn()
const mockRenameAlbum = vi.fn()
const mockAddAlbumTrack = vi.fn()
const mockRemoveAlbumTrack = vi.fn()
const mockReorderAlbum = vi.fn()

vi.mock('../bridge/albums', () => ({
  listAlbums: () => mockListAlbums(),
  createAlbum: (name: string) => mockCreateAlbum(name),
  renameAlbum: (from: string, to: string) => mockRenameAlbum(from, to),
  addAlbumTrack: (albumName: string, trackId: string) => mockAddAlbumTrack(albumName, trackId),
  removeAlbumTrack: (albumName: string, trackId: string) => mockRemoveAlbumTrack(albumName, trackId),
  reorderAlbum: (albumName: string, trackIds: string[]) => mockReorderAlbum(albumName, trackIds),
}))

function album(name: string, tracks: string[] = []): AlbumList {
  return { name, tracks }
}

function track(id: string, name: string): TrackRow {
  return {
    id,
    name,
    model: 'ACE-Step 1.5 XL Turbo',
    license: 'Apache-2.0',
    duration: '2:00',
    created: '2026-08-30',
    loras: '',
    seed: '42',
    promptId: null,
    file: `tracks/${id}.flac`,
  }
}

function reset() {
  useAlbumsStore.setState({ albums: [], open: null, loading: false, error: null })
}

describe('albumRows', () => {
  it('joins each track id to the loaded row name, in album order', () => {
    const rows = albumRows(
      [album('A', ['tr-0001', 'tr-0002']), album('B')],
      [track('tr-0001', 'First'), track('tr-0002', 'Second')],
    )
    expect(rows).toEqual([
      {
        name: 'A',
        entries: [
          { trackId: 'tr-0001', name: 'First' },
          { trackId: 'tr-0002', name: 'Second' },
        ],
      },
      { name: 'B', entries: [] },
    ])
  })

  /**
   * The T-403 trap. Mutation check: dropping the `?? null` fallback would make
   * a deleted track's entry vanish, and this test would fail.
   */
  it('keeps a track with no row as a missing entry, not a dropped one', () => {
    const rows = albumRows([album('A', ['tr-0001', 'tr-0009'])], [track('tr-0001', 'First')])
    expect(rows[0]!.entries).toEqual([
      { trackId: 'tr-0001', name: 'First' },
      { trackId: 'tr-0009', name: null },
    ])
  })
})

describe('moveTrackId', () => {
  it('moves a track up', () => {
    expect(moveTrackId(['a', 'b', 'c'], 'b', 'up')).toEqual(['b', 'a', 'c'])
  })

  it('moves a track down', () => {
    expect(moveTrackId(['a', 'b', 'c'], 'b', 'down')).toEqual(['a', 'c', 'b'])
  })

  it('leaves the order unchanged at either end', () => {
    expect(moveTrackId(['a', 'b'], 'a', 'up')).toEqual(['a', 'b'])
    expect(moveTrackId(['a', 'b'], 'b', 'down')).toEqual(['a', 'b'])
  })

  it('leaves the order unchanged for an unknown id', () => {
    expect(moveTrackId(['a', 'b'], 'zzz', 'up')).toEqual(['a', 'b'])
  })
})

describe('albums store', () => {
  beforeEach(() => {
    mockListAlbums.mockReset()
    mockCreateAlbum.mockReset()
    mockRenameAlbum.mockReset()
    mockAddAlbumTrack.mockReset()
    mockRemoveAlbumTrack.mockReset()
    mockReorderAlbum.mockReset()
    reset()
  })

  it('load adopts the album list', async () => {
    mockListAlbums.mockResolvedValue([album('A'), album('B')])
    await useAlbumsStore.getState().load()
    expect(useAlbumsStore.getState().albums).toEqual([album('A'), album('B')])
    expect(useAlbumsStore.getState().loading).toBe(false)
  })

  it('load surfaces an error without wiping the last good list', async () => {
    useAlbumsStore.setState({ albums: [album('A')] })
    mockListAlbums.mockRejectedValue(new Error('disk failed'))
    await useAlbumsStore.getState().load()
    expect(useAlbumsStore.getState().error).toBe('disk failed')
    expect(useAlbumsStore.getState().albums).toEqual([album('A')])
  })

  it('create refuses a blank name before calling the bridge', async () => {
    const ok = await useAlbumsStore.getState().create('   ')
    expect(ok).toBe(false)
    expect(mockCreateAlbum).not.toHaveBeenCalled()
    expect(useAlbumsStore.getState().error).toBe('Name the album before creating it.')
  })

  it('create trims the name and adopts the refreshed list', async () => {
    mockCreateAlbum.mockResolvedValue([album('Night Drive')])
    const ok = await useAlbumsStore.getState().create('  Night Drive  ')
    expect(ok).toBe(true)
    expect(mockCreateAlbum).toHaveBeenCalledWith('Night Drive')
    expect(useAlbumsStore.getState().albums).toEqual([album('Night Drive')])
  })

  it('rename follows the open album to its new name', async () => {
    useAlbumsStore.setState({ albums: [album('Old')], open: 'Old' })
    mockRenameAlbum.mockResolvedValue([album('New')])
    const ok = await useAlbumsStore.getState().rename('Old', 'New')
    expect(ok).toBe(true)
    expect(mockRenameAlbum).toHaveBeenCalledWith('Old', 'New')
    expect(useAlbumsStore.getState().open).toBe('New')
  })

  it('rename keeps a different album open', async () => {
    useAlbumsStore.setState({ albums: [album('A'), album('B')], open: 'B' })
    mockRenameAlbum.mockResolvedValue([album('A2'), album('B')])
    await useAlbumsStore.getState().rename('A', 'A2')
    expect(useAlbumsStore.getState().open).toBe('B')
  })

  it('addTrack adopts the refreshed list', async () => {
    mockAddAlbumTrack.mockResolvedValue([album('A', ['tr-0001'])])
    const ok = await useAlbumsStore.getState().addTrack('A', 'tr-0001')
    expect(ok).toBe(true)
    expect(mockAddAlbumTrack).toHaveBeenCalledWith('A', 'tr-0001')
    expect(useAlbumsStore.getState().albums).toEqual([album('A', ['tr-0001'])])
  })

  it('removeTrack adopts the refreshed list', async () => {
    useAlbumsStore.setState({ albums: [album('A', ['tr-0001'])] })
    mockRemoveAlbumTrack.mockResolvedValue([album('A')])
    const ok = await useAlbumsStore.getState().removeTrack('A', 'tr-0001')
    expect(ok).toBe(true)
    expect(mockRemoveAlbumTrack).toHaveBeenCalledWith('A', 'tr-0001')
    expect(useAlbumsStore.getState().albums).toEqual([album('A')])
  })

  it('move sends the full new order to the bridge', async () => {
    useAlbumsStore.setState({ albums: [album('A', ['tr-0001', 'tr-0002'])] })
    mockReorderAlbum.mockResolvedValue([album('A', ['tr-0002', 'tr-0001'])])
    const ok = await useAlbumsStore.getState().move('A', 'tr-0001', 'down')
    expect(ok).toBe(true)
    expect(mockReorderAlbum).toHaveBeenCalledWith('A', ['tr-0002', 'tr-0001'])
  })

  it('move at an end is a no-op that never calls the bridge', async () => {
    useAlbumsStore.setState({ albums: [album('A', ['tr-0001'])] })
    const ok = await useAlbumsStore.getState().move('A', 'tr-0001', 'up')
    expect(ok).toBe(true)
    expect(mockReorderAlbum).not.toHaveBeenCalled()
  })

  it('move on an unknown album errors without calling the bridge', async () => {
    const ok = await useAlbumsStore.getState().move('nope', 'tr-0001', 'up')
    expect(ok).toBe(false)
    expect(mockReorderAlbum).not.toHaveBeenCalled()
    expect(useAlbumsStore.getState().error).toContain('nope')
  })

  it('openAlbum sets and clears the open album', () => {
    useAlbumsStore.getState().openAlbum('A')
    expect(useAlbumsStore.getState().open).toBe('A')
    useAlbumsStore.getState().openAlbum(null)
    expect(useAlbumsStore.getState().open).toBeNull()
  })
})
```

## Acceptance criteria

- [ ] `tsc -b`, `oxlint src`, `vitest run` and `vite build` green.
- [ ] frontend goes **355 -> 373** tests (18 new: 2 albumRows, 4 moveTrackId, 12 store).
- [ ] `invoke` appears only in `bridge/albums.ts` (grep `@tauri-apps` across `app/src`); the store
      and tests never touch the bridge directly except through the mock.
- [ ] No changes outside the three listed files; `bridge/projects.ts` is not modified (its
      `AlbumList` type is imported).
- [ ] The missing-track entry test is the flagship guard: a mutation that drops the `?? null`
      fallback fails it.

## Out of scope

- The album view itself (T-403c), CSS, and the Library wiring.
- Any schema or backend change (T-403a).

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read app/src/bridge/projects.ts --read app/src/state/library.ts --read app/src/state/projects.test.ts --file app/src/bridge/albums.ts --file app/src/state/albums.ts --file app/src/state/albums.test.ts
```
