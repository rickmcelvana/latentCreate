import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ArtSet, ArtWarning, Artwork } from '../bridge/art'
import type { InputValue } from './params'
import { useAlbumsStore } from './albums'
import { useLibraryStore } from './library'
import { EMPTY_ART, artRows, artWarningLine, useArtStore } from './art'

const mockListArt = vi.fn()
const mockArtImageUrl = vi.fn()
const mockSubscribeArt = vi.fn()
const mockDeleteArt = vi.fn()
// Togglable, the way `projects.test.ts` holds it: `startListening` is guarded on
// `isTauri()`, so a fixed `false` would make its test assert on a call the guard
// had already refused -- a guard that reads as a subscription that happened.
let mockIsTauri = false

vi.mock('../bridge/art', () => ({
  listArt: () => mockListArt(),
  artImageUrl: (id: string) => mockArtImageUrl(id),
  subscribeArt: (cb: (e: { id: string; project_slug: string; file: string }) => void) =>
    mockSubscribeArt(cb),
  deleteArt: (id: string) => mockDeleteArt(id),
}))

vi.mock('../bridge/jobs', () => ({ isTauri: () => mockIsTauri }))

function makeArtwork(overrides: {
  id?: string
  title?: string | null
  file?: string
  width?: number | null
  height?: number | null
  profile_display_name?: string
  profile_id?: string
  license?: string
  prompt_id?: string | null
  created_at?: string
  inputs?: Record<string, InputValue>
} = {}): Artwork {
  const id = overrides.id ?? 'ar-0001'
  return {
    id,
    title: overrides.title ?? null,
    file: overrides.file ?? `art/${id}.png`,
    // `=== undefined`, not `??`: the default is a number, so `??` would swallow
    // an explicit `null` and quietly hand back 768 -- the missing-size case
    // would then never be built, and the test claiming to check it would pass
    // against a size that was there all along.
    width: overrides.width === undefined ? 768 : overrides.width,
    height: overrides.height === undefined ? 768 : overrides.height,
    provenance: {
      profile_id: overrides.profile_id ?? 'flux-schnell',
      profile_display_name: overrides.profile_display_name ?? 'FLUX Schnell',
      model_license: overrides.license ?? 'Apache-2.0',
      template: null,
      spec: {
        profile_id: overrides.profile_id ?? 'flux-schnell',
        inputs: overrides.inputs ?? { seed: { type: 'seed', value: 42 } },
        loras: [],
        lyrics: null,
        title: null,
      },
      resolved_slots: {},
      comfy: null,
      created_at: overrides.created_at ?? '2026-09-03T10:00:00Z',
      prompt_id: overrides.prompt_id ?? null,
    },
  }
}

function makeSet(art: Artwork[], warnings: ArtWarning[] = []): ArtSet {
  return { art, warnings }
}

describe('artRows', () => {
  it('carries the title, else the id', () => {
    const rows = artRows(
      makeSet([
        makeArtwork({ id: 'ar-1', title: 'Cover One' }),
        makeArtwork({ id: 'ar-2', title: null }),
        makeArtwork({ id: 'ar-3', title: '   ' }),
      ]),
      { 'ar-1': 'url-1', 'ar-2': 'url-2', 'ar-3': 'url-3' },
    )
    expect(rows[0].name).toBe('Cover One')
    expect(rows[1].name).toBe('ar-2')
    expect(rows[2].name).toBe('ar-3')
  })

  it('formats size when both dimensions are present, and shows -- otherwise', () => {
    const rows = artRows(
      makeSet([
        makeArtwork({ id: 'ar-1', width: 768, height: 768 }),
        makeArtwork({ id: 'ar-2', width: null, height: null }),
        makeArtwork({ id: 'ar-3', width: 512, height: null }),
      ]),
      {},
    )
    expect(rows[0].size).toBe('768 x 768')
    expect(rows[1].size).toBe('--')
    expect(rows[2].size).toBe('--')
  })

  it('maps every artwork to a URL, leaving null for a failed resolution', () => {
    const rows = artRows(
      makeSet([makeArtwork({ id: 'ar-1' }), makeArtwork({ id: 'ar-2' })]),
      { 'ar-1': 'url-1' },
    )
    expect(rows[0].url).toBe('url-1')
    expect(rows[1].url).toBeNull()
  })
})

