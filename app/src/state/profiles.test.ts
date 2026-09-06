import { describe, expect, it } from 'vitest'
import type { Config } from '../bridge/config'
import type { ModelsView, ProfileStatus } from '../bridge/models'
import {
  DEFAULT_PROFILE_ID,
  effectiveImageProfileId,
  effectiveProfileId,
  imageStudioNote,
  imageStudioState,
  installedOptions,
  optionsNote,
  pickable,
  profileRow,
  selectedImageProfile,
  selectedProfile,
} from './profiles'

function config(default_profile_id: string | null): Config {
  return {
    schema_version: 1,
    comfy: { mode: 'local', url: null, comfy_bin: null },
    llm: null,
    default_profile_id,
    default_image_profile_id: null,
    default_project_slug: null,
    export: { artist: null },
  }
}

function configWithImage(default_image_profile_id: string | null): Config {
  return {
    schema_version: 1,
    comfy: { mode: 'local', url: null, comfy_bin: null },
    llm: null,
    default_profile_id: null,
    default_image_profile_id,
    default_project_slug: null,
    export: { artist: null },
  }
}

function profile(overrides: Partial<ProfileStatus> & { id: string }): ProfileStatus {
  return {
    display_name: 'Test',
    kind: 'music',
    license: 'MIT',
    license_notes: null,
    source: 'shipped',
    vram_gb_min: null,
    template: null,
    readiness: { state: 'unknown' },
    ...overrides,
    id: overrides.id,
  }
}

function view(profiles: ProfileStatus[]): ModelsView {
  return {
    profiles,
    warnings: [],
    inventory_available: true,
    inventory_detail: null,
  }
}

describe('effectiveProfileId', () => {
  it('returns configured id when present', () => {
    expect(effectiveProfileId(config('minimax-music-3'))).toBe('minimax-music-3')
  })

  it('falls back to default when null', () => {
    expect(effectiveProfileId(config(null))).toBe(DEFAULT_PROFILE_ID)
  })

  it('falls back to default when empty', () => {
    expect(effectiveProfileId(config(''))).toBe(DEFAULT_PROFILE_ID)
  })

  it('falls back to default when whitespace', () => {
    expect(effectiveProfileId(config('   '))).toBe(DEFAULT_PROFILE_ID)
  })
})

describe('effectiveImageProfileId', () => {
  it('returns configured image id when present', () => {
    expect(effectiveImageProfileId(configWithImage('flux2-klein-9b'))).toBe('flux2-klein-9b')
  })

  it('returns null when null', () => {
    expect(effectiveImageProfileId(configWithImage(null))).toBeNull()
  })

  it('returns null when empty', () => {
    expect(effectiveImageProfileId(configWithImage(''))).toBeNull()
  })

  it('returns null when whitespace', () => {
    expect(effectiveImageProfileId(configWithImage('   '))).toBeNull()
  })
})

describe('pickable', () => {
  it('filters by kind and orders curated first', () => {
    const userReady = profile({
      id: 'user-ready',
      kind: 'music',
      source: 'user',
      readiness: { state: 'ready' },
    })
    const shippedUnknown = profile({
      id: 'shipped-unknown',
      kind: 'music',
      source: 'shipped',
      readiness: { state: 'unknown' },
    })
    const image = profile({ id: 'image', kind: 'image', source: 'shipped' })
    const result = pickable(view([userReady, shippedUnknown, image]), 'music')
    expect(result.map((p) => p.id)).toEqual(['shipped-unknown', 'user-ready'])
  })
})

describe('selectedProfile', () => {
  it('returns matching profile when configured id exists', () => {
    const p = profile({ id: 'minimax-music-3' })
    expect(selectedProfile(view([p]), config('minimax-music-3'))).toEqual(p)
  })

  it('returns null when configured id is not in list', () => {
    expect(selectedProfile(view([profile({ id: 'other' })]), config('missing'))).toBeNull()
  })

  it('returns null when list has not loaded', () => {
    expect(selectedProfile(null, config('minimax-music-3'))).toBeNull()
  })
})

