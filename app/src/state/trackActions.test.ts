import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useLibraryStore } from './library'
import { errorFor, filenameSafe, isRow, useTrackActionsStore } from './trackActions'

const mockDeleteTrack = vi.fn()
const mockRenameTrack = vi.fn()
const mockSetTrackCover = vi.fn()
const mockExportTrack = vi.fn()
const mockRevealTrack = vi.fn()
const mockPickExportPath = vi.fn()

vi.mock('../bridge/tracks', () => ({
  deleteTrack: (id: string) => mockDeleteTrack(id),
  renameTrack: (id: string, title: string) => mockRenameTrack(id, title),
  setTrackCover: (id: string, cover: string | null) => mockSetTrackCover(id, cover),
  exportTrack: (id: string, dest: string) => mockExportTrack(id, dest),
  revealTrack: (id: string) => mockRevealTrack(id),
  pickExportPath: (defaultName: string) => mockPickExportPath(defaultName),
}))

function reset() {
  useTrackActionsStore.setState({
    busy: null,
    error: null,
    confirming: null,
    renaming: null,
  })
}

describe('errorFor', () => {
  it('returns the message for its own track', () => {
    expect(
      errorFor({ trackId: 'tr-0001', message: 'file missing' }, 'tr-0001'),
    ).toBe('file missing')
  })

  it('returns null for another track', () => {
    expect(
      errorFor({ trackId: 'tr-0001', message: 'file missing' }, 'tr-0002'),
    ).toBeNull()
  })

  it('returns null when nothing failed', () => {
    expect(errorFor(null, 'tr-0001')).toBeNull()
  })
})

describe('filenameSafe', () => {
  it('replaces Windows-illegal characters with underscores', () => {
    // The T-409 trap-4 case from the brief.
    expect(filenameSafe('Midnight: Drive/2')).toBe('Midnight_ Drive_2')
    expect(filenameSafe('My Song: Vol. 2/2')).toBe('My Song_ Vol. 2_2')
  })

  it('leaves a clean title unchanged', () => {
    expect(filenameSafe('Midnight Drive')).toBe('Midnight Drive')
  })

  it('reduces a title of only illegal characters to empty, for the id fallback', () => {
    // The caller does `filenameSafe(name) || row.id`, so an empty result means
    // the id is used -- a filename of bare underscores is never produced.
    expect(filenameSafe('///')).toBe('')
    expect(filenameSafe(':*?')).toBe('')
  })

  it('strips a trailing dot or space, which Windows forbids', () => {
    expect(filenameSafe('Untitled.')).toBe('Untitled')
    expect(filenameSafe('spaced   ')).toBe('spaced')
  })
})

describe('isRow', () => {
  it('is true only for the marked id', () => {
    expect(isRow('tr-0001', 'tr-0001')).toBe(true)
    expect(isRow('tr-0001', 'tr-0002')).toBe(false)
    expect(isRow(null, 'tr-0001')).toBe(false)
  })
})

describe('delete flow', () => {
  beforeEach(() => {
    mockDeleteTrack.mockReset()
    reset()
  })

  it('arms and disarms the inline confirm', () => {
    useTrackActionsStore.getState().askDelete('tr-0001')
    expect(useTrackActionsStore.getState().confirming).toBe('tr-0001')
    expect(useTrackActionsStore.getState().error).toBeNull()

    useTrackActionsStore.getState().cancelDelete()
    expect(useTrackActionsStore.getState().confirming).toBeNull()
  })

  it('success clears busy and confirming and leaves no error', async () => {
    mockDeleteTrack.mockResolvedValue(undefined)
    // Arm the confirm first -- confirmDelete only runs once askDelete has, and
    // the success clears it. Without arming, the assertion would pass vacuously.
    useTrackActionsStore.getState().askDelete('tr-0001')
    const ok = await useTrackActionsStore.getState().confirmDelete('tr-0001')
    expect(ok).toBe(true)
    expect(useTrackActionsStore.getState().busy).toBeNull()
    expect(useTrackActionsStore.getState().confirming).toBeNull()
    expect(useTrackActionsStore.getState().error).toBeNull()
  })

  // Protects: the row stays in confirm state so the user can retry or cancel.
  // Clearing confirming on failure would force the user to re-open the menu.
  it('failure records the error and keeps confirming set for retry', async () => {
    mockDeleteTrack.mockRejectedValue('trash failed')
    useTrackActionsStore.getState().askDelete('tr-0001')
    const ok = await useTrackActionsStore.getState().confirmDelete('tr-0001')
    expect(ok).toBe(false)
    expect(useTrackActionsStore.getState().busy).toBeNull()
    expect(useTrackActionsStore.getState().confirming).toBe('tr-0001')
    expect(useTrackActionsStore.getState().error).toEqual({
      trackId: 'tr-0001',
      message: 'trash failed',
    })
  })
})

