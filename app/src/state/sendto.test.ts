import { beforeEach, describe, expect, it, vi } from 'vitest'
import { failureFor, isSending, useSendToStore } from './sendto'

const mockSendTo = vi.fn()

vi.mock('../bridge/sendto', () => ({
  sendTo: (id: string, target: 'mixing' | 'mastering') => mockSendTo(id, target),
}))

function reset() {
  useSendToStore.setState({ sending: null, failure: null })
}

describe('failureFor', () => {
  it('returns the message for its own track', () => {
    expect(
      failureFor({ trackId: 'tr-0001', message: 'file missing' }, 'tr-0001'),
    ).toBe('file missing')
  })

  it('returns null for another track', () => {
    expect(
      failureFor({ trackId: 'tr-0001', message: 'file missing' }, 'tr-0002'),
    ).toBeNull()
  })

  it('returns null when nothing failed', () => {
    expect(failureFor(null, 'tr-0001')).toBeNull()
  })
})

describe('isSending', () => {
  it('is true only for the track in flight', () => {
    expect(isSending('tr-0001', 'tr-0001')).toBe(true)
    expect(isSending('tr-0001', 'tr-0002')).toBe(false)
    expect(isSending(null, 'tr-0001')).toBe(false)
  })
})

describe('sendTo store', () => {
  beforeEach(() => {
    mockSendTo.mockReset()
    reset()
  })

  it('records the failure against the track that failed', async () => {
    mockSendTo.mockRejectedValue('That track is gone')
    await useSendToStore.getState().send('tr-0001', 'mixing')
    expect(useSendToStore.getState().failure).toEqual({
      trackId: 'tr-0001',
      message: 'That track is gone',
    })
  })

  it('clears a previous failure before trying again', async () => {
    useSendToStore.setState({ failure: { trackId: 'tr-0001', message: 'old' } })
    mockSendTo.mockImplementation(() => new Promise(() => {}))
    void useSendToStore.getState().send('tr-0001', 'mixing')
    expect(useSendToStore.getState().failure).toBeNull()
  })

  // Protects: the destination the user clicked is the destination that is
  // opened. Choosing between two sites is the whole of what this store does,
  // and every other test here passes with the target hardcoded (mutation-
  // checked 2026-09-01).
  it('passes the chosen destination through to the bridge', async () => {
    mockSendTo.mockResolvedValue(undefined)
    await useSendToStore.getState().send('tr-0007', 'mastering')
    expect(mockSendTo).toHaveBeenCalledWith('tr-0007', 'mastering')
  })

  // Protects: the buttons come back after a send that worked. Only the
  // failure path's reset was covered, so deleting the success path's left
  // the row disabled forever with a green suite.
  it('clears the sending marker after a successful send', async () => {
    mockSendTo.mockResolvedValue(undefined)
    await useSendToStore.getState().send('tr-0007', 'mixing')
    expect(useSendToStore.getState().sending).toBeNull()
    expect(useSendToStore.getState().failure).toBeNull()
  })

  it('leaves no row marked sending after a failure', async () => {
    mockSendTo.mockRejectedValue('nope')
    await useSendToStore.getState().send('tr-0001', 'mastering')
    expect(useSendToStore.getState().sending).toBeNull()
  })
})
