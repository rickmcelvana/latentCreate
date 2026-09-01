import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { LyricBrief, PromptOptimization } from '../bridge/lyrics'
import type { ProfileGuide } from '../bridge/profiles'
import type { LyricDoc, LintFinding } from '../bridge/lyricdoc'
import { useConfigStore } from './config'
import {
  applyLyricEvent,
  approvedLabel,
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
const mockOptimizePrompt = vi.fn()
const mockCancelLyrics = vi.fn()
const mockSubscribeLyrics = vi.fn()
const mockOpenLyricDoc = vi.fn()
const mockSaveLyricDoc = vi.fn()
const mockListLyricDocs = vi.fn()
const mockCreateLyricDoc = vi.fn()
const mockDeleteLyricDoc = vi.fn()
const mockDeleteLyricVersion = vi.fn()
const mockLintLyrics = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/lyrics', () => ({
  isTauri: () => mockIsTauri,
  generateLyrics: (brief: unknown, profileId: string, promptOverride: string | null) =>
    mockGenerateLyrics(brief, profileId, promptOverride),
  optimizePrompt: (brief: unknown, profileId: string) => mockOptimizePrompt(brief, profileId),
  cancelLyrics: () => mockCancelLyrics(),
  subscribeLyrics: (cb: (e: unknown) => void) => mockSubscribeLyrics(cb),
  DEFAULT_BRIEF: mockDefaultBrief,
}))

