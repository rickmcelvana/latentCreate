import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/** Mirrors Rust `create-core::lyrics::PointOfView` (snake_case on the wire). */
export type PointOfView = 'first_person' | 'second_person' | 'third_person'

/**
 * Mirrors Rust `create-core::lyrics::LyricBrief`, the record of what the user
 * asked for. Field names are snake_case on the wire, exactly as serde emits
 * them.
 */
export interface LyricBrief {
  /** What the song is about. */
  theme: string
  /** Genre and style tags, comma-separated. */
  style_tags: string
  mood: string
  /** Section letters, e.g. `"V-C-V-C-B-C"`. */
  structure: string
  /** The language to write in, as a person names it. */
  language: string
  point_of_view: PointOfView
  /** Era or artist references, when the user gives any. */
  era_refs: string | null
  /** Whether explicit language is allowed. */
  explicit_allowed: boolean
  /** Target song length in seconds. */
  target_duration_s: number
}

/** Mirrors Rust `wire::TokenUsage`. */
export interface TokenUsage {
  prompt_tokens: number
  completion_tokens: number
  total_tokens: number
}

/** Mirrors Rust `src-tauri/src/lyrics.rs` `LyricDone`. */
export interface LyricDone {
  /** The model's stop reason, e.g. `"stop"` or `"length"` (truncation). */
  finish_reason: string | null
  usage: TokenUsage | null
}

/** Mirrors Rust `LyricFailed`. */
export interface LyricFailed {
  error: string
}

/** One event from the lyric pump, tagged so the store can switch on it. */
export type LyricEvent =
  | { kind: 'delta'; payload: { text: string } }
  | { kind: 'thinking'; payload: { text: string } }
  | { kind: 'done'; payload: LyricDone }
  | { kind: 'failed'; payload: LyricFailed }

/**
 * The prefilled brief, mirroring Rust `LyricBrief::default`. ARCHITECTURE 6
 * requires the form to open with strong examples rather than empty boxes.
 */
export const DEFAULT_BRIEF: LyricBrief = {
  theme: 'A night drive out of a city you are leaving for good',
  style_tags: 'synthwave, retro, 80s, dreamy, female vocal, driving beat',
  mood: 'bittersweet, hopeful',
  structure: 'V-C-V-C-B-C',
  language: 'English',
  point_of_view: 'first_person',
  era_refs: null,
  explicit_allowed: false,
  target_duration_s: 120,
}

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * Stream a lyric for the brief against one profile.
 *
 * Returns once the backend has accepted the generation, not when it finishes --
 * progress arrives as [`LyricEvent`]s on the subscription.
 */
export async function generateLyrics(brief: LyricBrief, profileId: string): Promise<void> {
  await invoke('lyrics_generate', { brief, profileId })
}

/** Abort the in-flight generation, if any. */
export async function cancelLyrics(): Promise<void> {
  await invoke('lyrics_cancel')
}

/**
 * Subscribe to all four lyric events, dispatching each as a tagged [`LyricEvent`].
 * Returns the unsubscribe function.
 */
export async function subscribeLyrics(onEvent: (event: LyricEvent) => void): Promise<UnlistenFn> {
  const undelta = await listen<{ text: string }>('lyrics://delta', (event) => {
    onEvent({ kind: 'delta', payload: event.payload })
  })
  const unthinking = await listen<{ text: string }>('lyrics://thinking', (event) => {
    onEvent({ kind: 'thinking', payload: event.payload })
  })
  const undone = await listen<LyricDone>('lyrics://done', (event) => {
    onEvent({ kind: 'done', payload: event.payload })
  })
  const unfailed = await listen<LyricFailed>('lyrics://failed', (event) => {
    onEvent({ kind: 'failed', payload: event.payload })
  })
  return () => {
    undelta()
    unthinking()
    undone()
    unfailed()
  }
}
