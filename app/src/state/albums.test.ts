import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AlbumList } from '../bridge/projects'
import type { TrackRow } from './library'
import { albumRows, moveTrackId, useAlbumsStore } from './albums'

const mockListAlbums = vi.fn()
const mockCreateAlbum = vi.fn()
const mockRenameAlbum = vi.fn()
const mockSetAlbumCover = vi.fn()
const mockAddAlbumTrack = vi.fn()
const mockRemoveAlbumTrack = vi.fn()
const mockReorderAlbum = vi.fn()
const mockDeleteAlbum = vi.fn()

vi.mock('../bridge/albums', () => ({
  listAlbums: () => mockListAlbums(),
  createAlbum: (name: string) => mockCreateAlbum(name),
  renameAlbum: (from: string, to: string) => mockRenameAlbum(from, to),
  setAlbumCover: (albumName: string, cover: string | null) => mockSetAlbumCover(albumName, cover),
  deleteAlbum: (name: string) => mockDeleteAlbum(name),
  addAlbumTrack: (albumName: string, trackId: string) => mockAddAlbumTrack(albumName, trackId),
  removeAlbumTrack: (albumName: string, trackId: string) => mockRemoveAlbumTrack(albumName, trackId),
  reorderAlbum: (albumName: string, trackIds: string[]) => mockReorderAlbum(albumName, trackIds),
}))

function album(name: string, tracks: string[] = [], cover: string | null = null): AlbumList {
  return { name, tracks, cover }
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
    cover: null,
  }
}

function reset() {
  useAlbumsStore.setState({
    albums: [],
    open: null,
    confirmingDelete: null,
    loading: false,
    error: null,
  })
}

describe('albumRows', () => {
  it('joins each track id to the loaded row name, in album order', () => {
    // Album A carries a real cover and B carries none: with both at `null` the
    // row's `cover` would read identically whether it was carried through or
    // hardcoded, and the assertion would prove nothing about the join.
    const rows = albumRows(
      [album('A', ['tr-0001', 'tr-0002'], 'ar-0001'), album('B')],
      [track('tr-0001', 'First'), track('tr-0002', 'Second')],
    )
    expect(rows).toEqual([
      {
        name: 'A',
        cover: 'ar-0001',
        entries: [
          { trackId: 'tr-0001', name: 'First' },
          { trackId: 'tr-0002', name: 'Second' },
        ],
      },
      { name: 'B', cover: null, entries: [] },
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
    mockSetAlbumCover.mockReset()
    mockAddAlbumTrack.mockReset()
    mockRemoveAlbumTrack.mockReset()
    mockReorderAlbum.mockReset()
    mockDeleteAlbum.mockReset()
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

  it('askDelete and cancelDelete toggle the pending confirm', () => {
    useAlbumsStore.getState().askDelete('A')
    expect(useAlbumsStore.getState().confirmingDelete).toBe('A')
    useAlbumsStore.getState().cancelDelete()
    expect(useAlbumsStore.getState().confirmingDelete).toBe(null)
  })

  it('deleteAlbum adopts the refreshed list and clears the confirm', async () => {
    useAlbumsStore.setState({ albums: [album('A'), album('B')], confirmingDelete: 'B' })
    mockDeleteAlbum.mockResolvedValue([album('A')])
    const ok = await useAlbumsStore.getState().deleteAlbum('B')
    expect(ok).toBe(true)
    expect(mockDeleteAlbum).toHaveBeenCalledWith('B')
    expect(useAlbumsStore.getState().albums).toEqual([album('A')])
    expect(useAlbumsStore.getState().confirmingDelete).toBe(null)
  })

  it('deleteAlbum closes the album when it was the open one', async () => {
    useAlbumsStore.setState({ albums: [album('A')], open: 'A' })
    mockDeleteAlbum.mockResolvedValue([])
    await useAlbumsStore.getState().deleteAlbum('A')
    expect(useAlbumsStore.getState().open).toBe(null)
  })

  it('deleteAlbum leaves a different open album untouched', async () => {
    useAlbumsStore.setState({ albums: [album('A'), album('B')], open: 'B' })
    mockDeleteAlbum.mockResolvedValue([album('B')])
    await useAlbumsStore.getState().deleteAlbum('A')
    expect(useAlbumsStore.getState().open).toBe('B')
  })

  it('deleteAlbum surfaces an error and clears the confirm', async () => {
    useAlbumsStore.setState({ albums: [album('A')], confirmingDelete: 'A' })
    mockDeleteAlbum.mockRejectedValue(new Error('disk failed'))
    const ok = await useAlbumsStore.getState().deleteAlbum('A')
    expect(ok).toBe(false)
    expect(useAlbumsStore.getState().error).toBe('disk failed')
    expect(useAlbumsStore.getState().confirmingDelete).toBe(null)
  })

  it('setCover uses the returned list and does not re-list', async () => {
    useAlbumsStore.setState({ albums: [album('A')] })
    mockSetAlbumCover.mockResolvedValue([album('A', [], 'ar-1')])
    const ok = await useAlbumsStore.getState().setCover('A', 'ar-1')
    expect(ok).toBe(true)
    expect(mockSetAlbumCover).toHaveBeenCalledWith('A', 'ar-1')
    expect(mockListAlbums).not.toHaveBeenCalled()
    expect(useAlbumsStore.getState().albums).toEqual([album('A', [], 'ar-1')])
  })

  it('setCover clears a cover with null', async () => {
    useAlbumsStore.setState({ albums: [album('A', [], 'ar-1')] })
    mockSetAlbumCover.mockResolvedValue([album('A')])
    const ok = await useAlbumsStore.getState().setCover('A', null)
    expect(ok).toBe(true)
    expect(mockSetAlbumCover).toHaveBeenCalledWith('A', null)
    expect(useAlbumsStore.getState().albums).toEqual([album('A')])
  })

  it('setCover surfaces an error', async () => {
    useAlbumsStore.setState({ albums: [album('A')] })
    mockSetAlbumCover.mockRejectedValue(new Error('disk failed'))
    const ok = await useAlbumsStore.getState().setCover('A', 'ar-1')
    expect(ok).toBe(false)
    expect(useAlbumsStore.getState().error).toBe('disk failed')
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
