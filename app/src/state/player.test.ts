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
