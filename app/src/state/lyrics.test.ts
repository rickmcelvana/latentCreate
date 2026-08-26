import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { LyricBrief } from '../bridge/lyrics'
import type { ProfileGuide } from '../bridge/profiles'
import type { LyricDoc, LintFinding } from '../bridge/lyricdoc'
import { useConfigStore } from './config'
import {
  applyLyricEvent,
  approvedText,
  generationPhase,
  nextVersionNumber,
  structureOptions,
  styleTagsFromGuide,
  thinkingTail,
  useLyricsStore,
  type LyricsSnapshot,
} from './lyrics'

const mockDefaultBrief: LyricBrief = vi.hoisted(() => ({
  theme: 'A night drive out of a city you are leaving for good',
  style_tags: 'synthwave, retro, 80s, dreamy, female vocal, driving beat',
  mood: 'bittersweet, hopeful',
  structure: 'V-C-V-C-B-C',
  language: 'English',
  point_of_view: 'first_person',
  era_refs: null,
  explicit_allowed: false,
  target_duration_s: 120,
}))

const mockGenerateLyrics = vi.fn()
const mockCancelLyrics = vi.fn()
const mockSubscribeLyrics = vi.fn()
const mockOpenLyricDoc = vi.fn()
const mockSaveLyricDoc = vi.fn()
const mockLintLyrics = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/lyrics', () => ({
  isTauri: () => mockIsTauri,
  generateLyrics: (brief: unknown, profileId: string) => mockGenerateLyrics(brief, profileId),
  cancelLyrics: () => mockCancelLyrics(),
  subscribeLyrics: (cb: (e: unknown) => void) => mockSubscribeLyrics(cb),
  DEFAULT_BRIEF: mockDefaultBrief,
}))

vi.mock('../bridge/lyricdoc', () => ({
  openLyricDoc: () => mockOpenLyricDoc(),
  saveLyricDoc: (doc: unknown) => mockSaveLyricDoc(doc),
  lintLyrics: (profileId: string, brief: unknown, text: string) =>
    mockLintLyrics(profileId, brief, text),
}))

function snapshot(over: Partial<LyricsSnapshot> = {}): LyricsSnapshot {
  return {
    draft: '',
    thinking: [],
    truncated: false,
    generating: false,
    error: null,
    ...over,
  }
}

function profileGuide(over: Partial<ProfileGuide> = {}): ProfileGuide {
  return {
    display_name: 'ACE-Step 1.5 XL Turbo',
    tag_style: 'comma-separated short tags',
    examples: [
      { tags: 'synthwave, retro, 80s, dreamy, female vocal, driving beat, 105 bpm', lyrics: null },
    ],
    ...over,
  }
}

function lyricDoc(over: Partial<LyricDoc> = {}): LyricDoc {
  return {
    id: 'ld-0001',
    title: null,
    versions: [
      { number: 1, text: 'first draft', created_at: 't1', source: { kind: 'human' } },
    ],
    approved: null,
    ...over,
  }
}

beforeEach(() => {
  mockIsTauri = true
  mockGenerateLyrics.mockReset()
  mockCancelLyrics.mockReset()
  mockSubscribeLyrics.mockReset()
  mockOpenLyricDoc.mockReset()
  mockSaveLyricDoc.mockReset()
  mockLintLyrics.mockReset()
  useLyricsStore.setState({
    brief: mockDefaultBrief,
    draft: '',
    thinking: [],
    truncated: false,
    generating: false,
    error: null,
    listening: false,
    doc: null,
    findings: [],
  })
})

