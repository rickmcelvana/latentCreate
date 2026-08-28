import { describe, expect, it } from 'vitest'
import aceProfile from '../../../profiles/ace-step-1.5-turbo.json'
import type { Submission } from '../bridge/generate'
import type { LyricDoc, LyricVersion } from '../bridge/lyricdoc'
import type { ProfileInputs } from '../bridge/profiles'
import {
  GENERATE,
  QUEUED,
  QUEUEING,
  approvedFill,
  approvedOffer,
  blockers,
  lyricRefFor,
  notesFor,
  specFor,
  submissionNotes,
} from './generate'
import type { StackRow } from './loras'
import { defaults, panelModel } from './params'

const model = panelModel(aceProfile.inputs as unknown as ProfileInputs)

const APPROVED = '[Verse]\nthe approved words\n\n[Chorus]\nexactly these'

function version(number: number, text: string): LyricVersion {
  return {
    number,
    text,
    created_at: '2026-08-28T00:00:00Z',
    source: { kind: 'llm', model: 'qwen3.5:9b', prompt_optimized: false },
  }
}

function doc(overrides: Partial<LyricDoc> = {}): LyricDoc {
  return {
    id: 'lyric-01',
    title: 'A Song',
    approved: 2,
    versions: [version(1, 'an earlier draft'), version(2, APPROVED)],
    ...overrides,
  }
}

function values(overrides: Record<string, string | number> = {}) {
  return { ...defaults(model), ...overrides }
}

function submission(overrides: Partial<Submission> = {}): Submission {
  return {
    prompt_id: 'p-1',
    workflow_path: 'C:/jobs/1/workflow.json',
    unchecked_slots: [],
    lora_nodes: [],
    output_format: null,
    ...overrides,
  }
}

describe('lyricRefFor', () => {
  /**
   * Protects: the ref names the version whose words are actually being sent.
   *
   * `GenerationSpec` carries the text and the ref side by side and nothing
   * downstream reconciles them, so a ref naming v2 beside v3's words is a
   * sidecar that lies -- and T-311's bar is that a run reproduces from the
   * sidecar alone.
   */
  it('test_the_ref_is_attached_when_the_text_is_the_approved_version', () => {
    const ref = lyricRefFor(doc(), model, values({ lyrics: APPROVED }))

    expect(ref).toEqual({ doc_id: 'lyric-01', version: 2 })
  })

  /**
   * Protects: one changed character drops the ref.
   *
   * The vacuity trap here is real -- with an empty lyrics field and no approved
   * version, every assertion below passes with the comparison deleted. This
   * test and the one above are the pair that make the rule a rule: same
   * document, same approval, text differing by one word.
   */
  it('test_edited_text_carries_no_ref', () => {
    const edited = APPROVED.replace('approved', 'edited')

    expect(lyricRefFor(doc(), model, values({ lyrics: edited }))).toBeNull()
    expect(lyricRefFor(doc(), model, values({ lyrics: `${APPROVED} ` }))).toBeNull()
  })

  /** Protects: nothing approved, nothing claimed. */
  it('test_an_unapproved_document_carries_no_ref', () => {
    expect(lyricRefFor(doc({ approved: null }), model, values({ lyrics: APPROVED }))).toBeNull()
    expect(lyricRefFor(null, model, values({ lyrics: APPROVED }))).toBeNull()
  })

  /**
   * Protects: an approval pointing at a version that is not there claims
   * nothing. `approvedText` already returns null for it; this pins that the
   * ref follows the text rather than the number.
   */
  it('test_an_approval_with_no_version_behind_it_carries_no_ref', () => {
    const orphan = doc({ approved: 7 })

    expect(lyricRefFor(orphan, model, values({ lyrics: APPROVED }))).toBeNull()
  })
})

