import { beforeEach, describe, expect, it, vi } from 'vitest'
import { subscribeJobs, type JobEvent } from './jobs'

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

describe('bridge/jobs', () => {
  it('test_subscribe_jobs_registers_the_three_event_names', async () => {
    await subscribeJobs(() => {})
    const names = mockListen.mock.calls.map((call) => call[0] as string)
    expect(names).toEqual(['job://progress', 'job://done', 'job://failed'])
  })

  it('test_subscribe_jobs_dispatches_tagged_events', async () => {
    const received: JobEvent[] = []
    await subscribeJobs((event) => received.push(event))

    const handlers = mockListen.mock.calls.map((call) => call[1])
    handlers[0]!({ payload: { id: 'a', status: 'running', outputs: [] } })
    handlers[1]!({ payload: { id: 'a', outputs: ['x.mp3'] } })
    handlers[2]!({ payload: { id: 'a', error: 'boom' } })

    expect(received).toEqual([
      { kind: 'progress', payload: { id: 'a', status: 'running', outputs: [] } },
      { kind: 'done', payload: { id: 'a', outputs: ['x.mp3'] } },
      { kind: 'failed', payload: { id: 'a', error: 'boom' } },
    ])
  })
})
