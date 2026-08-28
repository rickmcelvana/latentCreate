import type { Job } from './jobs'

/**
 * Everything the queue panel decides, so that the panel decides nothing.
 *
 * The queue is where this app's two hardest reporting bugs have lived. Both
 * were sentences: a row that said `running` for a job ComfyUI had stopped
 * (MCP-SURFACE 21), and a warning that fired on every healthy generation
 * (22). Neither was visible to `tsc`, to oxlint, or to any test, because the
 * wording was derived in JSX where a DOM-less vitest cannot reach it.
 */

/** Shown in place of the list when nothing has been generated yet. */
export const EMPTY_QUEUE = 'Generations you start will appear here.'

/** The row labels. */
export const QUEUED = 'Queued'
export const RUNNING = 'Running'
export const DONE = 'Done'
export const CANCELLED = 'Cancelled'
export const FAILED = 'Failed'

/**
 * Statuses that mean the job is over.
 *
 * `error` is what a real failure reports; `failed` has never been observed and
 * is carried because dropping an inferred terminal value can only cost a job
 * that polls for ever -- which is exactly what the missing `cancelled` did
 * (MCP-SURFACE 24).
 */
const TERMINAL = new Set(['completed', 'cancelled', 'error', 'failed'])

/** Whether the job is over. */
export function isDone(job: Job): boolean {
  return TERMINAL.has(job.status)
}

/**
 * Whether to offer Cancel.
 *
 * Derived from [`TERMINAL`] rather than listing the live statuses, because two
 * lists of statuses drift: a new terminal value added to one would leave a
 * finished job offering a Cancel button that does nothing.
 */
export function canCancel(job: Job): boolean {
  return !isDone(job)
}

/**
 * The row's status word.
 *
 * An unrecognised status reads as [`RUNNING`], not as itself and not as blank.
 * `status` carries whatever ComfyUI said, this project has already been wrong
 * once about that vocabulary, and of the three options -- guess "running",
 * echo a raw token, or show nothing -- only the first is both honest and
 * useful: the job is not in any state we know to be terminal, so it is still
 * going as far as anyone here can tell.
 */
export function statusLabel(job: Job): string {
  switch (job.status) {
    case 'queued':
      return QUEUED
    case 'completed':
      return DONE
    case 'cancelled':
      return CANCELLED
    case 'error':
    case 'failed':
      return FAILED
    default:
      return RUNNING
  }
}

/**
 * The error to show, or `null`.
 *
 * **`null` for a cancelled job, always.** A cancel carries
 * `"Job was interrupted/cancelled."` on one of comfy-cli's two error shapes
 * (MCP-SURFACE 24.3), and putting that under a red "Failed" heading reports
 * the user's own decision back to them as a fault -- the defect section 21 was
 * written to fix, returning through the front door.
 */
export function errorFor(job: Job): string | null {
  if (job.status === 'cancelled') return null
  return job.error
}

/** `"12s"`, `"1m 12s"`. */
export function elapsed(job: Job, now: number): string {
  const seconds = Math.max(0, Math.floor((now - job.submittedAt) / 1000))
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}

/** One row of the queue, with every decision already made. */
export interface QueueRow {
  id: string
  label: string
  profileId: string
  elapsed: string
  error: string | null
  canCancel: boolean
  /** For the row's CSS class, so the view composes no logic of its own. */
  status: string
}

/**
 * The rows, ordered.
 *
 * **Live jobs first, then newest first within each group.** Ordering by time
 * alone buries the job the user is waiting on underneath the ones that have
 * finished -- and the queue only earns its place on screen when more than one
 * job is in it, which is precisely when that happens.
 *
 * `now` is a parameter rather than a `Date.now()` call inside, so elapsed
 * times are testable and every row in one render agrees on the time.
 */
export function queueRows(jobs: Record<string, Job>, now: number): QueueRow[] {
  return Object.values(jobs)
    .toSorted((a, b) => {
      const live = Number(isDone(a)) - Number(isDone(b))
      if (live !== 0) return live
      return b.submittedAt - a.submittedAt
    })
    .map((job) => ({
      id: job.id,
      label: statusLabel(job),
      profileId: job.profileId,
      elapsed: elapsed(job, now),
      error: errorFor(job),
      canCancel: canCancel(job),
      status: job.status,
    }))
}
