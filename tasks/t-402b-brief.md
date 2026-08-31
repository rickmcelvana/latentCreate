# T-402b: playback + visualizer -- the player state machine

**Depends:** T-402a (the `track_audio_path` command + asset protocol) | **Crate/dir:** app/src
**Files to create/modify:**
- `app/src/bridge/player.ts` (new)
- `app/src/state/player.ts` (new)
- `app/src/state/player.test.ts` (new)

## Goal

The pure half of the player: a bridge wrapper that turns a track id into a playable URL, and a
Zustand store whose state machine (load/play/pause/seek/end/error) is a pure, tested fold. The
`<audio>` element and the canvas visualizer are T-402c; this brief is everything a unit test can
reach, so nothing here touches the DOM. `convertFileSrc` and `invoke` stay in the bridge, never in
the store or a component (CONVENTIONS).

## Spec

### `app/src/bridge/player.ts` (new)

```ts
import { convertFileSrc, invoke } from '@tauri-apps/api/core'

/**
 * Resolve a track id to a URL the webview can play.
 *
 * The backend returns an absolute path (T-402a's `track_audio_path`, which
 * validates the id and the stored file); `convertFileSrc` turns that into an
 * `asset://localhost/...` (or `http://asset.localhost/...`) URL the asset
 * protocol serves. Both halves stay here: the store and components import this
 * wrapper, never `@tauri-apps/*`.
 */
export async function trackAudioUrl(id: string): Promise<string> {
  const absolute = await invoke<string>('track_audio_path', { id })
  return convertFileSrc(absolute)
}
```

### `app/src/state/player.ts` (new)

```ts
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
```

## Tests (`app/src/state/player.test.ts`, new)

Import only the pure functions -- no bridge mock is needed because nothing here touches the
bridge until `play` is called, and the pure functions never call it.

```ts
import { describe, expect, it } from 'vitest'
import {
  applyPlayerEvent,
  clampPosition,
  formatTime,
  initialPlayerState,
  statusLabel,
  togglePlayer,
  type PlayerState,
} from './player'

function stateWith(overrides: Partial<PlayerState> = {}): PlayerState {
  return {
    ...initialPlayerState,
    track: { id: 'tr-0001', name: 'Midnight Drive', url: 'asset://localhost/track.flac' },
    ...overrides,
  }
}

describe('formatTime', () => {
  it('formats m:ss, flooring and zero-padding seconds', () => {
    expect(formatTime(0)).toBe('0:00')
    expect(formatTime(59)).toBe('0:59')
    expect(formatTime(60)).toBe('1:00')
    expect(formatTime(119.6)).toBe('1:59')
    expect(formatTime(120)).toBe('2:00')
  })

  it('reads non-finite or negative input as zero', () => {
    expect(formatTime(Number.NaN)).toBe('0:00')
    expect(formatTime(Number.POSITIVE_INFINITY)).toBe('0:00')
    expect(formatTime(-5)).toBe('0:00')
  })
})

describe('clampPosition', () => {
  it('clamps to [0, duration]', () => {
    expect(clampPosition(-1, 100)).toBe(0)
    expect(clampPosition(50, 100)).toBe(50)
    expect(clampPosition(101, 100)).toBe(100)
  })

  it('clamps only the lower bound when duration is unknown', () => {
    expect(clampPosition(-1, null)).toBe(0)
    expect(clampPosition(500, null)).toBe(500)
  })

  it('reads non-finite input as zero', () => {
    expect(clampPosition(Number.NaN, 100)).toBe(0)
  })
})

describe('statusLabel', () => {
  it('labels every status', () => {
    expect(statusLabel('idle')).toBe('')
    expect(statusLabel('loading')).toBe('Loading')
    expect(statusLabel('playing')).toBe('Playing')
    expect(statusLabel('paused')).toBe('Paused')
    expect(statusLabel('ended')).toBe('Ended')
    expect(statusLabel('error')).toBe('Playback error')
  })
})