describe('artWarningLine', () => {
  it('returns null for no warnings', () => {
    expect(artWarningLine([])).toBeNull()
  })

  it('returns one sentence naming the count and folder', () => {
    expect(artWarningLine([{ kind: 'missing', id: 'ar-1' }])).toBe(
      "1 artwork sidecar could not be read; check the files in your project's art folder.",
    )
    expect(
      artWarningLine([
        { kind: 'missing', id: 'ar-1' },
        { kind: 'malformed', id: 'ar-2', detail: 'bad json' },
      ]),
    ).toBe(
      "2 artwork sidecars could not be read; check the files in your project's art folder.",
    )
  })
})

describe('EMPTY_ART', () => {
  it('is the empty-state sentence', () => {
    expect(EMPTY_ART).toBe(
      'Cover art you generate will appear here, with the recipe that made it.',
    )
  })
})

describe('art store', () => {
  beforeEach(() => {
    mockListArt.mockReset()
    mockArtImageUrl.mockReset()
    mockSubscribeArt.mockReset()
    mockDeleteArt.mockReset()
    mockIsTauri = false

    useArtStore.setState({
      art: [],
      byId: {},
      warnings: null,
      loading: false,
      error: null,
      listening: false,
      confirmingDelete: null,
    })
  })

  it('load resolves a URL per artwork and keeps the row when one fails', async () => {
    const works = makeArtwork({ id: 'ar-1' })
    const broken = makeArtwork({ id: 'ar-2' })
    mockListArt.mockResolvedValue(makeSet([works, broken]))
    mockArtImageUrl.mockImplementation(async (id: string) => {
      if (id === 'ar-1') return 'url-1'
      throw new Error('denied')
    })

    await useArtStore.getState().load()

    expect(mockArtImageUrl).toHaveBeenCalledTimes(2)
    expect(useArtStore.getState().art).toHaveLength(2)
    expect(useArtStore.getState().art[0].url).toBe('url-1')
    expect(useArtStore.getState().art[1].url).toBeNull()
    expect(useArtStore.getState().error).toBeNull()
  })

  it('load surfaces a listing failure and clears loading', async () => {
    mockListArt.mockRejectedValue(new Error('disk failed'))

    await useArtStore.getState().load()

    expect(useArtStore.getState().error).toBe('disk failed')
    expect(useArtStore.getState().loading).toBe(false)
  })

  it('load re-resolves URLs on a second call', async () => {
    const art = makeArtwork({ id: 'ar-1' })
    mockListArt.mockResolvedValue(makeSet([art]))
    mockArtImageUrl.mockResolvedValue('url-1')

    await useArtStore.getState().load()
    await useArtStore.getState().load()

    expect(mockArtImageUrl).toHaveBeenCalledTimes(2)
    expect(mockArtImageUrl).toHaveBeenLastCalledWith('ar-1')
  })

  it('load keeps raw artworks by id', async () => {
    const art = makeArtwork({ id: 'ar-1', inputs: { seed: { type: 'seed', value: 9 } } })
    mockListArt.mockResolvedValue(makeSet([art]))
    mockArtImageUrl.mockResolvedValue('url-1')

    await useArtStore.getState().load()

    expect(useArtStore.getState().byId['ar-1']).toEqual(art)
  })

  it('startListening subscribes once', async () => {
    mockIsTauri = true
    mockSubscribeArt.mockResolvedValue(vi.fn())
    await useArtStore.getState().startListening()
    await useArtStore.getState().startListening()
    expect(mockSubscribeArt).toHaveBeenCalledTimes(1)
  })

  /**
   * Invariant: outside Tauri nothing is subscribed **and `listening` stays
   * false**. Setting the flag before the guard would leave a store that can
   * never listen -- the browser dev server mounts the view once, and the flag
   * would still be set when the same store runs in the app.
   */
  it('startListening outside Tauri neither subscribes nor arms the guard', async () => {
    await useArtStore.getState().startListening()
    expect(mockSubscribeArt).not.toHaveBeenCalled()
    expect(useArtStore.getState().listening).toBe(false)
  })

  /**
   * Invariant: a sidecar that could not be read is a sentence, not an empty
   * gallery. The frontend half of `list_art` returning warnings rather than an
   * error.
   */
  it('load lists the readable artwork and reports the rest as a warning', async () => {
    mockListArt.mockResolvedValue(
      makeSet([makeArtwork({ id: 'ar-1' })], [{ kind: 'malformed', id: 'ar-2', detail: 'bad json' }]),
    )
    mockArtImageUrl.mockResolvedValue('url-1')

    await useArtStore.getState().load()

    expect(useArtStore.getState().art.map((row) => row.id)).toEqual(['ar-1'])
    expect(useArtStore.getState().warnings).toBe(
      "1 artwork sidecar could not be read; check the files in your project's art folder.",
    )
    expect(useArtStore.getState().error).toBeNull()
  })
})

