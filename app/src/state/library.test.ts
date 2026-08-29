import { describe, expect, it } from 'vitest'
import { EMPTY_LIBRARY, trackRows, warningLine, type TrackRow } from './library'
import { type Track, type TrackSet, type TrackWarning } from '../bridge/library'
import { type LoraRef } from '../bridge/generate'
import type { InputValue } from './params'

function makeTrack(overrides: {
  id: string
  title?: string | null
  duration_s?: number | null
  profile_display_name?: string
  profile_id?: string
  loras?: LoraRef[]
  prompt_id?: string | null
  created_at?: string
  file?: string
  license?: string
  inputs?: Record<string, InputValue>
}): Track {
  const id = overrides.id
  return {
    id,
    title: overrides.title ?? null,
    file: overrides.file ?? `tracks/${id}.flac`,
    duration_s: overrides.duration_s ?? null,
    provenance: {
      profile_id: overrides.profile_id ?? 'ace-step-1.5-turbo',
      profile_display_name: overrides.profile_display_name ?? 'ACE-Step 1.5 XL Turbo',
      model_license: overrides.license ?? 'Apache-2.0',
      template: null,
      spec: {
        profile_id: overrides.profile_id ?? 'ace-step-1.5-turbo',
        inputs: overrides.inputs ?? {},
        loras: overrides.loras ?? [],
        lyrics: null,
      },
      resolved_slots: {},
      comfy: null,
      created_at: overrides.created_at ?? '2026-08-29T05:56:07Z',
      prompt_id: overrides.prompt_id ?? null,
    },
  }
}

function makeSet(tracks: Track[], warnings: TrackWarning[] = []): TrackSet {
  return { tracks, warnings }
}

describe('trackRows', () => {
  it('formats duration as m:ss with zero-padded seconds, flooring', () => {
    const rows = trackRows(
      makeSet([
        makeTrack({ id: 'tr-1', duration_s: 120.0 }),
        makeTrack({ id: 'tr-2', duration_s: 59.0 }),
        makeTrack({ id: 'tr-3', duration_s: 119.6 }),
        makeTrack({ id: 'tr-4', duration_s: null }),
      ]),
    )
    expect(rows[0].duration).toBe('2:00')
    expect(rows[1].duration).toBe('0:59')
    expect(rows[2].duration).toBe('1:59')
    expect(rows[3].duration).toBe('--')
  })

  it('falls back to the id when title is null or whitespace', () => {
    const rows = trackRows(
      makeSet([
        makeTrack({ id: 'tr-1', title: null }),
        makeTrack({ id: 'tr-2', title: '   ' }),
        makeTrack({ id: 'tr-3', title: 'Named' }),
      ]),
    )
    expect(rows[0].name).toBe('tr-1')
    expect(rows[1].name).toBe('tr-2')
    expect(rows[2].name).toBe('Named')
  })

  it('falls back to profile id, then to Unknown model', () => {
    const rows = trackRows(
      makeSet([
        makeTrack({
          id: 'tr-1',
          profile_display_name: '',
          profile_id: 'ace-step-1.5-turbo',
        }),
        makeTrack({ id: 'tr-2', profile_display_name: '', profile_id: '' }),
      ]),
    )
    expect(rows[0].model).toBe('ace-step-1.5-turbo')
    expect(rows[1].model).toBe('Unknown model')
  })

  it('lists enabled LoRA stems in order with strengths and leaves stored paths untouched', () => {
    const loras: LoraRef[] = [
      {
        file: 'ACE-Step-v1.5-acoustic-guitar-and-a-merge-LoRA\\vocal_instrument_merge_adapter_model.safetensors',
        strength: 1.0,
        enabled: true,
      },
      { file: 'adapter_model.safetensors', strength: 0.8, enabled: true },
      { file: 'disabled_lora.safetensors', strength: 0.5, enabled: false },
    ]
    const track = makeTrack({ id: 'tr-1', loras })
    const rows = trackRows(makeSet([track]))
    expect(rows[0].loras).toBe(
      'vocal_instrument_merge_adapter_model x1.0, adapter_model x0.8',
    )
    expect(track.provenance.spec.loras[0].file).toBe(
      'ACE-Step-v1.5-acoustic-guitar-and-a-merge-LoRA\\vocal_instrument_merge_adapter_model.safetensors',
    )
  })

  it('produces a complete row when prompt_id is null', () => {
    const rows = trackRows(makeSet([makeTrack({ id: 'tr-0001', prompt_id: null })]))
    expect(rows[0]).toMatchObject<TrackRow>({
      id: 'tr-0001',
      name: 'tr-0001',
      model: 'ACE-Step 1.5 XL Turbo',
      license: 'Apache-2.0',
      duration: '--',
      created: '2026-08-29',
      loras: '',
      seed: '--',
      promptId: null,
      file: 'tracks/tr-0001.flac',
    })
  })
})

describe('warningLine', () => {
  it('returns null for no warnings', () => {
    expect(warningLine([])).toBeNull()
  })

  it('returns one sentence naming the count and what to do', () => {
    expect(warningLine([{ kind: 'missing', id: 'tr-1' }])).toBe(
      "1 track sidecar could not be read; check the files in your project's tracks folder.",
    )
    expect(
      warningLine([
        { kind: 'missing', id: 'tr-1' },
        { kind: 'malformed', id: 'tr-2', detail: 'bad json' },
      ]),
    ).toBe(
      "2 track sidecars could not be read; check the files in your project's tracks folder.",
    )
  })
})

describe('EMPTY_LIBRARY', () => {
  it('is the empty-state sentence', () => {
    expect(EMPTY_LIBRARY).toBe(
      'Tracks you generate will appear here, with the recipe that made them.',
    )
  })
})