describe('approvedOffer', () => {
  it('test_an_approved_version_is_offered_by_number', () => {
    const offer = approvedOffer(doc(), model, values())

    expect(offer).toContain('v2')
    expect(offer).toContain('approved')
  })

  /** Protects: nothing is offered when the field already holds that text. */
  it('test_nothing_is_offered_once_the_text_is_already_there', () => {
    expect(approvedOffer(doc(), model, values({ lyrics: APPROVED }))).toBeNull()
  })

  it('test_nothing_is_offered_without_an_approval', () => {
    expect(approvedOffer(doc({ approved: null }), model, values())).toBeNull()
    expect(approvedOffer(null, model, values())).toBeNull()
    expect(approvedOffer(doc(), null, values())).toBeNull()
  })

  /** Protects: the fill names the profile's own lyrics input. */
  it('test_the_fill_targets_the_profiles_lyrics_control', () => {
    const fill = approvedFill(doc(), model)

    expect(fill).toEqual({ name: 'lyrics', text: APPROVED })
  })
})

describe('blockers', () => {
  it('test_a_ready_panel_has_no_blockers', () => {
    expect(blockers('ace-step-1.5-turbo', model, values({ seed: 42 }))).toEqual([])
  })

  /**
   * Protects: a seed JavaScript cannot hold never reaches Rust.
   *
   * Third layer of one rule -- `params.ts` refuses it, the text input keeps the
   * DOM from rounding it, and this is the layer that decides what is submitted.
   * `18446744073709551615` would arrive in Rust as `...616`, be generated with,
   * and be written into the provenance sidecar.
   */
  it('test_an_unusable_seed_blocks_generate', () => {
    const reasons = blockers('ace-step-1.5-turbo', model, values({ seed: '18446744073709551615' }))

    expect(reasons).toHaveLength(1)
    expect(reasons[0]).toContain('9007199254740991')
  })

  it('test_an_empty_seed_blocks_generate', () => {
    expect(blockers('ace-step-1.5-turbo', model, values({ seed: '' }))).toHaveLength(1)
  })

  /**
   * Protects: an unknown profile says which one.
   *
   * `effectiveProfileId` returns the configured id whether or not a profile
   * answers to it, so this is reachable by deleting a user profile from disk.
   */
  it('test_an_unknown_profile_blocks_generate_by_name', () => {
    const reasons = blockers('gone', null, {})

    expect(reasons).toHaveLength(1)
    expect(reasons[0]).toContain('gone')
  })

  /**
   * Protects: ComfyUI being disconnected is **not** a blocker.
   *
   * `generate_audio` calls `ensure_connected`, which starts comfy-mcp itself.
   * A button disabled on connection state would be dead on every cold start,
   * refusing a generation the backend would have handled -- and if ComfyUI is
   * genuinely down, the command's own error says so accurately. This test
   * exists so nobody "fixes" that by adding a connection argument.
   */
  it('test_connection_state_is_not_among_the_blockers', () => {
    expect(blockers.length).toBe(3)
    expect(blockers('ace-step-1.5-turbo', model, values({ seed: 1 }))).toEqual([])
  })
})

describe('specFor', () => {
  /** Protects: the spec is assembled from the two panels, not re-derived. */
  it('test_the_spec_carries_the_panels_values_and_the_stack', () => {
    const stack: StackRow[] = [
      { path: 'a\\adapter_model.safetensors', label: 'a', strength: 0.8, enabled: true },
      { path: 'b\\adapter_model.safetensors', label: 'b', strength: 1.2, enabled: false },
    ]

    const spec = specFor(
      'ace-step-1.5-turbo',
      model,
      values({ seed: 4242, tags: 'synthwave', lyrics: APPROVED }),
      stack,
      doc(),
    )

    expect(spec.profile_id).toBe('ace-step-1.5-turbo')
    expect(spec.inputs.seed).toEqual({ type: 'seed', value: 4242 })
    expect(spec.inputs.tags).toEqual({ type: 'text', value: 'synthwave' })
    expect(spec.lyrics).toEqual({ doc_id: 'lyric-01', version: 2 })
    expect(spec.loras).toHaveLength(2)
  })

  /**
   * Protects: a bypassed LoRA reaches the spec, disabled.
   *
   * Rust's `active_loras()` is what filters before the splice, and the sidecar
   * records the whole stack -- dropping it here would make a bypass look like a
   * delete in the record of how the track was made.
   */
  it('test_a_bypassed_lora_is_in_the_spec_and_marked_disabled', () => {
    const stack: StackRow[] = [
      { path: 'b\\adapter_model.safetensors', label: 'b', strength: 1.2, enabled: false },
    ]

    const spec = specFor('ace-step-1.5-turbo', model, values(), stack, null)

    expect(spec.loras).toEqual([{ file: 'b\\adapter_model.safetensors', strength: 1.2, enabled: false }])
  })
})