describe('selectedImageProfile', () => {
  it('returns matching image profile when configured id exists', () => {
    const p = profile({ id: 'flux2-klein-9b', kind: 'image' })
    expect(selectedImageProfile(view([p]), configWithImage('flux2-klein-9b'))).toEqual(p)
  })

  it('returns null when configured id is not in list', () => {
    expect(
      selectedImageProfile(view([profile({ id: 'other', kind: 'image' })]), configWithImage('missing')),
    ).toBeNull()
  })

  it('returns null when list has not loaded', () => {
    expect(selectedImageProfile(null, configWithImage('flux2-klein-9b'))).toBeNull()
  })

  it('returns null when no image profile is chosen', () => {
    expect(
      selectedImageProfile(view([profile({ id: 'flux2-klein-9b', kind: 'image' })]), configWithImage(null)),
    ).toBeNull()
  })
})

describe('imageStudioState', () => {
  it('is loading when the view has not loaded', () => {
    expect(imageStudioState(null, configWithImage(null))).toBe('loading')
  })

  it('is no-profiles when the list has no image profiles, even if a music id is configured', () => {
    const music = profile({ id: 'music', kind: 'music' })
    expect(imageStudioState(view([music]), configWithImage(null))).toBe('no-profiles')
  })

  it('is none-chosen when image profiles exist but none is chosen', () => {
    const img = profile({ id: 'img', kind: 'image' })
    expect(imageStudioState(view([img]), configWithImage(null))).toBe('none-chosen')
  })

  it('is missing when the configured id is not in the list', () => {
    const img = profile({ id: 'img', kind: 'image' })
    expect(imageStudioState(view([img]), configWithImage('gone'))).toBe('missing')
  })

  /**
   * Invariant: a *music* id in the image slot is `missing`, never `ready`.
   * The two default fields are independent and `config.json` is editable, so
   * this is reachable -- and reading it as ready would put a music param panel
   * in Cover Art with only the backend's kind guard behind it.
   */
  it('is missing when the configured id names a music profile', () => {
    const music = profile({ id: 'music', kind: 'music' })
    const img = profile({ id: 'img', kind: 'image' })
    expect(imageStudioState(view([music, img]), configWithImage('music'))).toBe('missing')
  })

  it('is ready when a configured image profile is loaded', () => {
    const img = profile({ id: 'img', kind: 'image' })
    expect(imageStudioState(view([img]), configWithImage('img'))).toBe('ready')
  })
})

describe('imageStudioNote', () => {
  it('says nothing while loading', () => {
    expect(imageStudioNote('loading', null)).toBeNull()
  })

  it('says nothing when ready', () => {
    expect(imageStudioNote('ready', null)).toBeNull()
  })

  it('points to the catalog when no image profiles exist', () => {
    expect(imageStudioNote('no-profiles', null)).toBe(
      'No image model profile yet. Install one on Setup.',
    )
  })

  it('asks the user to pick when profiles exist but none is chosen', () => {
    expect(imageStudioNote('none-chosen', null)).toBe('Pick an image model to start.')
  })

  it('names the missing configured id', () => {
    expect(imageStudioNote('missing', 'gone')).toBe(
      'The configured image profile gone is not among the loaded profiles. Pick one here to continue.',
    )
  })
})

describe('profileRow', () => {
  it('maps shipped profile origin', () => {
    const row = profileRow(profile({ id: 'ace', source: 'shipped', license: 'Apache-2.0' }))
    expect(row.origin).toBe('Shipped')
    expect(row.license).toBe('Apache-2.0')
  })

  it('maps user profile origin', () => {
    const row = profileRow(profile({ id: 'mine', source: 'user' }))
    expect(row.origin).toBe('Yours')
  })

  it('leaves vramClaim null when undeclared', () => {
    const row = profileRow(profile({ id: 'no-vram', vram_gb_min: null }))
    expect(row.vramClaim).toBeNull()
  })

  it('words vram as a claim when declared', () => {
    const row = profileRow(profile({ id: 'vram', vram_gb_min: 8 }))
    expect(row.vramClaim).toBe('Profile states 8 GB VRAM')
  })

  it('license is non-empty for shipped profiles', () => {
    const row = profileRow(profile({ id: 'shipped', source: 'shipped', license: 'Apache-2.0' }))
    expect(row.license).not.toBe('')
  })
})

