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

interface LyricsState extends LyricsSnapshot {
  brief: LyricBrief
  listening: boolean
  setBrief: (patch: Partial<LyricBrief>) => void
  generate: (profileId: string) => Promise<void>
  cancel: () => Promise<void>
  startListening: () => Promise<void>
}

export const useLyricsStore = create<LyricsState>((set, get) => ({
  brief: DEFAULT_BRIEF,
  draft: '',
  thinking: [],
  truncated: false,
  generating: false,
  error: null,
  listening: false,

  setBrief: (patch) => set((state) => ({ brief: { ...state.brief, ...patch } })),

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
    await subscribeLyrics((event) => set((state) => applyLyricEvent(state, event)))
  },
}))