describe('delete flow', () => {
  beforeEach(() => {
    // Every mock this suite touches, not just its own: `remove` calls the
    // gallery's `load`, so a `listArt` left armed by the previous describe's
    // last test would decide whether `error` is null here. A guard that reads
    // an earlier test's state proves nothing (T-505d-d, T-506c-b).
    mockDeleteArt.mockReset()
    mockListArt.mockReset()
    mockArtImageUrl.mockReset()
    mockListArt.mockResolvedValue(makeSet([]))
    mockArtImageUrl.mockResolvedValue('url-1')
    useArtStore.setState({
      art: [],
      byId: {},
      warnings: null,
      loading: false,
      error: null,
      listening: false,
      confirmingDelete: null,
    })
    vi.spyOn(useLibraryStore.getState(), 'load').mockResolvedValue(undefined)
    vi.spyOn(useAlbumsStore.getState(), 'load').mockResolvedValue(undefined)
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('arms and disarms the inline confirm', () => {
    useArtStore.getState().askDelete('ar-1')
    expect(useArtStore.getState().confirmingDelete).toBe('ar-1')
    expect(useArtStore.getState().error).toBeNull()

    useArtStore.getState().cancelDelete()
    expect(useArtStore.getState().confirmingDelete).toBeNull()
  })

  // Protects: a track's cover was cleared on disk by this call, and a stale row
  // would show a cover that is gone.
  it('remove reloads the gallery, the library and the albums', async () => {
    mockDeleteArt.mockResolvedValue(undefined)
    const ok = await useArtStore.getState().remove('ar-1')
    expect(ok).toBe(true)
    expect(mockDeleteArt).toHaveBeenCalledWith('ar-1')
    expect(mockListArt).toHaveBeenCalledTimes(1)
    expect(useLibraryStore.getState().load).toHaveBeenCalled()
    expect(useAlbumsStore.getState().load).toHaveBeenCalled()
    expect(useArtStore.getState().confirmingDelete).toBeNull()
    expect(useArtStore.getState().error).toBeNull()
  })

  // Protects: the tile stays in confirm state so the user can retry or cancel.
  it('a failed remove keeps confirmingDelete set, stores the error, and reloads nothing', async () => {
    mockDeleteArt.mockRejectedValue(new Error('trash failed'))
    useArtStore.getState().askDelete('ar-1')
    const ok = await useArtStore.getState().remove('ar-1')
    expect(ok).toBe(false)
    expect(mockDeleteArt).toHaveBeenCalledWith('ar-1')
    expect(mockListArt).not.toHaveBeenCalled()
    expect(useLibraryStore.getState().load).not.toHaveBeenCalled()
    expect(useAlbumsStore.getState().load).not.toHaveBeenCalled()
    expect(useArtStore.getState().confirmingDelete).toBe('ar-1')
    expect(useArtStore.getState().error).toBe('trash failed')
  })
})