/** A view whose inventory could not be read -- ComfyUI down. */
function blindView(profiles: ProfileStatus[]): ModelsView {
  return {
    profiles,
    warnings: [],
    inventory_available: false,
    inventory_detail: 'Start ComfyUI.',
  }
}

describe('installedOptions', () => {
  it('offers nothing while the list has not loaded', () => {
    expect(installedOptions(null, 'music', 'a')).toEqual([])
  })

  it('offers only installed models of the asked-for kind', () => {
    const v = view([
      profile({ id: 'ready-music', display_name: 'Ready', readiness: { state: 'ready' } }),
      profile({ id: 'missing-music', readiness: { state: 'missing', files: [], total_bytes: null, installable: true } }),
      profile({ id: 'ready-image', kind: 'image', readiness: { state: 'ready' } }),
    ])
    expect(installedOptions(v, 'music', null)).toEqual([{ id: 'ready-music', name: 'Ready' }])
  })

  // The whole point of the guard: `unknown` is "could not check", so filtering
  // on it would hand a user with a working install an empty dropdown.
  it('offers every model when the inventory could not be read', () => {
    const v = blindView([
      profile({ id: 'one', display_name: 'One' }),
      profile({ id: 'two', display_name: 'Two' }),
    ])
    expect(installedOptions(v, 'music', null).map((o) => o.id)).toEqual(['one', 'two'])
  })

  // A `<select>` whose value matches no option renders the first one, which
  // would claim a model the user never chose.
  it('includes the chosen model even when it is not installed', () => {
    const v = view([
      profile({ id: 'chosen', display_name: 'Chosen', readiness: { state: 'missing', files: [], total_bytes: null, installable: true } }),
      profile({ id: 'other', display_name: 'Other', readiness: { state: 'ready' } }),
    ])
    expect(installedOptions(v, 'music', 'chosen').map((o) => o.id)).toEqual(['chosen', 'other'])
  })

  it('does not duplicate the chosen model when it is installed', () => {
    const v = view([profile({ id: 'chosen', readiness: { state: 'ready' } })])
    expect(installedOptions(v, 'music', 'chosen').map((o) => o.id)).toEqual(['chosen'])
  })

  it('ignores a chosen id no profile answers to', () => {
    const v = view([profile({ id: 'real', readiness: { state: 'ready' } })])
    expect(installedOptions(v, 'music', 'ghost').map((o) => o.id)).toEqual(['real'])
  })
})

describe('optionsNote', () => {
  it('says nothing while the list has not loaded', () => {
    expect(optionsNote(null, 'music', null)).toBeNull()
  })

  it('says nothing when the chosen model is installed', () => {
    const v = view([profile({ id: 'a', readiness: { state: 'ready' } })])
    expect(optionsNote(v, 'music', 'a')).toBeNull()
  })

  // Reported before "nothing installed": with ComfyUI down the app does not
  // know that nothing is installed.
  it('blames the unreadable inventory before claiming nothing is installed', () => {
    const v = blindView([profile({ id: 'a' })])
    const note = optionsNote(v, 'music', 'a')
    expect(note).toContain('ComfyUI is not running')
    expect(note).toContain('Setup')
  })

  it('points at Setup when no model of the kind is installed', () => {
    const v = view([profile({ id: 'a', kind: 'image', readiness: { state: 'missing', files: [], total_bytes: null, installable: true } })])
    expect(optionsNote(v, 'image', null)).toBe('No image model is installed. Install one on Setup.')
  })

  it('names a chosen model that is not installed', () => {
    const v = view([
      profile({ id: 'a', display_name: 'Chroma', readiness: { state: 'missing', files: [], total_bytes: null, installable: true } }),
      profile({ id: 'b', readiness: { state: 'ready' } }),
    ])
    expect(optionsNote(v, 'music', 'a')).toContain('Chroma is selected but not installed')
  })
})
