import { describe, expect, it } from 'vitest'
import type { Job } from './jobs'
import {
  CANCELLED,
  DONE,
  FAILED,
  QUEUED,
  RUNNING,
  canCancel,
  elapsed,
  errorFor,
  isDone,
  UNKNOWN_MODEL,
  modelName,
  queueRows,
  statusLabel,
} from './queue'

const T0 = 1_700_000_000_000

function job(over: Partial<Job> = {}): Job {
  return {
    id: 'p-1',
    status: 'running',
    outputs: [],
    error: null,
    profileId: 'ace-step-1.5-turbo',
    submittedAt: T0,
    finishedAt: null,
    ...over,
  }
}

describe('statusLabel', () => {
  it('names each status ComfyUI has actually been observed to send', () => {
    // Measured live 2026-08-29 against a real two-job queue (MCP-SURFACE 25),
    // not assumed. The previous version of this test listed `queued` as an
    // observed value and omitted `pending`; `queued` is the submit response's
    // word and this store's own initial value, and the poll never sends it.
    expect(statusLabel(job({ status: 'pending' }))).toBe(QUEUED)
    expect(statusLabel(job({ status: 'running' }))).toBe(RUNNING)
    expect(statusLabel(job({ status: 'completed' }))).toBe(DONE)
    expect(statusLabel(job({ status: 'cancelled' }))).toBe(CANCELLED)
    expect(statusLabel(job({ status: 'error' }))).toBe(FAILED)
  })

  /**
   * Protects: a job waiting its turn claims to be running.
   *
   * The whole point of the column. `pending` used to fall through to the
   * default and read "Running", so a producer queuing three jobs behind a slow
   * one saw four rows all claiming the GPU -- while three of them had not
   * started and the GPU was idle at 4 GB.
   */
  it('reads a job waiting its turn as Queued, not Running', () => {
    expect(statusLabel(job({ status: 'pending' }))).toBe(QUEUED)
  })

  /** The word this store sets locally before the first poll lands. */
  it('still reads its own initial `queued` as Queued', () => {
    expect(statusLabel(job({ status: 'queued' }))).toBe(QUEUED)
  })

  it('reads `failed` as a failure even though it has never been observed', () => {
    expect(statusLabel(job({ status: 'failed' }))).toBe(FAILED)
  })

  // Protects: `status` is whatever ComfyUI said. A row that renders an
  // unrecognised token raw, or renders nothing, is how the queue lied before.
  it('falls back to Running for a status it does not know, never to blank', () => {
    const label = statusLabel(job({ status: 'some_future_status' }))
    expect(label).toBe(RUNNING)
    expect(label).not.toBe('')
    expect(label).not.toBe('some_future_status')
  })
})

describe('errorFor', () => {
  it('shows a failure its error text', () => {
    const failed = job({ status: 'error', error: 'VAEDecodeAudio failed: shape mismatch' })
    expect(errorFor(failed)).toBe('VAEDecodeAudio failed: shape mismatch')
  })

  // Protects: MCP-SURFACE 21 and 24.3. A cancel carries "Job was
  // interrupted/cancelled." on one of comfy-cli's two error shapes, and
  // showing it under a failure heading reports the user's own decision back to
  // them as a fault.
  it('shows nothing for a cancel, even when one arrives carrying error text', () => {
    const cancelled = job({ status: 'cancelled', error: 'Job was interrupted/cancelled.' })
    expect(errorFor(cancelled)).toBeNull()
    expect(statusLabel(cancelled)).toBe(CANCELLED)
    expect(statusLabel(cancelled)).not.toBe(FAILED)
  })

  it('shows nothing for a job that has no error', () => {
    expect(errorFor(job({ status: 'completed' }))).toBeNull()
  })
})

describe('canCancel', () => {
  it('offers Cancel only while the job is live', () => {
    expect(canCancel(job({ status: 'queued' }))).toBe(true)
    expect(canCancel(job({ status: 'running' }))).toBe(true)
    expect(canCancel(job({ status: 'completed' }))).toBe(false)
    expect(canCancel(job({ status: 'cancelled' }))).toBe(false)
    expect(canCancel(job({ status: 'error' }))).toBe(false)
    expect(canCancel(job({ status: 'failed' }))).toBe(false)
  })

  // Protects: the live set is derived from the terminal set rather than listed
  // separately. Two lists drift, and the drift shows up as a finished job
  // offering a Cancel button that cannot do anything.
  it('is the exact complement of isDone across every status here', () => {
    for (const status of [
      'queued',
      'running',
      'completed',
      'cancelled',
      'error',
      'failed',
      'some_future_status',
    ]) {
      const j = job({ status })
      expect(canCancel(j)).toBe(!isDone(j))
    }
  })
})