vi.mock('../bridge/lyricdoc', () => ({
  openLyricDoc: (id?: string) => mockOpenLyricDoc(id),
  saveLyricDoc: (doc: unknown) => mockSaveLyricDoc(doc),
  listLyricDocs: () => mockListLyricDocs(),
  createLyricDoc: (title?: string) => mockCreateLyricDoc(title),
  deleteLyricDoc: (docId: string) => mockDeleteLyricDoc(docId),
  deleteLyricVersion: (docId: string, number: number) => mockDeleteLyricVersion(docId, number),
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

function optimization(over: Partial<PromptOptimization> = {}): PromptOptimization {
  return {
    original: 'Theme: A night drive out of a city you are leaving for good',
    optimized: 'Theme: A rain-slick night drive out of a coastal city you are leaving for good',
    truncated: false,
    ...over,
  }
}

beforeEach(() => {
  mockIsTauri = true
  mockGenerateLyrics.mockReset()
  mockOptimizePrompt.mockReset()
  mockCancelLyrics.mockReset()
  mockSubscribeLyrics.mockReset()
  mockOpenLyricDoc.mockReset()
  mockSaveLyricDoc.mockReset()
  mockListLyricDocs.mockReset()
  mockCreateLyricDoc.mockReset()
  mockDeleteLyricDoc.mockReset()
  mockDeleteLyricVersion.mockReset()
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
    docs: [],
    selectedDocId: null,
    confirmingDocDelete: false,
    deleteDocError: null,
    findings: [],
    linted: false,
    confirmingVersion: null,
    deleteError: null,
    optimization: null,
    proposed: '',
    optimizing: false,
    promptOverride: null,
  })
})

describe('lyrics store', () => {
  it('test_generate_resets_and_submits_the_brief', async () => {
    mockGenerateLyrics.mockResolvedValue(undefined)
    useLyricsStore.setState({ draft: 'old', generating: false })

    await useLyricsStore.getState().generate('ace-step-1.5-turbo')

    expect(mockGenerateLyrics).toHaveBeenCalledWith(mockDefaultBrief, 'ace-step-1.5-turbo', null)
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

  it('test_ask_and_cancel_delete_version_toggle_the_marker', () => {
    // A stale refusal from a prior attempt is cleared when a new confirm arms.
    useLyricsStore.setState({
      deleteError: { version: 5, message: 'old' },
    })
    useLyricsStore.getState().askDeleteVersion(2)
    expect(useLyricsStore.getState().confirmingVersion).toBe(2)
    expect(useLyricsStore.getState().deleteError).toBeNull()
    useLyricsStore.getState().cancelDeleteVersion()
    expect(useLyricsStore.getState().confirmingVersion).toBeNull()
  })

  /**
   * Protects the happy path (T-405b lesson): a successful delete replaces `doc`
   * with the backend's result -- never a local edit -- and clears the confirm.
   * Asserting the call arguments guards the dropped-argument mutation (T-404b).
   */
  it('test_delete_version_replaces_the_doc_with_the_backend_result', async () => {
    const before = lyricDoc({
      versions: [
        { number: 1, text: 'one', created_at: 't1', source: { kind: 'human' } },
        { number: 2, text: 'two', created_at: 't2', source: { kind: 'human' } },
      ],
    })
    const after = lyricDoc({
      versions: [{ number: 1, text: 'one', created_at: 't1', source: { kind: 'human' } }],
    })
    mockDeleteLyricVersion.mockResolvedValue(after)
    useLyricsStore.setState({ doc: before, confirmingVersion: 2 })

    const ok = await useLyricsStore.getState().deleteVersion(2)

    expect(ok).toBe(true)
    expect(mockDeleteLyricVersion).toHaveBeenCalledWith('ld-0001', 2)
    const state = useLyricsStore.getState()
    expect(state.doc).toEqual(after)
    expect(state.confirmingVersion).toBeNull()
    expect(state.deleteError).toBeNull()
  })

  /**
   * Protects: a refusal records its message (which names the blocking tracks)
   * and leaves the document unchanged -- the delete must not appear to succeed.
   */
  it('test_delete_version_refusal_keeps_the_doc_and_shows_the_message', async () => {
    const doc = lyricDoc({
      versions: [{ number: 1, text: 'one', created_at: 't1', source: { kind: 'human' } }],
    })
    mockDeleteLyricVersion.mockRejectedValue(
      'version 1 of ld-0001 is still used by 1 track(s): tr-0007',
    )
    useLyricsStore.setState({ doc, confirmingVersion: 1 })

    const ok = await useLyricsStore.getState().deleteVersion(1)

    expect(ok).toBe(false)
    const state = useLyricsStore.getState()
    expect(state.doc).toEqual(doc)
    // The error is keyed to the version, so the view can render it at that row.
    expect(state.deleteError).toEqual({
      version: 1,
      message: 'version 1 of ld-0001 is still used by 1 track(s): tr-0007',
    })
    expect(state.confirmingVersion).toBeNull()
  })

  it('test_delete_version_is_a_noop_without_a_doc', async () => {
    useLyricsStore.setState({ doc: null })
    const ok = await useLyricsStore.getState().deleteVersion(1)
    expect(ok).toBe(false)
    expect(mockDeleteLyricVersion).not.toHaveBeenCalled()
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

describe('approvedLabel', () => {
  /**
   * Protects the fact the user is shown. The notice was reported missing twice
   * while rendering correctly, so the logic behind it is pinned here rather
   * than derived inline in a view no test can see.
   */
  it('test_approved_label_names_the_version_and_is_null_without_one', () => {
    const doc = lyricDoc({
      approved: 2,
      versions: [
        { number: 1, text: 'first', created_at: 't1', source: { kind: 'human' } },
        { number: 2, text: 'second', created_at: 't2', source: { kind: 'human' } },
      ],
    })
    expect(approvedLabel(doc)).toBe('v2 approved')
    expect(approvedLabel(lyricDoc())).toBeNull()
    expect(approvedLabel(null)).toBeNull()
  })

  /** Protects: an approved number with no version behind it announces nothing. */
  it('test_approved_label_is_null_when_the_version_is_gone', () => {
    expect(approvedLabel(lyricDoc({ approved: 9 }))).toBeNull()
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

  it('test_load_docs_opens_the_selected_document_and_restores_its_draft', async () => {
    mockListLyricDocs.mockResolvedValue({ docs: [openDoc], warnings: [] })
    mockOpenLyricDoc.mockResolvedValue(openDoc)
    await useLyricsStore.getState().loadDocs()
    const state = useLyricsStore.getState()
    expect(state.docs.map((d) => d.id)).toEqual(['ld-0001'])
    expect(state.selectedDocId).toBe('ld-0001')
    expect(state.doc?.id).toBe('ld-0001')
    expect(state.draft).toBe('first draft')
  })

  it('test_load_docs_creates_one_when_the_project_has_none', async () => {
    mockListLyricDocs.mockResolvedValue({ docs: [], warnings: [] })
    mockCreateLyricDoc.mockResolvedValue(openDoc)
    mockOpenLyricDoc.mockResolvedValue(openDoc)
    await useLyricsStore.getState().loadDocs()
    expect(mockCreateLyricDoc).toHaveBeenCalledTimes(1)
    expect(useLyricsStore.getState().selectedDocId).toBe('ld-0001')
  })

  /** Protects: switching documents resets the draft, so an unsaved edit in one
   * document never bleeds into another (a fresh open reads from disk). */
  it('test_select_doc_swaps_the_draft_to_the_opened_document', async () => {
    const other = lyricDoc({
      id: 'ld-0002',
      versions: [{ number: 1, text: 'other song', created_at: 't', source: { kind: 'human' } }],
    })
    useLyricsStore.setState({ docs: [openDoc, other], selectedDocId: 'ld-0001', draft: 'unsaved' })
    mockOpenLyricDoc.mockResolvedValue(other)

    await useLyricsStore.getState().selectDoc('ld-0002')

    expect(mockOpenLyricDoc).toHaveBeenCalledWith('ld-0002')
    const state = useLyricsStore.getState()
    expect(state.selectedDocId).toBe('ld-0002')
    expect(state.doc?.id).toBe('ld-0002')
    expect(state.draft).toBe('other song')
  })

  it('test_create_doc_appends_and_selects_the_new_document', async () => {
    const created = lyricDoc({ id: 'ld-0002', versions: [] })
    useLyricsStore.setState({ docs: [openDoc], selectedDocId: 'ld-0001' })
    mockCreateLyricDoc.mockResolvedValue(created)
    mockOpenLyricDoc.mockResolvedValue(created)

    await useLyricsStore.getState().createDoc()

    const state = useLyricsStore.getState()
    expect(state.docs.map((d) => d.id)).toEqual(['ld-0001', 'ld-0002'])
    expect(state.selectedDocId).toBe('ld-0002')
    expect(state.draft).toBe('')
  })

  it('test_ask_and_cancel_delete_doc_toggle_the_marker', () => {
    useLyricsStore.setState({ deleteDocError: 'old' })
    useLyricsStore.getState().askDeleteDoc()
    expect(useLyricsStore.getState().confirmingDocDelete).toBe(true)
    expect(useLyricsStore.getState().deleteDocError).toBeNull()
    useLyricsStore.getState().cancelDeleteDoc()
    expect(useLyricsStore.getState().confirmingDocDelete).toBe(false)
  })

  /** Protects the happy path (T-405b lesson): deleting the open document lands
   * on a remaining one, and the list is the backend's returned remainder. */
  it('test_delete_doc_replaces_the_list_and_reselects', async () => {
    const kept = lyricDoc({ id: 'ld-0002' })
    useLyricsStore.setState({ docs: [openDoc, kept], selectedDocId: 'ld-0001' })
    mockDeleteLyricDoc.mockResolvedValue({ docs: [kept], warnings: [] })
    mockOpenLyricDoc.mockResolvedValue(kept)

    const ok = await useLyricsStore.getState().deleteDoc('ld-0001')

    expect(ok).toBe(true)
    expect(mockDeleteLyricDoc).toHaveBeenCalledWith('ld-0001')
    const state = useLyricsStore.getState()
    expect(state.docs.map((d) => d.id)).toEqual(['ld-0002'])
    expect(state.selectedDocId).toBe('ld-0002')
    expect(state.confirmingDocDelete).toBe(false)
    expect(state.deleteDocError).toBeNull()
  })

  /** Protects: a refusal keeps the list and records the message, not a delete
   * that appears to succeed. */
  it('test_delete_doc_refusal_keeps_the_list_and_shows_the_message', async () => {
    useLyricsStore.setState({ docs: [openDoc], selectedDocId: 'ld-0001' })
    mockDeleteLyricDoc.mockRejectedValue('ld-0001 is still used by 1 track(s): tr-0007')

    const ok = await useLyricsStore.getState().deleteDoc('ld-0001')

    expect(ok).toBe(false)
    const state = useLyricsStore.getState()
    expect(state.docs.map((d) => d.id)).toEqual(['ld-0001'])
    expect(state.deleteDocError).toBe('ld-0001 is still used by 1 track(s): tr-0007')
    expect(state.confirmingDocDelete).toBe(false)
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
        llm: {
          provider: 'open_ai_compat',
          base_url: null,
          model: 'gemma4:12b-32k',
          accepts_reasoning_effort: null,
        },
        default_profile_id: null,
        default_project_slug: null,
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
   * Protects: a clean check is distinguishable from no check. Empty findings
   * alone cannot say which, and the UI rendered nothing for both -- leaving a
   * user who checked a clean lyric unable to tell the button had fired.
   */
  it('test_lint_records_that_it_ran_even_with_nothing_to_report', async () => {
    mockLintLyrics.mockResolvedValue([])
    expect(useLyricsStore.getState().linted).toBe(false)

    await useLyricsStore.getState().lint('ace-step-1.5-turbo')

    expect(useLyricsStore.getState().findings).toEqual([])
    expect(useLyricsStore.getState().linted).toBe(true)
  })

  /**
   * Protects: findings never outlive the text they describe. They carry line
   * numbers, so a stale finding points at a line the user has since changed --
   * worse than showing nothing.
   */
  it('test_changing_the_draft_clears_a_stale_check', async () => {
    mockLintLyrics.mockResolvedValue([{ kind: 'no_structure_tags' }] as LintFinding[])
    await useLyricsStore.getState().lint('ace-step-1.5-turbo')
    expect(useLyricsStore.getState().linted).toBe(true)

    useLyricsStore.getState().setDraft('rewritten by hand')
    expect(useLyricsStore.getState().findings).toEqual([])
    expect(useLyricsStore.getState().linted).toBe(false)

    // Restoring an older version replaces the draft too, and must clear it.
    await useLyricsStore.getState().lint('ace-step-1.5-turbo')
    useLyricsStore.setState({ doc: openDoc })
    useLyricsStore.getState().restore(1)
    expect(useLyricsStore.getState().linted).toBe(false)
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
  /**
   * Protects the consent gate at its narrowest point: an optimizer that has
   * run but not been accepted must change nothing about the request. The
   * failure mode this guards is the one ARCHITECTURE 6 forbids outright --
   * silently generating from a rewrite the user never approved.
   */
  it('test_a_reviewed_but_unaccepted_rewrite_is_not_sent', async () => {
    mockOptimizePrompt.mockResolvedValue(optimization())
    mockGenerateLyrics.mockResolvedValue(undefined)

    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')
    expect(useLyricsStore.getState().promptOverride).toBeNull()

    await useLyricsStore.getState().generate('ace-step-1.5-turbo')
    expect(mockGenerateLyrics).toHaveBeenCalledWith(mockDefaultBrief, 'ace-step-1.5-turbo', null)
  })

  /**
   * Protects: Accept is what sends the rewrite, and it sends the text as the
   * user last edited it -- not the model's original proposal. Editing then
   * accepting is the common case; sending the unedited version would discard
   * the user's own words.
   */
  it('test_accept_sends_the_edited_proposal', async () => {
    mockOptimizePrompt.mockResolvedValue(optimization())
    mockGenerateLyrics.mockResolvedValue(undefined)

    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')
    useLyricsStore.getState().setProposed('Theme: a night drive I edited myself')
    useLyricsStore.getState().acceptOptimized()

    const state = useLyricsStore.getState()
    expect(state.promptOverride).toBe('Theme: a night drive I edited myself')
    expect(state.optimization).toBeNull()

    await useLyricsStore.getState().generate('ace-step-1.5-turbo')
    expect(mockGenerateLyrics).toHaveBeenCalledWith(
      mockDefaultBrief,
      'ace-step-1.5-turbo',
      'Theme: a night drive I edited myself',
    )
  })

  /** Protects: a blank proposal is not something a user can have consented to. */
  it('test_accept_ignores_a_blank_proposal', async () => {
    mockOptimizePrompt.mockResolvedValue(optimization())
    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')
    useLyricsStore.getState().setProposed('   \n ')
    useLyricsStore.getState().acceptOptimized()

    expect(useLyricsStore.getState().promptOverride).toBeNull()
    expect(useLyricsStore.getState().optimization).not.toBeNull()
  })

  /** Protects: Revert puts the user back on their own brief, proposal and all. */
  it('test_revert_clears_the_proposal_and_the_accepted_prompt', async () => {
    mockOptimizePrompt.mockResolvedValue(optimization())
    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')
    useLyricsStore.getState().acceptOptimized()
    useLyricsStore.getState().revertOptimized()

    const state = useLyricsStore.getState()
    expect(state.promptOverride).toBeNull()
    expect(state.optimization).toBeNull()
    expect(state.proposed).toBe('')
  })

  /**
   * Protects: an accepted prompt goes stale the moment the brief changes. It
   * was written against the old brief, so keeping it would leave the form
   * describing one song while Generate sends another -- and the form is what
   * the user believes they are sending.
   */
  it('test_editing_the_brief_drops_an_accepted_prompt', async () => {
    mockOptimizePrompt.mockResolvedValue(optimization())
    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')
    useLyricsStore.getState().acceptOptimized()

    useLyricsStore.getState().setBrief({ target_duration_s: 180 })

    expect(useLyricsStore.getState().promptOverride).toBeNull()
    expect(useLyricsStore.getState().brief.target_duration_s).toBe(180)
  })

  /**
   * Protects the provenance flag: it records whether an optimized prompt was
   * actually sent, not whether the optimizer ran. A reverted rewrite never
   * reached the model, and a sidecar claiming otherwise is a false record of
   * how the lyric was made.
   */
  it('test_prompt_optimized_records_the_accepted_prompt_not_the_optimizer_run', async () => {
    mockOptimizePrompt.mockResolvedValue(optimization())
    useLyricsStore.setState({ doc: { ...openDoc, versions: [] }, draft: 'generated' })

    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')
    useLyricsStore.getState().revertOptimized()
    await useLyricsStore.getState().commitGenerated()
    expect(useLyricsStore.getState().doc?.versions[0]?.source).toMatchObject({
      prompt_optimized: false,
    })

    useLyricsStore.setState({ doc: { ...openDoc, versions: [] }, draft: 'generated' })
    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')
    useLyricsStore.getState().acceptOptimized()
    await useLyricsStore.getState().commitGenerated()
    expect(useLyricsStore.getState().doc?.versions[0]?.source).toMatchObject({
      prompt_optimized: true,
    })
  })

  /** Protects: a failed optimizer call surfaces and leaves nothing half-set. */
  it('test_a_failed_optimize_reports_and_proposes_nothing', async () => {
    mockOptimizePrompt.mockRejectedValue(new Error('no lyric LLM configured'))
    await useLyricsStore.getState().optimize('ace-step-1.5-turbo')

    const state = useLyricsStore.getState()
    expect(state.optimizing).toBe(false)
    expect(state.optimization).toBeNull()
    expect(state.error).toContain('no lyric LLM configured')
  })
})