describe('lyrics store', () => {
  it('test_generate_resets_and_submits_the_brief', async () => {
    mockGenerateLyrics.mockResolvedValue(undefined)
    useLyricsStore.setState({ draft: 'old', generating: false })

    await useLyricsStore.getState().generate('ace-step-1.5-turbo')

    expect(mockGenerateLyrics).toHaveBeenCalledWith(mockDefaultBrief, 'ace-step-1.5-turbo')
    const state = useLyricsStore.getState()
    expect(state.draft).toBe('')
    expect(state.generating).toBe(true)
    expect(state.truncated).toBe(false)
  })

  it('test_generate_is_skipped_outside_tauri', async () => {
    mockIsTauri = false
    await useLyricsStore.getState().generate('ace-step-1.5-turbo')
    expect(mockGenerateLyrics).not.toHaveBeenCalled()
  })

  /**
   * Protects: a rejected command must not leave the store stuck "generating".
   * The backend returns an error for "no LLM configured", and the UI has to be
   * able to show it and let the user retry.
   */
  it('test_generate_recovers_when_the_backend_rejects', async () => {
    mockGenerateLyrics.mockRejectedValue(new Error('no lyric LLM configured'))
    await useLyricsStore.getState().generate('ace-step-1.5-turbo')
    const state = useLyricsStore.getState()
    expect(state.generating).toBe(false)
    expect(state.error).toBe('Error: no lyric LLM configured')
  })

  it('test_cancel_calls_the_backend', async () => {
    mockCancelLyrics.mockResolvedValue(undefined)
    await useLyricsStore.getState().cancel()
    expect(mockCancelLyrics).toHaveBeenCalledTimes(1)
  })

  it('test_start_listening_subscribes_once', async () => {
    mockSubscribeLyrics.mockResolvedValue(() => {})
    await useLyricsStore.getState().startListening()
    await useLyricsStore.getState().startListening()
    expect(mockSubscribeLyrics).toHaveBeenCalledTimes(1)
  })

  it('test_start_listening_is_skipped_outside_tauri', async () => {
    mockIsTauri = false
    await useLyricsStore.getState().startListening()
    expect(mockSubscribeLyrics).not.toHaveBeenCalled()
  })

  it('test_set_brief_merges_over_the_prefills', () => {
    useLyricsStore.getState().setBrief({ mood: 'somber' })
    const brief = useLyricsStore.getState().brief
    expect(brief.mood).toBe('somber')
    expect(brief.structure).toBe('V-C-V-C-B-C')
  })
})

describe('applyLyricEvent', () => {
  it('test_folds_delta_thinking_done_failed', () => {
    const deltas = applyLyricEvent(snapshot(), {
      kind: 'delta',
      payload: { text: 'line one\n' },
    })
    expect(deltas.draft).toBe('line one\n')

    const thinking = applyLyricEvent(deltas, {
      kind: 'thinking',
      payload: { text: 'weighing it up' },
    })
    expect(thinking.thinking).toEqual(['weighing it up'])

    const done = applyLyricEvent(thinking, {
      kind: 'done',
      payload: { finish_reason: 'length', usage: null },
    })
    expect(done.truncated).toBe(true)
    expect(done.generating).toBe(false)
    expect(done.draft).toBe('line one\n')

    const failed = applyLyricEvent(snapshot({ draft: 'partial' }), {
      kind: 'failed',
      payload: { error: 'boom' },
    })
    expect(failed.error).toBe('boom')
    expect(failed.generating).toBe(false)
    expect(failed.draft).toBe('partial')
  })

  /**
   * Protects: `finish_reason` is read as the `truncated` flag, and only
   * `length` sets it. A clean `stop` is not a truncation, and the distinction
   * is what decides whether the UI offers a retry with more budget.
   */
  it('test_truncated_is_set_only_on_a_length_finish', () => {
    const stop = applyLyricEvent(snapshot({ generating: true }), {
      kind: 'done',
      payload: { finish_reason: 'stop', usage: null },
    })
    expect(stop.truncated).toBe(false)

    const length = applyLyricEvent(snapshot({ generating: true }), {
      kind: 'done',
      payload: { finish_reason: 'length', usage: null },
    })
    expect(length.truncated).toBe(true)
  })

  /**
   * Protects: the reasoning trace is bounded. A model can emit thousands of
   * characters of chain-of-thought (LLM-SURFACE 12.1), and storing all of it
   * would grow the store without bound for a status line that only shows the
   * most recent thinking.
   */
  it('test_thinking_trace_is_bounded', () => {
    let state = snapshot()
    for (let i = 0; i < 120; i += 1) {
      state = applyLyricEvent(state, { kind: 'thinking', payload: { text: `t${i}` } })
    }
    expect(state.thinking.length).toBe(50)
    expect(state.thinking[0]).toBe('t70')
    expect(state.thinking[49]).toBe('t119')
  })
})