describe('rename flow', () => {
  beforeEach(() => {
    mockRenameTrack.mockReset()
    reset()
  })

  it('toggles the inline rename editor', () => {
    useTrackActionsStore.getState().startRename('tr-0001')
    expect(useTrackActionsStore.getState().renaming).toBe('tr-0001')
    expect(useTrackActionsStore.getState().error).toBeNull()

    useTrackActionsStore.getState().cancelRename()
    expect(useTrackActionsStore.getState().renaming).toBeNull()
  })

  // Protects: the title argument is passed through, not dropped.
  it('success passes the title through and clears renaming', async () => {
    mockRenameTrack.mockResolvedValue(undefined)
    useTrackActionsStore.getState().startRename('tr-0001')
    const ok = await useTrackActionsStore.getState().submitRename('tr-0001', 'New Title')
    expect(ok).toBe(true)
    expect(mockRenameTrack).toHaveBeenCalledWith('tr-0001', 'New Title')
    expect(useTrackActionsStore.getState().busy).toBeNull()
    expect(useTrackActionsStore.getState().renaming).toBeNull()
    expect(useTrackActionsStore.getState().error).toBeNull()
  })

  it('failure records the error and keeps renaming set', async () => {
    mockRenameTrack.mockRejectedValue('rename failed')
    useTrackActionsStore.getState().startRename('tr-0001')
    const ok = await useTrackActionsStore.getState().submitRename('tr-0001', 'New Title')
    expect(ok).toBe(false)
    expect(useTrackActionsStore.getState().busy).toBeNull()
    expect(useTrackActionsStore.getState().renaming).toBe('tr-0001')
    expect(useTrackActionsStore.getState().error).toEqual({
      trackId: 'tr-0001',
      message: 'rename failed',
    })
  })
})

describe('setCover flow', () => {
  beforeEach(() => {
    mockSetTrackCover.mockReset()
    reset()
    vi.spyOn(useLibraryStore.getState(), 'load').mockResolvedValue(undefined)
  })

  // Protects: the sidecar is the source of truth and the row is built from it,
  // so without the reload the change is invisible until something else reloads.
  it('sets a cover and reloads the library', async () => {
    mockSetTrackCover.mockResolvedValue(undefined)
    const ok = await useTrackActionsStore.getState().setCover('tr-0001', 'ar-1')
    expect(ok).toBe(true)
    expect(mockSetTrackCover).toHaveBeenCalledWith('tr-0001', 'ar-1')
    expect(useLibraryStore.getState().load).toHaveBeenCalled()
    expect(useTrackActionsStore.getState().busy).toBeNull()
    expect(useTrackActionsStore.getState().error).toBeNull()
  })

  it('clears a cover with null and reloads the library', async () => {
    mockSetTrackCover.mockResolvedValue(undefined)
    const ok = await useTrackActionsStore.getState().setCover('tr-0001', null)
    expect(ok).toBe(true)
    expect(mockSetTrackCover).toHaveBeenCalledWith('tr-0001', null)
    expect(useLibraryStore.getState().load).toHaveBeenCalled()
  })

  it('failure stores the error against that track id and clears busy', async () => {
    mockSetTrackCover.mockRejectedValue('cover failed')
    const ok = await useTrackActionsStore.getState().setCover('tr-0001', 'ar-1')
    expect(ok).toBe(false)
    expect(useTrackActionsStore.getState().busy).toBeNull()
    expect(useTrackActionsStore.getState().error).toEqual({
      trackId: 'tr-0001',
      message: 'cover failed',
    })
  })
})

describe('export flow', () => {
  beforeEach(() => {
    mockPickExportPath.mockReset()
    mockExportTrack.mockReset()
    reset()
  })

  // Protects: cancelling the save dialog is the user's own decision, not a
  // fault, and must not be reported as an error or touch the busy marker.
  it('does nothing when the user cancels the save dialog', async () => {
    mockPickExportPath.mockResolvedValue(null)
    await useTrackActionsStore.getState().runExport('tr-0001', 'track.flac')
    expect(mockExportTrack).not.toHaveBeenCalled()
    expect(useTrackActionsStore.getState().error).toBeNull()
    expect(useTrackActionsStore.getState().busy).toBeNull()
  })

  it('calls exportTrack with the chosen path', async () => {
    mockPickExportPath.mockResolvedValue('/home/user/track.flac')
    await useTrackActionsStore.getState().runExport('tr-0001', 'track.flac')
    expect(mockExportTrack).toHaveBeenCalledWith('tr-0001', '/home/user/track.flac')
    expect(useTrackActionsStore.getState().busy).toBeNull()
  })
})

describe('reveal', () => {
  beforeEach(() => {
    mockRevealTrack.mockReset()
    reset()
  })

  it('calls revealTrack and records errors against the id', async () => {
    mockRevealTrack.mockRejectedValue('no file manager')
    await useTrackActionsStore.getState().reveal('tr-0001')
    expect(mockRevealTrack).toHaveBeenCalledWith('tr-0001')
    expect(useTrackActionsStore.getState().error).toEqual({
      trackId: 'tr-0001',
      message: 'no file manager',
    })
  })
})
