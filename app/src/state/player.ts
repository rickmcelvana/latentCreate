import { create } from 'zustand'
import { trackAudioUrl } from '../bridge/player'

/** What the player is doing. `idle` means nothing has ever been loaded. */
export type PlayerStatus = 'idle' | 'loading' | 'playing' | 'paused' | 'ended' | 'error'

/** The track currently loaded in the player, or `null` when nothing is. */
export interface PlayingTrack {
  id: string
  /** The user-facing title, else the id -- already resolved by the Library row. */
  name: string
  /** The asset URL this track plays from, resolved by [`trackAudioUrl`]. */
  url: string
}

/** The whole player state, driven by a pure fold over [`PlayerEvent`]s. */
export interface PlayerState {
  track: PlayingTrack | null
  status: PlayerStatus
  /** Playhead position, seconds. */
  position: number
  /** Track length, seconds; `null` until the media reports it. */
  duration: number | null
  error: string | null
}

/** The state before anything is played. */
export const initialPlayerState: PlayerState = {
  track: null,
  status: 'idle',
  position: 0,
  duration: null,
  error: null,
}

/**
 * Formats a duration as `m:ss`, flooring and zero-padding the seconds.
 *
 * Non-finite or negative input reads as zero rather than rendering `NaN`.
 */
export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00'
  const total = Math.floor(seconds)
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}

/**
 * Clamps a position to `[0, duration]`.
 *
 * With no duration yet, only the lower bound applies: a `timeupdate` can arrive
 * before `loadedmetadata`, and clamping it to `[0, 0]` would pin the playhead.
 * Non-finite input reads as zero.
 */
export function clampPosition(position: number, duration: number | null): number {
  if (!Number.isFinite(position)) return 0
  const upper = duration === null ? Number.POSITIVE_INFINITY : Math.max(0, duration)
  return Math.min(Math.max(0, position), upper)
}

/** A short label for the player's status, for the transport header. */
export function statusLabel(status: PlayerStatus): string {
  switch (status) {
    case 'idle':
      return ''
    case 'loading':
      return 'Loading'
    case 'playing':
      return 'Playing'
    case 'paused':
      return 'Paused'
    case 'ended':
      return 'Ended'
    case 'error':
      return 'Playback error'
  }
}

/** One thing the media element or the user told the player. */
export type PlayerEvent =
  | { kind: 'load'; payload: { id: string; name: string; url: string } }
  | { kind: 'play' }
  | { kind: 'pause' }
  | { kind: 'ended' }
  | { kind: 'seek'; payload: { position: number } }
  | { kind: 'time'; payload: { position: number } }
  | { kind: 'duration'; payload: { duration: number } }
  | { kind: 'error'; payload: { message: string } }

/**
 * Fold one event into the player state. Pure so every transition is testable
 * without a store, a bridge, or an audio element.
 *
 * `load` is the result of a play request, so it lands on `playing` rather than
 * `loading` -- the `loading` status only covers the URL-resolution round trip
 * the store's `play` action owns.
 */
export function applyPlayerEvent(state: PlayerState, event: PlayerEvent): PlayerState {
  switch (event.kind) {
    case 'load':
      return {
        track: { id: event.payload.id, name: event.payload.name, url: event.payload.url },
        status: 'playing',
        position: 0,
        duration: null,
        error: null,
      }
    case 'play':
      if (state.track === null) return state
      return { ...state, status: 'playing', error: null }
    case 'pause':
      if (state.track === null) return state
      return { ...state, status: 'paused' }
    case 'ended':
      if (state.track === null) return state
      return { ...state, status: 'ended', position: state.duration ?? 0 }
    case 'seek':
      return { ...state, position: clampPosition(event.payload.position, state.duration) }
    case 'time':
      return { ...state, position: clampPosition(event.payload.position, state.duration) }
    case 'duration':
      return { ...state, duration: Math.max(0, event.payload.duration) }
    case 'error':
      return { ...state, status: 'error', error: event.payload.message }
  }
}

/**
 * The transport toggle, as a pure function so its two decisions are testable:
 * playing pauses; anything else plays, and a track that ended restarts from
 * zero rather than resuming at the tail.
 */
export function togglePlayer(state: PlayerState): PlayerState {
  if (state.track === null) return state
  if (state.status === 'playing') return applyPlayerEvent(state, { kind: 'pause' })
  const reset =
    state.status === 'ended'
      ? applyPlayerEvent(state, { kind: 'seek', payload: { position: 0 } })
      : state
  return applyPlayerEvent(reset, { kind: 'play' })
}

interface PlayerStore extends PlayerState {
  /** Resolve a track's URL and start playing it. */
  play: (id: string, name: string) => Promise<void>
  pause: () => void
  toggle: () => void
  /** Seek to an absolute position in seconds; clamped by the fold. */
  seek: (position: number) => void
  /** The media reported a new playhead position. */
  reportTime: (position: number) => void
  /** The media reported its length. */
  reportDuration: (duration: number) => void
  /** The media ended. */
  ended: () => void
  /** The media failed to load or play. */
  fail: (message: string) => void
}

export const usePlayerStore = create<PlayerStore>((set, get) => ({
  ...initialPlayerState,

  play: async (id, name) => {
    // Only the URL round trip is `loading`; `load` lands on `playing`.
    set({ status: 'loading', error: null })
    try {
      const url = await trackAudioUrl(id)
      set(applyPlayerEvent(get(), { kind: 'load', payload: { id, name, url } }))
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err)
      set(applyPlayerEvent(get(), { kind: 'error', payload: { message } }))
    }
  },

  pause: () => set(applyPlayerEvent(get(), { kind: 'pause' })),
  toggle: () => set(togglePlayer(get())),
  seek: (position) => set(applyPlayerEvent(get(), { kind: 'seek', payload: { position } })),
  reportTime: (position) => set(applyPlayerEvent(get(), { kind: 'time', payload: { position } })),
  reportDuration: (duration) =>
    set(applyPlayerEvent(get(), { kind: 'duration', payload: { duration } })),
  ended: () => set(applyPlayerEvent(get(), { kind: 'ended' })),
  fail: (message) => set(applyPlayerEvent(get(), { kind: 'error', payload: { message } })),
}))