describe('generationPhase', () => {
  it('test_idle_when_not_generating', () => {
    expect(generationPhase(snapshot())).toBe('idle')
  })

  /**
   * Protects the reasoning status. A model can spend tens of seconds thinking
   * before writing a word (LLM-SURFACE 12.1), and "thinking" is the only proof
   * of life in that window -- collapsing it into "starting" would read as a
   * hang.
   */
  it('test_starting_then_thinking_then_writing', () => {
    expect(generationPhase(snapshot({ generating: true }))).toBe('starting')
    expect(generationPhase(snapshot({ generating: true, thinking: ['hmm'] }))).toBe('thinking')
    expect(generationPhase(snapshot({ generating: true, draft: 'line' }))).toBe('writing')
  })

  it('test_failed_when_error_and_not_generating', () => {
    expect(generationPhase(snapshot({ error: 'boom' }))).toBe('failed')
  })

  it('test_writing_beats_thinking_once_content_flows', () => {
    expect(
      generationPhase(snapshot({ generating: true, thinking: ['hmm'], draft: 'x' })),
    ).toBe('writing')
  })
})

describe('thinkingTail', () => {
  it('test_joins_the_recent_thinking', () => {
    expect(thinkingTail([' one', ' two'])).toBe(' one two')
  })

  /** Protects: the status shows the newest reasoning, not the whole chain. */
  it('test_truncates_to_the_most_recent', () => {
    const tail = thinkingTail(['a'.repeat(200)])
    expect(tail).toBe('a'.repeat(140))
  })
})

describe('structureOptions', () => {
  /** Protects: a custom structure stays selectable rather than being dropped. */
  it('test_appends_a_custom_value_to_the_presets', () => {
    const options = structureOptions('V-Spoken word-C')
    expect(options).toContain('V-Spoken word-C')
    expect(options[options.length - 1]).toBe('V-Spoken word-C')
  })

  it('test_does_not_duplicate_a_preset', () => {
    const options = structureOptions('V-C-V-C-B-C')
    expect(options.filter((o) => o === 'V-C-V-C-B-C')).toHaveLength(1)
  })
})

describe('styleTagsFromGuide', () => {
  /** Protects: the prefill is the profile's own example, not a constant. */
  it('test_reads_the_first_example', () => {
    expect(
      styleTagsFromGuide(profileGuide({ examples: [{ tags: 'synthwave, dreamy', lyrics: null }] })),
    ).toBe('synthwave, dreamy')
  })

  /** Protects: no guide and an empty example both mean "nothing to prefill". */
  it('test_is_null_without_an_example', () => {
    expect(styleTagsFromGuide(null)).toBeNull()
    expect(styleTagsFromGuide(profileGuide({ examples: [] }))).toBeNull()
    expect(styleTagsFromGuide(profileGuide({ examples: [{ tags: '  ', lyrics: null }] }))).toBeNull()
  })
})

describe('prefillFrom', () => {
  /**
   * Protects the "never modify the user's words" rule: the prefill replaces only
   * the untouched default, so an edit the user already made is left alone.
   */
  it('test_prefills_over_the_default_but_not_over_an_edit', () => {
    useLyricsStore.getState().prefillFrom(profileGuide())
    expect(useLyricsStore.getState().brief.style_tags).toBe(
      'synthwave, retro, 80s, dreamy, female vocal, driving beat, 105 bpm',
    )

    useLyricsStore.setState({ brief: { ...mockDefaultBrief, style_tags: 'my own words' } })
    useLyricsStore.getState().prefillFrom(profileGuide())
    expect(useLyricsStore.getState().brief.style_tags).toBe('my own words')
  })

  /** Protects: a null guide changes nothing. */
  it('test_a_null_guide_changes_nothing', () => {
    useLyricsStore.getState().prefillFrom(null)
    expect(useLyricsStore.getState().brief.style_tags).toBe(mockDefaultBrief.style_tags)
  })
})

describe('nextVersionNumber', () => {
  it('test_starts_at_one', () => {
    expect(nextVersionNumber([])).toBe(1)
  })

  /** Protects: numbers continue from the highest, so a restore never collides. */
  it('test_continues_from_the_highest_number', () => {
    expect(
      nextVersionNumber([
        { number: 1, text: 'a', created_at: '', source: { kind: 'human' } },
        { number: 5, text: 'e', created_at: '', source: { kind: 'human' } },
      ]),
    ).toBe(6)
  })
})