describe('applyPlayerEvent', () => {
  it('load resets state, sets the track, and lands on playing', () => {
    const next = applyPlayerEvent(initialPlayerState, {
      kind: 'load',
      payload: { id: 'tr-0001', name: 'Midnight Drive', url: 'asset://localhost/track.flac' },
    })
    expect(next).toEqual({
      track: { id: 'tr-0001', name: 'Midnight Drive', url: 'asset://localhost/track.flac' },
      status: 'playing',
      position: 0,
      duration: null,
      error: null,
    })
  })

  it('play and pause are no-ops with no track loaded', () => {
    expect(applyPlayerEvent(initialPlayerState, { kind: 'play' })).toBe(initialPlayerState)
    expect(applyPlayerEvent(initialPlayerState, { kind: 'pause' })).toBe(initialPlayerState)
    expect(applyPlayerEvent(initialPlayerState, { kind: 'ended' })).toBe(initialPlayerState)
  })

  it('play sets playing and clears a prior error', () => {
    const errored = stateWith({ status: 'error', error: 'boom' })
    const next = applyPlayerEvent(errored, { kind: 'play' })
    expect(next.status).toBe('playing')
    expect(next.error).toBeNull()
  })

  it('ended snaps position to duration', () => {
    const next = applyPlayerEvent(stateWith({ status: 'playing', position: 12, duration: 120 }), {
      kind: 'ended',
    })
    expect(next.status).toBe('ended')
    expect(next.position).toBe(120)
  })

  it('ended with an unknown duration leaves position at zero', () => {
    const next = applyPlayerEvent(stateWith({ status: 'playing', position: 12, duration: null }), {
      kind: 'ended',
    })
    expect(next.status).toBe('ended')
    expect(next.position).toBe(0)
  })

  it('seek clamps to duration', () => {
    const next = applyPlayerEvent(stateWith({ duration: 100 }), {
      kind: 'seek',
      payload: { position: 500 },
    })
    expect(next.position).toBe(100)
  })

  it('time updates position', () => {
    const next = applyPlayerEvent(stateWith({ position: 0 }), {
      kind: 'time',
      payload: { position: 42 },
    })
    expect(next.position).toBe(42)
  })

  it('duration floors negatives to zero', () => {
    const next = applyPlayerEvent(stateWith({}), {
      kind: 'duration',
      payload: { duration: -3 },
    })
    expect(next.duration).toBe(0)
  })

  it('error sets the error status and message', () => {
    const next = applyPlayerEvent(stateWith({}), {
      kind: 'error',
      payload: { message: 'This track could not be played.' },
    })
    expect(next.status).toBe('error')
    expect(next.error).toBe('This track could not be played.')
  })
})

describe('togglePlayer', () => {
  it('is a no-op with no track', () => {
    expect(togglePlayer(initialPlayerState)).toBe(initialPlayerState)
  })

  it('pauses a playing track', () => {
    expect(togglePlayer(stateWith({ status: 'playing' })).status).toBe('paused')
  })

  it('plays a paused track', () => {
    expect(togglePlayer(stateWith({ status: 'paused' })).status).toBe('playing')
  })

  it('restarts an ended track from zero', () => {
    const next = togglePlayer(stateWith({ status: 'ended', position: 120, duration: 120 }))
    expect(next.status).toBe('playing')
    expect(next.position).toBe(0)
  })
})
```

## Acceptance criteria

- [ ] `tsc -b`, `oxlint src`, `vitest run` and `vite build` green; frontend goes 336 -> **351** tests (15 new).
- [ ] Every public pure function (`formatTime`, `clampPosition`, `statusLabel`, `applyPlayerEvent`, `togglePlayer`) is exported and tested; no store action touches `convertFileSrc` or `invoke` directly.
- [ ] `invoke` and `convertFileSrc` appear only in `app/src/bridge/player.ts` (grep `convertFileSrc` and `invoke('track_audio_path'` across `app/src`).
- [ ] The `play` action's `catch` narrows `unknown` to a string without `any`.
- [ ] No changes outside the three listed files.

## Out of scope

- The `<audio>` element, transport controls, seek bar and canvas visualizer (T-402c).
- Cross-view playback persistence (the player bar's placement is T-402c; persistence across
  navigation is a later product decision, not this phase).

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/state/jobs.ts --read app/src/bridge/library.ts --read app/src/bridge/jobs.ts --file app/src/bridge/player.ts --file app/src/state/player.ts --file app/src/state/player.test.ts
```
