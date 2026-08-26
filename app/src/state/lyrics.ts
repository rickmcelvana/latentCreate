import { create } from 'zustand'
import {
  cancelLyrics,
  generateLyrics,
  isTauri,
  subscribeLyrics,
  DEFAULT_BRIEF,
  type LyricBrief,
  type LyricEvent,
} from '../bridge/lyrics'
import type { ProfileGuide } from '../bridge/profiles'
import {
  lintLyrics,
  openLyricDoc,
  saveLyricDoc,
  type LintFinding,
  type LyricDoc,
  type LyricSource,
  type LyricVersion,
} from '../bridge/lyricdoc'
import { useConfigStore } from './config'

/**
 * Cap on the reasoning trace, so a model that reasons at length cannot grow the
 * store without bound. Only the most recent entries are kept -- the status UI
 * shows "still thinking", not the whole chain.
 */
const MAX_THINKING = 50

/**
 * The streaming state the lyric events fold into.
 *
 * Kept as its own shape so [`applyLyricEvent`] can be tested without a store or
 * a Tauri bridge.
 */
export interface LyricsSnapshot {
  /** The accumulated content -- the lyric itself. */
  draft: string
  /** The most recent reasoning deltas, for a status trace. */
  thinking: string[]
  /** True when the model stopped with `finish_reason: "length"`. */
  truncated: boolean
  generating: boolean
  error: string | null
}

/**
 * The structure strings offered in the picker.
 *
 * Letters expand per `create-core::lyrics::expand_structure`: V = Verse,
 * C = Chorus, B = Bridge, I = Intro, O = Outro. A user can still type a custom
 * structure -- the picker keeps whatever is not a preset selectable.
 */
export const STRUCTURE_PRESETS: readonly string[] = [
  'V-C-V-C-B-C',
  'V-C-V-C',
  'I-V-C-V-C-O',
  'V-V-C-V-C-B-C',
]

/**
 * The options for the structure picker: the presets, plus the current value
 * when it is not one of them, so a custom structure is never hidden.
 */
export function structureOptions(current: string): string[] {
  const options = [...STRUCTURE_PRESETS]
  if (!options.includes(current)) options.push(current)
  return options
}

/**
 * The style tags a profile's guide prefills, when its first example names any.
 *
 * `null` means "nothing to prefill": no guide, no examples, or an empty first
 * example. The form then keeps the built-in default.
 */
export function styleTagsFromGuide(guide: ProfileGuide | null): string | null {
  const first = guide?.examples[0]
  if (first === undefined || first.tags.trim() === '') return null
  return first.tags
}

/**
 * Fold one lyric event into the streaming snapshot.
 *
 * Pure so it can be tested without a store or a Tauri bridge. `Content` goes
 * into the draft, `Reasoning` into a bounded status trace, and `done`/`failed`
 * are terminal. `finish_reason: "length"` sets `truncated` -- the signal the UI
 * uses to offer a retry with more budget, so it must reach the store intact
 * rather than be swallowed as an error (LLM-SURFACE 12.1).
 */
export function applyLyricEvent(snapshot: LyricsSnapshot, event: LyricEvent): LyricsSnapshot {
  switch (event.kind) {
    case 'delta':
      return { ...snapshot, draft: snapshot.draft + event.payload.text }
    case 'thinking':
      return {
        ...snapshot,
        thinking: [...snapshot.thinking, event.payload.text].slice(-MAX_THINKING),
      }
    case 'done':
      return {
        ...snapshot,
        truncated: event.payload.finish_reason === 'length',
        generating: false,
      }
    case 'failed':
      return { ...snapshot, error: event.payload.error, generating: false }
  }
}

/** What the generation is doing, for the status line. */
export type GenerationPhase = 'idle' | 'starting' | 'thinking' | 'writing' | 'failed'

/**
 * The generation phase, derived from the snapshot.
 *
 * Pure so it can be tested without a store. `thinking` beats `starting` because
 * a model can spend tens of seconds on chain-of-thought before writing a word
 * (LLM-SURFACE 12.1) -- showing that it is thinking is what keeps a healthy
 * generation from reading as a hang. `writing` beats `thinking` because once
 * content is flowing, that is the useful signal.
 */
export function generationPhase(snapshot: LyricsSnapshot): GenerationPhase {
  if (snapshot.error !== null && !snapshot.generating) return 'failed'
  if (!snapshot.generating) return 'idle'
  if (snapshot.draft.length > 0) return 'writing'
  if (snapshot.thinking.length > 0) return 'thinking'
  return 'starting'
}

/**
 * The tail of the reasoning trace, for the status line.
 *
 * The trace is already bounded, but the status shows only the most recent
 * reasoning -- the model's thinking scrolls past and the user needs to see the
 * newest, not the whole chain.
 */
export function thinkingTail(thinking: string[]): string {
  const joined = thinking.join('')
  return joined.length > 140 ? joined.slice(-140) : joined
}

