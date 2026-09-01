import { create } from 'zustand'
import {
  cancelLyrics,
  generateLyrics,
  isTauri,
  optimizePrompt,
  subscribeLyrics,
  DEFAULT_BRIEF,
  type LyricBrief,
  type LyricEvent,
  type PromptOptimization,
} from '../bridge/lyrics'
import type { ProfileGuide } from '../bridge/profiles'
import {
  deleteLyricVersion,
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

/**
 * How the approved version reads as a status, or null when none is approved.
 *
 * Separate from [`approvedText`] because the two answer different questions:
 * that one is the handoff to audio, this one is what the user is shown. Kept
 * pure and tested because the alternative -- deriving it inline in the view --
 * is how the notice ended up correct, untested, and invisible.
 */
export function approvedLabel(doc: LyricDoc | null): string | null {
  if (doc === null || doc.approved === null) return null
  // An approved number with no version behind it is not something to announce.
  if (!doc.versions.some((version) => version.number === doc.approved)) return null
  return `v${doc.approved} approved`
}

/**
 * The optimizer's state, as three fields that cannot contradict each other.
 *
 * `optimization` is the round trip awaiting review and `proposed` is the text
 * the user is reviewing (their edits included); both clear on Accept or Revert.
 * `promptOverride` is the only one generation reads, and it is set **only** by
 * Accept -- which is what makes the optimizer consent-gated rather than a
 * rewrite that happens to be visible (ARCHITECTURE 6).
 */
export interface OptimizerState {
  optimization: PromptOptimization | null
  proposed: string
  optimizing: boolean
  /** The accepted prompt, sent in place of the assembled brief. */
  promptOverride: string | null
}

/** Lint state as it looks before a check, and after the draft changes under one. */
const NO_LINT = { findings: [] as LintFinding[], linted: false }

/** The optimizer state as it looks with nothing proposed and nothing accepted. */
const NO_OPTIMIZATION: OptimizerState = {
  optimization: null,
  proposed: '',
  optimizing: false,
  promptOverride: null,
}

interface LyricsState extends LyricsSnapshot, OptimizerState {
  brief: LyricBrief
  listening: boolean
  /** The working document, opened by [`loadDoc`]. */
  doc: LyricDoc | null
  /** Advisory lint findings for the current draft. */
  findings: LintFinding[]
  /**
   * Whether [`lint`] has run against the draft as it now reads.
   *
   * Distinguishes "checked, nothing to say" from "not checked yet", which an
   * empty `findings` alone cannot. Any change to the draft clears it, because
   * findings about text the user has since edited are worse than none.
   */
  linted: boolean
  /**
   * The version number awaiting a delete confirmation, or `null`. Keyed by
   * number because that is a version's identity within a document -- the T-405
   * `confirming` shape, applied to versions.
   */
  confirmingVersion: number | null
  /**
   * The message from a refused version delete (it names the tracks holding the
   * version) and the version it belongs to, or `null`. Kept separate from
   * `error` on purpose: `error` is part of `LyricsSnapshot` and feeds
   * `generationPhase`, so reusing it would flip the editor's status pill to
   * "Failed" for a refusal that is not a generation failure. Carries `version`
   * so the message renders at *that row* -- a document can hold dozens of
   * versions, and a message at the top of the list is off-screen when the row
   * acted on is far down (T-408a click-through).
   */
  deleteError: { version: number; message: string } | null
  setBrief: (patch: Partial<LyricBrief>) => void
  optimize: (profileId: string) => Promise<void>
  setProposed: (text: string) => void
  acceptOptimized: () => void
  revertOptimized: () => void
  prefillFrom: (guide: ProfileGuide | null) => void
  setDraft: (text: string) => void
  generate: (profileId: string) => Promise<void>
  cancel: () => Promise<void>
  startListening: () => Promise<void>
  loadDoc: () => Promise<void>
  commit: (source: LyricSource) => Promise<void>
  commitGenerated: () => Promise<void>
  commitEdited: () => Promise<void>
  saveDraft: () => Promise<void>
  restore: (number: number) => void
  approve: (number: number) => Promise<void>
  askDeleteVersion: (number: number) => void
  cancelDeleteVersion: () => void
  deleteVersion: (number: number) => Promise<boolean>
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
  linted: false,
  confirmingVersion: null,
  deleteError: null,
  ...NO_OPTIMIZATION,

  // Editing the brief drops any accepted prompt. The override was written
  // against the previous brief, so keeping it would leave the form describing
  // one song while the request describes another -- and the form is what the
  // user believes they are sending.
  setBrief: (patch) =>
    set((state) => ({ brief: { ...state.brief, ...patch }, ...NO_OPTIMIZATION })),

  optimize: async (profileId) => {
    if (!isTauri() || get().optimizing) return
    set({ optimizing: true, error: null })
    try {
      const optimization = await optimizePrompt(get().brief, profileId)
      set({ optimization, proposed: optimization.optimized, optimizing: false })
    } catch (err: unknown) {
      set({ optimizing: false, error: String(err) })
    }
  },

  setProposed: (text) => set({ proposed: text }),

  // The one place `promptOverride` is written. A blank proposal is not an
  // acceptance: there is nothing there to have consented to.
  acceptOptimized: () => {
    const proposed = get().proposed
    if (proposed.trim() === '') return
    set({ promptOverride: proposed, optimization: null, proposed: '' })
  },

  revertOptimized: () => set({ ...NO_OPTIMIZATION }),

  prefillFrom: (guide) => {
    const tags = styleTagsFromGuide(guide)
    if (tags === null) return
    const brief = get().brief
    // Never overwrite the user's own words: only the untouched default is
    // prefilled, so an edit the user already made is left alone.
    if (brief.style_tags !== DEFAULT_BRIEF.style_tags) return
    set({ brief: { ...brief, style_tags: tags } })
  },

  setDraft: (text) => set({ draft: text, ...NO_LINT }),

  generate: async (profileId) => {
    if (!isTauri() || get().generating) return
    set({
      draft: '',
      thinking: [],
      truncated: false,
      error: null,
      generating: true,
      ...NO_LINT,
    })
    try {
      await generateLyrics(get().brief, profileId, get().promptOverride)
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
    // The provenance flag is the accepted override, not the fact that an
    // optimizer ran: a rewrite the user reverted never reached the model.
    await get().commit({
      kind: 'llm',
      model,
      prompt_optimized: get().promptOverride !== null,
    })
  },

  commitEdited: async () => {
    const doc = get().doc
    if (doc === null || doc.versions.length === 0) return
    const from_version = nextVersionNumber(doc.versions) - 1
    await get().commit({ kind: 'edited', from_version })
  },

  saveDraft: async () => {
    const doc = get().doc
    if (doc === null) return
    if (doc.versions.length === 0) {
      await get().commit({ kind: 'human' })
    } else {
      await get().commitEdited()
    }
  },

  restore: (number) => {
    const version = get().doc?.versions.find((v) => v.number === number)
    if (version !== undefined) set({ draft: version.text, ...NO_LINT })
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

  askDeleteVersion: (number) => set({ confirmingVersion: number, deleteError: null }),
  cancelDeleteVersion: () => set({ confirmingVersion: null }),

  // The backend is the authority: it refuses when a track references the
  // version, and returns the document with the version removed. The store
  // replaces `doc` with that result -- it never edits `versions` locally, which
  // would skip the refusal check entirely.
  deleteVersion: async (number) => {
    const doc = get().doc
    if (doc === null) return false
    try {
      const updated = await deleteLyricVersion(doc.id, number)
      set({ doc: updated, confirmingVersion: null, deleteError: null })
      return true
    } catch (err: unknown) {
      // The message names the tracks holding the version -- show it as-is, at
      // the row it belongs to.
      set({ deleteError: { version: number, message: String(err) }, confirmingVersion: null })
      return false
    }
  },

  lint: async (profileId) => {
    try {
      const findings = await lintLyrics(profileId, get().brief, get().draft)
      set({ findings, linted: true })
    } catch (err: unknown) {
      set({ error: String(err) })
    }
  },
}))
