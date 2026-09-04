import { describe, expect, it } from 'vitest'
import type { Provenance } from '../bridge/library'
import type { InputValue } from './params'
import {
  createdDate,
  modelLabel,
  provenanceView,
  seedText,
  type ProvenanceSection,
} from './provenance'

function makeProvenance(overrides: {
  profile_display_name?: string
  profile_id?: string
  license?: string
  template?: string | null
  inputs?: Record<string, InputValue>
  lyrics?: { doc_id: string; version: number } | null
  resolved_slots?: Record<string, InputValue>
  comfy?: {
    comfyui_version: string | null
    comfy_cli_version: string | null
    url: string | null
  }
  created_at?: string
  prompt_id?: string | null
} = {}): Provenance {
  return {
    profile_id: overrides.profile_id ?? 'ace-step-1.5-turbo',
    profile_display_name: overrides.profile_display_name ?? 'ACE-Step 1.5 XL Turbo',
    model_license: overrides.license ?? 'Apache-2.0',
    template: overrides.template ?? null,
    spec: {
      profile_id: overrides.profile_id ?? 'ace-step-1.5-turbo',
      inputs: overrides.inputs ?? {},
      loras: [],
      lyrics: overrides.lyrics ?? null,
      title: null,
    },
    resolved_slots: overrides.resolved_slots ?? {},
    comfy: overrides.comfy ?? null,
    created_at: overrides.created_at ?? '2026-08-29T05:56:07Z',
    prompt_id: overrides.prompt_id ?? null,
  }
}

describe('modelLabel', () => {
  it('returns the display name when present', () => {
    expect(modelLabel(makeProvenance())).toBe('ACE-Step 1.5 XL Turbo')
  })

  it('falls back to the profile id when the display name is empty', () => {
    expect(
      modelLabel(
        makeProvenance({ profile_display_name: '', profile_id: 'ace-step-1.5-turbo' }),
      ),
    ).toBe('ace-step-1.5-turbo')
  })

  /**
   * Invariant: a display name that is only whitespace is empty, not a name.
   * It renders as a blank where the model should be, and an emitted profile
   * takes its display name from a workflow filename -- so `'   '` is a value
   * that can really arrive. The `.trim()` this pins was carried through the
   * T-506c-c extraction untested; the artwork tiles share it now, so the gap
   * would have shown in two places.
   */
  it('treats a whitespace-only display name as empty', () => {
    expect(
      modelLabel(makeProvenance({ profile_display_name: '   ', profile_id: 'flux-schnell' })),
    ).toBe('flux-schnell')
  })

  it('falls back to Unknown model when both are empty', () => {
    expect(modelLabel(makeProvenance({ profile_display_name: '', profile_id: '' }))).toBe(
      'Unknown model',
    )
  })
})

describe('seedText', () => {
  it('returns the seed value as text when present', () => {
    expect(
      seedText(makeProvenance({ inputs: { seed: { type: 'seed', value: 42 } } })),
    ).toBe('42')
  })

  it('returns -- when no seed input exists', () => {
    expect(seedText(makeProvenance())).toBe('--')
  })
})

describe('createdDate', () => {
  it('returns the date half of the timestamp', () => {
    expect(createdDate(makeProvenance({ created_at: '2026-08-29T05:56:07Z' }))).toBe(
      '2026-08-29',
    )
  })
})

describe('provenanceView', () => {
  it('renders inputs by name, a seed as its number and not [object Object]', () => {
    const p = makeProvenance({
      inputs: {
        tags: { type: 'text', value: 'synthwave' },
        seed: { type: 'seed', value: 42 },
      },
    })
    const sections = provenanceView(p)
    const inputs = sections.find((s: ProvenanceSection) => s.title === 'Inputs')
    expect(inputs?.facts).toEqual([
      { label: 'tags', value: 'synthwave' },
      { label: 'seed', value: '42' },
    ])
  })

  it('builds every section from a full sidecar', () => {
    const p = makeProvenance({
      inputs: { seed: { type: 'seed', value: 7 } },
      template: 'audio_ace_step1_5_xl_turbo',
      lyrics: { doc_id: 'ld-0001', version: 2 },
      resolved_slots: { '94.duration': { type: 'float', value: 120 } },
      comfy: {
        comfyui_version: '0.3.26',
        comfy_cli_version: '0.1.0',
        url: 'http://127.0.0.1:8188',
      },
    })
    const titles = provenanceView(p).map((s) => s.title)
    expect(titles).toEqual(['Inputs', 'Lyrics', 'Resolved slots', 'Server'])

    const view = provenanceView(p)
    expect(view.find((s) => s.title === 'Lyrics')?.facts).toEqual([
      { label: 'Document', value: 'ld-0001, v2' },
    ])
    expect(view.find((s) => s.title === 'Resolved slots')?.facts).toEqual([
      { label: '94.duration', value: '120' },
    ])
    expect(view.find((s) => s.title === 'Server')?.facts).toEqual([
      { label: 'ComfyUI', value: '0.3.26' },
      { label: 'comfy-cli', value: '0.1.0' },
      { label: 'Endpoint', value: 'http://127.0.0.1:8188' },
      { label: 'Template', value: 'audio_ace_step1_5_xl_turbo' },
    ])
  })

  it('omits empty sections, so an older sidecar still renders cleanly', () => {
    expect(provenanceView(makeProvenance())).toEqual([])
  })
})
