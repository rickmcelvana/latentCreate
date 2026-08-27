import { describe, expect, it } from 'vitest'
import type { Config } from '../bridge/config'
import type { ModelsView, ProfileStatus } from '../bridge/models'
import {
  DEFAULT_PROFILE_ID,
  effectiveProfileId,
  pickable,
  profileRow,
  selectedProfile,
} from './profiles'

function config(default_profile_id: string | null): Config {
  return {
    schema_version: 1,
    comfy: { mode: 'local', url: null, comfy_bin: null },
    llm: null,
    default_profile_id,
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
