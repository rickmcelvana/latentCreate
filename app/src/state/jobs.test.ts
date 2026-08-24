import { beforeEach, describe, expect, it, vi } from 'vitest'
import { applyJobEvent, useJobsStore, type Job } from './jobs'

const mockConnectComfy = vi.fn()
const mockRunWorkflow = vi.fn()
const mockCancelJob = vi.fn()
const mockSubscribeJobs = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/jobs', () => ({
  isTauri: () => mockIsTauri,
  connectComfy: (bin?: string) => mockConnectComfy(bin),
  runWorkflow: (path: string) => mockRunWorkflow(path),
  cancelJob: (id: string) => mockCancelJob(id),
  subscribeJobs: (cb: (e: unknown) => void) => mockSubscribeJobs(cb),
}))

beforeEach(() => {
  mockIsTauri = true
  mockConnectComfy.mockReset()
  mockRunWorkflow.mockReset()
  mockCancelJob.mockReset()
  mockSubscribeJobs.mockReset()
  useJobsStore.setState({ jobs: {}, connected: false, listening: false })
})

describe('jobs store', () => {
  it('test_run_adds_a_queued_job', async () => {
    mockRunWorkflow.mockResolvedValue('prompt-1')
    const id = await useJobsStore.getState().run('wf.json')
    expect(id).toBe('prompt-1')
    expect(useJobsStore.getState().jobs['prompt-1']).toMatchObject({
      id: 'prompt-1',
      status: 'queued',
      outputs: [],
      error: null,
    })
  })

  it('test_connect_sets_connected', async () => {
    mockConnectComfy.mockResolvedValue(undefined)
    await useJobsStore.getState().connect()
    expect(useJobsStore.getState().connected).toBe(true)
    expect(mockConnectComfy).toHaveBeenCalledTimes(1)
  })

  it('test_cancel_calls_the_backend', async () => {
    mockCancelJob.mockResolvedValue(undefined)
    await useJobsStore.getState().cancel('prompt-1')
    expect(mockCancelJob).toHaveBeenCalledWith('prompt-1')
  })

  it('test_start_listening_subscribes_once', async () => {
    mockSubscribeJobs.mockResolvedValue(() => {})
    await useJobsStore.getState().startListening()
    await useJobsStore.getState().startListening()
    expect(mockSubscribeJobs).toHaveBeenCalledTimes(1)
  })

  it('test_start_listening_is_skipped_outside_tauri', async () => {
    mockIsTauri = false
    await useJobsStore.getState().startListening()
    expect(mockSubscribeJobs).not.toHaveBeenCalled()
  })

  it('test_apply_event_folds_progress_done_failed', () => {
    const initial: Record<string, Job> = {
      'prompt-1': { id: 'prompt-1', status: 'queued', outputs: [], error: null },
    }

    const running = applyJobEvent(initial, {
      kind: 'progress',
      payload: { id: 'prompt-1', status: 'running', outputs: [] },
    })
    expect(running['prompt-1']!.status).toBe('running')

    const done = applyJobEvent(running, {
      kind: 'done',
      payload: { id: 'prompt-1', outputs: ['x.mp3'] },
    })
    expect(done['prompt-1']!.status).toBe('completed')
    expect(done['prompt-1']!.outputs).toEqual(['x.mp3'])

    const failed = applyJobEvent(initial, {
      kind: 'failed',
      payload: { id: 'prompt-1', error: 'boom' },
    })
    expect(failed['prompt-1']!.status).toBe('failed')
    expect(failed['prompt-1']!.error).toBe('boom')
  })

  it('test_apply_event_ignores_unknown_ids', () => {
    const initial: Record<string, Job> = {}
    const result = applyJobEvent(initial, {
      kind: 'done',
      payload: { id: 'nope', outputs: [] },
    })
    expect(result).toBe(initial)
  })
})