/**
 * The number a new version gets: one past the highest present.
 *
 * Mirrors `LyricDoc::push_version`'s "continue from the highest, never reuse a
 * number" rule, so a restored older version cannot collide with a later one.
 */
export function nextVersionNumber(versions: LyricVersion[]): number {
  return versions.reduce((max, version) => Math.max(max, version.number), 0) + 1
}

/**
 * The text of the approved version, when one is approved.
 *
 * This is the handoff to AudioStudio (Phase 3): approving a version is what
 * makes it available, and this selector is how the audio side reads it. It is a
 * pure store projection, not a navigation side effect.
 */
export function approvedText(doc: LyricDoc | null): string | null {
  if (doc === null || doc.approved === null) return null
  return doc.versions.find((version) => version.number === doc.approved)?.text ?? null
}

interface LyricsState extends LyricsSnapshot {
  brief: LyricBrief
  listening: boolean
  /** The working document, opened by [`loadDoc`]. */
  doc: LyricDoc | null
  /** Advisory lint findings for the current draft. */
  findings: LintFinding[]
  setBrief: (patch: Partial<LyricBrief>) => void
  prefillFrom: (guide: ProfileGuide | null) => void
  setDraft: (text: string) => void
  generate: (profileId: string) => Promise<void>
  cancel: () => Promise<void>
  startListening: () => Promise<void>
  loadDoc: () => Promise<void>
  commit: (source: LyricSource) => Promise<void>
  commitGenerated: () => Promise<void>
  commitEdited: () => Promise<void>
  restore: (number: number) => void
  approve: (number: number) => Promise<void>
  lint: (profileId: string) => Promise<void>
}

export const useLyricsStore = create<LyricsState>((set, get) => ({
  brief: DEFAULT_BRIEF,
  draft: '',
  thinking: [],
  truncated: false,
  generating: false,
  error: null,
  listening: false,
  doc: null,
  findings: [],

  setBrief: (patch) => set((state) => ({ brief: { ...state.brief, ...patch } })),

  prefillFrom: (guide) => {
    const tags = styleTagsFromGuide(guide)
    if (tags === null) return
    const brief = get().brief
    // Never overwrite the user's own words: only the untouched default is
    // prefilled, so an edit the user already made is left alone.
    if (brief.style_tags !== DEFAULT_BRIEF.style_tags) return
    set({ brief: { ...brief, style_tags: tags } })
  },

  setDraft: (text) => set({ draft: text }),

  generate: async (profileId) => {
    if (!isTauri() || get().generating) return
    set({ draft: '', thinking: [], truncated: false, error: null, generating: true })
    try {
      await generateLyrics(get().brief, profileId)
    } catch (err: unknown) {
      set({ generating: false, error: String(err) })
    }
  },

  cancel: async () => {
    if (!isTauri()) return
    await cancelLyrics()
  },

  startListening: async () => {
    if (get().listening) return
    if (!isTauri()) return
    set({ listening: true })
    await subscribeLyrics((event) => {
      set((state) => applyLyricEvent(state, event))
      if (event.kind === 'done') void get().commitGenerated()
    })
  },

  loadDoc: async () => {
    if (!isTauri()) return
    try {
      const doc = await openLyricDoc()
      const latest = doc.versions[doc.versions.length - 1]
      set({ doc, draft: latest?.text ?? '' })
    } catch (err: unknown) {
      set({ error: String(err) })
    }
  },

  commit: async (source) => {
    const doc = get().doc
    const draft = get().draft
    if (doc === null || draft.trim() === '') return
    const version: LyricVersion = {
      number: nextVersionNumber(doc.versions),
      text: draft,
      created_at: new Date().toISOString(),
      source,
    }
    const next: LyricDoc = { ...doc, versions: [...doc.versions, version] }
    set({ doc: next })
    try {
      await saveLyricDoc(next)
    } catch (err: unknown) {
      set({ error: String(err) })
    }
  },

  commitGenerated: async () => {
    const draft = get().draft
    if (draft.trim() === '') return
    const model = useConfigStore.getState().config?.llm?.model ?? ''
    await get().commit({ kind: 'llm', model, prompt_optimized: false })
  },

  commitEdited: async () => {
    const doc = get().doc
    if (doc === null || doc.versions.length === 0) return
    const from_version = nextVersionNumber(doc.versions) - 1
    await get().commit({ kind: 'edited', from_version })
  },

  restore: (number) => {
    const version = get().doc?.versions.find((v) => v.number === number)
    if (version !== undefined) set({ draft: version.text })
  },

  approve: async (number) => {
    const doc = get().doc
    if (doc === null) return
    const next: LyricDoc = { ...doc, approved: number }
    set({ doc: next })
    try {
      await saveLyricDoc(next)
    } catch (err: unknown) {
      set({ error: String(err) })
    }
  },

  lint: async (profileId) => {
    try {
      const findings = await lintLyrics(profileId, get().brief, get().draft)
      set({ findings })
    } catch (err: unknown) {
      set({ error: String(err) })
    }
  },
}))