describe('approvedText', () => {
  /** Protects: the handoff is the approved version's text, nothing else. */
  it('test_returns_the_approved_versions_text', () => {
    expect(approvedText(lyricDoc({ approved: 2, versions: [
      { number: 1, text: 'first', created_at: '', source: { kind: 'human' } },
      { number: 2, text: 'second', created_at: '', source: { kind: 'human' } },
    ] }))).toBe('second')
    expect(approvedText(lyricDoc())).toBeNull()
    expect(approvedText(null)).toBeNull()
  })
})

describe('versioned document store', () => {
  const openDoc = lyricDoc()

  it('test_load_doc_restores_the_latest_version_as_draft', async () => {
    mockOpenLyricDoc.mockResolvedValue(openDoc)
    await useLyricsStore.getState().loadDoc()
    expect(useLyricsStore.getState().doc?.id).toBe('ld-0001')
    expect(useLyricsStore.getState().draft).toBe('first draft')
  })

  it('test_commit_pushes_the_draft_and_saves', async () => {
    useLyricsStore.setState({ doc: openDoc, draft: 'edited' })
    await useLyricsStore.getState().commit({ kind: 'edited', from_version: 1 })

    const doc = useLyricsStore.getState().doc
    expect(doc?.versions.length).toBe(2)
    expect(doc?.versions[1]).toMatchObject({
      number: 2,
      text: 'edited',
      source: { kind: 'edited', from_version: 1 },
    })
    expect(mockSaveLyricDoc).toHaveBeenCalledWith(doc)
  })

  it('test_commit_is_a_noop_without_a_doc', async () => {
    await useLyricsStore.getState().commit({ kind: 'human' })
    expect(mockSaveLyricDoc).not.toHaveBeenCalled()
  })

  it('test_commit_generated_records_the_model_from_config', async () => {
    useConfigStore.setState({
      config: {
        schema_version: 1,
        comfy: { mode: 'local', url: null, comfy_bin: null },
        llm: { provider: 'open_ai_compat', base_url: null, model: 'gemma4:12b-32k' },
        default_profile_id: null,
      },
    })
    useLyricsStore.setState({ doc: { ...openDoc, versions: [] }, draft: 'generated' })
    await useLyricsStore.getState().commitGenerated()

    const doc = useLyricsStore.getState().doc
    expect(doc?.versions[0]?.source).toEqual({
      kind: 'llm',
      model: 'gemma4:12b-32k',
      prompt_optimized: false,
    })
  })

  it('test_approve_sets_and_saves_the_approved_version', async () => {
    useLyricsStore.setState({ doc: openDoc })
    await useLyricsStore.getState().approve(1)
    expect(useLyricsStore.getState().doc?.approved).toBe(1)
    expect(mockSaveLyricDoc).toHaveBeenCalled()
  })

  it('test_restore_sets_the_draft_from_a_version', () => {
    useLyricsStore.setState({ doc: openDoc, draft: 'unrelated' })
    useLyricsStore.getState().restore(1)
    expect(useLyricsStore.getState().draft).toBe('first draft')
  })

  it('test_lint_sets_the_findings', async () => {
    const findings: LintFinding[] = [{ kind: 'no_structure_tags' }]
    mockLintLyrics.mockResolvedValue(findings)
    await useLyricsStore.getState().lint('ace-step-1.5-turbo')
    expect(useLyricsStore.getState().findings).toEqual(findings)
  })

  /**
   * Protects the source choice: typing into an empty document is `human`, and
   * editing an existing version is `edited` from the latest version.
   */
  it('test_save_draft_commits_human_without_versions_and_edited_with', async () => {
    useLyricsStore.setState({ doc: { ...openDoc, versions: [] }, draft: 'typed' })
    await useLyricsStore.getState().saveDraft()
    expect(useLyricsStore.getState().doc?.versions[0]?.source).toEqual({ kind: 'human' })

    useLyricsStore.setState({ doc: openDoc, draft: 'revised' })
    await useLyricsStore.getState().saveDraft()
    expect(useLyricsStore.getState().doc?.versions[1]?.source).toEqual({
      kind: 'edited',
      from_version: 1,
    })
  })
})