describe('submissionNotes', () => {
  it('test_a_plain_submission_just_says_it_is_queued', () => {
    expect(submissionNotes(submission())).toEqual([QUEUED])
  })

  /**
   * Protects: the two graph edits validation cannot vouch for are reported.
   *
   * A LoRA chain spliced in but feeding nothing validates clean, runs, and
   * writes a track with no LoRA on it (MCP-SURFACE 17.1). The count here is the
   * only thing on screen that says the splice happened at all.
   */
  it('test_the_graph_edits_are_reported', () => {
    const notes = submissionNotes(
      submission({ lora_nodes: ['200', '201'], output_format: 'flac' }),
    )

    expect(notes).toContain('2 LoRAs applied.')
    expect(notes).toContain('Saving flac.')
  })

  it('test_one_lora_is_not_pluralised', () => {
    expect(submissionNotes(submission({ lora_nodes: ['200'] }))).toContain('1 LoRA applied.')
  })

  /**
   * Protects: addresses the audit could not resolve are surfaced.
   *
   * MiniMax's seed lands here -- **unverified rather than known-working**
   * (MCP-SURFACE 18.5). Empty on ACE-Step, so the note appears only where there
   * is genuinely something the app could not check.
   */
  it('test_unchecked_slots_are_named_not_swallowed', () => {
    const notes = submissionNotes(submission({ unchecked_slots: ['12.seed', '12.duration'] }))
    const note = notes.find((line) => line.includes('could not be checked'))

    expect(note).toContain('12.seed')
    expect(note).toContain('12.duration')
    expect(note).toContain('may not reach the model')
  })

  /** Protects: a clean submission is not warned about. */
  it('test_nothing_is_said_about_slots_when_all_were_checked', () => {
    expect(submissionNotes(submission()).some((n) => n.includes('could not be checked'))).toBe(
      false,
    )
  })
})

describe('notesFor', () => {
  /**
   * Protects: a submission's notes do not outlive the settings that made them.
   *
   * The misleading case is concrete -- generate two LoRAs on ACE-Step, switch
   * to MiniMax, and `2 LoRAs applied.` sits under a model with no LoRA support
   * and no panel to show for it.
   */
  it('test_notes_are_dropped_once_the_profile_changes', () => {
    const queued = submission({ lora_nodes: ['200', '201'] })

    expect(notesFor(queued, 'ace-step-1.5-turbo', 'ace-step-1.5-turbo')).toContain(
      '2 LoRAs applied.',
    )
    expect(notesFor(queued, 'ace-step-1.5-turbo', 'minimax-music-3')).toEqual([])
  })

  it('test_nothing_queued_says_nothing', () => {
    expect(notesFor(null, null, 'ace-step-1.5-turbo')).toEqual([])
    expect(notesFor(submission(), null, 'ace-step-1.5-turbo')).toEqual([])
  })

  /** Protects: the button says something in both states. */
  it('test_the_button_has_a_label_either_way', () => {
    expect(GENERATE).not.toBe('')
    expect(QUEUEING).not.toBe('')
    expect(GENERATE).not.toBe(QUEUEING)
  })
})
