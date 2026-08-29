import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { JobEvent } from '../bridge/jobs'
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
      'prompt-1': {
        id: 'prompt-1',
        status: 'queued',
        outputs: [],
        error: null,
        profileId: 'ace-step-1.5-turbo',
        submittedAt: 0,
        finishedAt: null,
      },
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

  /**
   * Protects: a cancelled job settles, rather than sitting on "running".
   *
   * This is the defect a producer found by pressing the button. `cancel_job`
   * aborted the monitor task, which was the only thing that could report the
   * outcome, and `is_terminal` did not know the word "cancelled" either -- so
   * the row froze on "running" for ever while ComfyUI had in fact stopped
   * within six seconds. Beside the next job, which ran normally, that read as
   * two generations at once (MCP-SURFACE 21).
   */
  it('test_a_cancelled_event_settles_the_row', () => {
    const jobs = {
      'p-1': {
        id: 'p-1',
        status: 'running',
        outputs: [],
        error: null,
        profileId: 'ace-step-1.5-turbo',
        submittedAt: 0,
        finishedAt: null,
      },
    }

    const after = applyJobEvent(jobs, { kind: 'cancelled', payload: { id: 'p-1' } })

    expect(after['p-1'].status).toBe('cancelled')
    expect(after['p-1'].error).toBeNull()
  })

  /** Protects: a cancellation for a job this store never started is ignored. */
  it('test_a_cancelled_event_for_an_unknown_job_changes_nothing', () => {
    expect(applyJobEvent({}, { kind: 'cancelled', payload: { id: 'someone-else' } })).toEqual({})
  })

  /**
   * Protects: the elapsed clock never stops.
   *
   * Aimed at a *missing* stamp, which is the mutation that survives everything
   * else -- every assertion about status still passes with `finishedAt` left
   * null, and the only symptom is a row that counts upward for ever. A producer
   * found this one by watching a cancelled job reach twenty minutes.
   *
   * All three terminal kinds in one test on purpose: the defect is not "done
   * forgot to stamp", it is "some ending does not stamp", and checking them
   * separately is how one gets missed.
   */
  it('test_every_terminal_event_stamps_the_finish_time', () => {
    const kinds: JobEvent[] = [
      { kind: 'done', payload: { id: 'p-1', outputs: [] } },
      { kind: 'cancelled', payload: { id: 'p-1' } },
      { kind: 'failed', payload: { id: 'p-1', error: 'node blew up' } },
    ]

    for (const event of kinds) {
      const jobs: Record<string, Job> = {
        'p-1': {
          id: 'p-1',
          status: 'running',
          outputs: [],
          error: null,
          profileId: 'ace-step-1.5-turbo',
          submittedAt: 0,
          finishedAt: null,
        },
      }

      const after = applyJobEvent(jobs, event, 9_000)

      expect(after['p-1']!.finishedAt, `${event.kind} must stamp a finish time`).toBe(9_000)
    }
  })

  /**
   * Protects: a running job's clock freezes early.
   *
   * The mirror of the test above. `progress` is the only non-terminal event,
   * and stamping it would stop the timer on the first poll -- every row would
   * then read the same two or three seconds no matter how long it ran.
   */
  it('test_a_progress_event_leaves_the_finish_time_unset', () => {
    const jobs: Record<string, Job> = {
      'p-1': {
        id: 'p-1',
        status: 'queued',
        outputs: [],
        error: null,
        profileId: 'ace-step-1.5-turbo',
        submittedAt: 0,
        finishedAt: null,
      },
    }

    const after = applyJobEvent(
      jobs,
      { kind: 'progress', payload: { id: 'p-1', status: 'pending', outputs: [] } },
      9_000,
    )

    expect(after['p-1']!.finishedAt).toBeNull()
  })
})