describe('modelName', () => {
  it('uses the display name when the map has one', () => {
    expect(modelName('ace-step-1.5-turbo', { 'ace-step-1.5-turbo': 'ACE-Step 1.5 Turbo' })).toBe(
      'ACE-Step 1.5 Turbo',
    )
  })

  // Protects: the commonest miss is the models list not having loaded yet, and
  // a blank column there makes a perfectly good row look broken.
  it('falls back to the id, which is ugly but is information', () => {
    expect(modelName('ace-step-1.5-turbo', {})).toBe('ace-step-1.5-turbo')
  })

  it('says so when there was no profile at all', () => {
    expect(modelName('', { 'ace-step-1.5-turbo': 'ACE-Step 1.5 Turbo' })).toBe(UNKNOWN_MODEL)
  })

  it('never returns an empty string', () => {
    for (const [id, map] of [
      ['', {}],
      ['unknown-id', {}],
      ['x', { x: '' }],
    ] as const) {
      expect(modelName(id, map)).not.toBe('')
    }
  })
})

describe('elapsed', () => {
  it('reads seconds below a minute', () => {
    expect(elapsed(job(), T0 + 12_000)).toBe('12s')
    expect(elapsed(job(), T0 + 59_000)).toBe('59s')
  })

  it('switches to minutes at exactly 60 seconds', () => {
    expect(elapsed(job(), T0 + 60_000)).toBe('1m 0s')
    expect(elapsed(job(), T0 + 72_000)).toBe('1m 12s')
  })

  it('does not go negative if the clock disagrees with itself', () => {
    expect(elapsed(job(), T0 - 5_000)).toBe('0s')
  })

  /**
   * Protects: the clock runs for ever on a job that ended.
   *
   * Found by a producer, not by this suite: a cancelled row was still counting
   * past twenty minutes. The assertion that matters is the second one -- the
   * label must be identical at two very different `now`s, because "it shows the
   * right number once" is exactly what the broken version also did.
   */
  it('stops counting once the job has finished', () => {
    const finished = job({ status: 'completed', finishedAt: T0 + 30_000 })

    expect(elapsed(finished, T0 + 30_000)).toBe('30s')
    expect(elapsed(finished, T0 + 20 * 60_000)).toBe('30s')
  })

  /**
   * Protects: `||` creeping in where `??` belongs.
   *
   * A job that finished in the same millisecond it started stamps `finishedAt`
   * at a falsy-adjacent instant relative to `submittedAt`. This is the fourth
   * absent-versus-empty bug this project has had, so it gets a test rather than
   * a comment.
   */
  it('treats a finish time of zero as a real instant', () => {
    const instant = job({ status: 'completed', submittedAt: 0, finishedAt: 0 })

    expect(elapsed(instant, 999_000)).toBe('0s')
  })

  /** A job still running has no stamp, so it tracks the live clock. */
  it('keeps counting while the job is still running', () => {
    expect(elapsed(job({ status: 'running' }), T0 + 12_000)).toBe('12s')
  })
})

describe('queueRows', () => {
  const live = job({ id: 'live', status: 'running', submittedAt: T0 })
  const older = job({ id: 'older', status: 'completed', submittedAt: T0 + 10_000 })
  const newest = job({ id: 'newest', status: 'completed', submittedAt: T0 + 20_000 })

  function rows(...jobs: Job[]) {
    return queueRows(Object.fromEntries(jobs.map((j) => [j.id, j])), T0 + 30_000)
  }

  // Protects: the job the user is waiting on is the one they came to look at.
  it('puts a live job above a completed one that started later', () => {
    expect(rows(newest, live).map((r) => r.id)).toEqual(['live', 'newest'])
  })

  it('orders newest first within a group', () => {
    expect(rows(older, newest).map((r) => r.id)).toEqual(['newest', 'older'])
  })

  it('applies both rules together', () => {
    expect(rows(older, newest, live).map((r) => r.id)).toEqual(['live', 'newest', 'older'])
  })

  it('is empty for an empty queue', () => {
    expect(queueRows({}, T0)).toEqual([])
  })

  it('carries the model a row was generated for', () => {
    const [row] = rows(job({ id: 'x', profileId: 'minimax-music-3' }))
    expect(row.profileId).toBe('minimax-music-3')
  })

  it('names the model from the map it is given', () => {
    const jobs = { x: job({ id: 'x', profileId: 'minimax-music-3' }) }
    const [row] = queueRows(jobs, T0, { 'minimax-music-3': 'MiniMax Music 3' })
    expect(row.model).toBe('MiniMax Music 3')
  })

  it('leaves the view nothing to decide', () => {
    const [row] = rows(job({ id: 'x', status: 'error', error: 'boom', submittedAt: T0 }))
    expect(row).toEqual({
      id: 'x',
      label: FAILED,
      model: 'ace-step-1.5-turbo',
      profileId: 'ace-step-1.5-turbo',
      elapsed: '30s',
      error: 'boom',
      canCancel: false,
      status: 'error',
    })
  })

  // No test that `jobs` is left unreordered: `Object.values` already returns a
  // fresh array, so such a test cannot fail and would be one more of the
  // vacuous assertions this phase keeps finding. `toSorted` says the intent.
})
