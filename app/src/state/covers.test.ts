import { describe, expect, it } from 'vitest'
import type { AlbumList } from '../bridge/projects'
import type { ArtRow } from './art'
import type { TrackRow } from './library'
import { coverChoices, coverUsage, coverView, deleteArtPrompt } from './covers'

const art: ArtRow[] = [
  {
    id: 'ar-1',
    name: 'Cover One',
    model: 'FLUX Schnell',
    license: 'Apache-2.0',
    size: '768 x 768',
    created: '2026-09-03',
    seed: '42',
    promptId: null,
    file: 'art/ar-1.png',
    url: 'url-1',
  },
  {
    id: 'ar-2',
    name: 'Cover Two',
    model: 'FLUX Schnell',
    license: 'Apache-2.0',
    size: '512 x 512',
    created: '2026-09-03',
    seed: '43',
    promptId: null,
    file: 'art/ar-2.png',
    url: 'url-2',
  },
]

function track(id: string, cover: string | null): TrackRow {
  return {
    id,
    name: id,
    model: 'ACE-Step 1.5 XL Turbo',
    license: 'Apache-2.0',
    duration: '2:00',
    created: '2026-09-03',
    loras: '',
    seed: '1',
    promptId: null,
    file: `tracks/${id}.flac`,
    cover,
  }
}

function album(name: string, cover: string | null): AlbumList {
  return { name, tracks: [], cover }
}

describe('coverView', () => {
  it('a null cover is none', () => {
    expect(coverView(null, art)).toEqual({ state: 'none' })
  })

  it('a cover the gallery has is shown with its name and URL', () => {
    expect(coverView('ar-1', art)).toEqual({
      state: 'shown',
      id: 'ar-1',
      name: 'Cover One',
      url: 'url-1',
    })
  })

  it('a cover id no artwork answers to is missing, carrying the id', () => {
    expect(coverView('ar-missing', art)).toEqual({ state: 'missing', id: 'ar-missing' })
  })
})

describe('coverUsage', () => {
  it('counts tracks and albums separately, and counts nothing for an unrelated id', () => {
    const tracks: TrackRow[] = [track('tr-1', 'ar-1'), track('tr-2', 'ar-1'), track('tr-3', null)]
    const albums: AlbumList[] = [album('A', 'ar-1'), album('B', 'ar-2'), album('C', null)]
    expect(coverUsage('ar-1', tracks, albums)).toEqual({ tracks: 2, albums: 1 })
    expect(coverUsage('ar-2', tracks, albums)).toEqual({ tracks: 0, albums: 1 })
    expect(coverUsage('ar-missing', tracks, albums)).toEqual({ tracks: 0, albums: 0 })
  })
})

describe('deleteArtPrompt', () => {
  it('states the rule with no counts when nothing uses it', () => {
    expect(deleteArtPrompt('Cover One', { tracks: 0, albums: 0 })).toBe(
      'Delete “Cover One”? The image and its record go to the Recycle Bin, and anything using it as a cover loses it.',
    )
  })

  it('appends singular and plural correctly, and omits a zero side', () => {
    expect(deleteArtPrompt('Cover One', { tracks: 1, albums: 0 })).toBe(
      'Delete “Cover One”? The image and its record go to the Recycle Bin, and anything using it as a cover loses it. It is the cover for 1 track.',
    )
    expect(deleteArtPrompt('Cover One', { tracks: 0, albums: 1 })).toBe(
      'Delete “Cover One”? The image and its record go to the Recycle Bin, and anything using it as a cover loses it. It is the cover for 1 album.',
    )
    expect(deleteArtPrompt('Cover One', { tracks: 2, albums: 1 })).toBe(
      'Delete “Cover One”? The image and its record go to the Recycle Bin, and anything using it as a cover loses it. It is the cover for 2 tracks and 1 album.',
    )
  })
})

describe('coverChoices', () => {
  it('puts No cover first with id null, then the gallery in order', () => {
    expect(coverChoices(art)).toEqual([
      { id: null, label: 'No cover' },
      { id: 'ar-1', label: 'Cover One' },
      { id: 'ar-2', label: 'Cover Two' },
    ])
  })
})
