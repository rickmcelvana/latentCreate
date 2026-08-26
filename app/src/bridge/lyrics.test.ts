import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DEFAULT_BRIEF, subscribeLyrics, type LyricEvent } from './lyrics'

const mockListen = vi.fn()

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, handler: (e: { payload: unknown }) => void) => {
    mockListen(event, handler)
    return Promise.resolve(() => {})
  },
}))

beforeEach(() => {
  mockListen.mockClear()
})

describe('bridge/lyrics', () => {
  it('test_subscribe_lyrics_registers_the_four_event_names', async () => {
    await subscribeLyrics(() => {})
    const names = mockListen.mock.calls.map((call) => call[0] as string)
    expect(names).toEqual([
      'lyrics://delta',
      'lyrics://thinking',
      'lyrics://done',
      'lyrics://failed',
    ])
  })

  it('test_subscribe_lyrics_dispatches_tagged_events', async () => {
    const received: LyricEvent[] = []
    await subscribeLyrics((event) => received.push(event))

    const handlers = mockListen.mock.calls.map((call) => call[1])
    handlers[0]!({ payload: { text: 'first line' } })
    handlers[1]!({ payload: { text: 'thinking...' } })
    handlers[2]!({
      payload: {
        finish_reason: 'length',
        usage: { prompt_tokens: 1, completion_tokens: 2, total_tokens: 3 },
      },
    })
    handlers[3]!({ payload: { error: 'boom' } })

    expect(received).toEqual([
      { kind: 'delta', payload: { text: 'first line' } },
      { kind: 'thinking', payload: { text: 'thinking...' } },
      {
        kind: 'done',
        payload: {
          finish_reason: 'length',
          usage: { prompt_tokens: 1, completion_tokens: 2, total_tokens: 3 },
        },
      },
      { kind: 'failed', payload: { error: 'boom' } },
    ])
  })

  /**
   * Protects: the prefills mirror the Rust default. A brief the frontend opens
   * with must be one the backend can accept as-is -- the two defaults drifting
   * apart would silently change what a generation asks for.
   */
  it('test_default_brief_matches_the_rust_prefills', () => {
    expect(DEFAULT_BRIEF.structure).toBe('V-C-V-C-B-C')
    expect(DEFAULT_BRIEF.language).toBe('English')
    expect(DEFAULT_BRIEF.point_of_view).toBe('first_person')
    expect(DEFAULT_BRIEF.explicit_allowed).toBe(false)
    expect(DEFAULT_BRIEF.target_duration_s).toBe(120)
    expect(DEFAULT_BRIEF.era_refs).toBeNull()
  })
})
