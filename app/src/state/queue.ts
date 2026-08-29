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

/**
 * The model column when nothing can name the profile.
 *
 * Two ways to get here, and they are deliberately not distinguished on screen:
 * a job submitted through `run(workflowPath)`, which carries no profile at all,
 * and a profile id the models list cannot name -- an uninstalled or renamed
 * profile, or simply a row rendered before the list has loaded. Neither is
 * worth its own sentence in a queue row.
 */
export const UNKNOWN_MODEL = 'Unknown model'

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
    // `pending` is what the *server* calls a job waiting its turn; `queued` is
    // what the *submit response* calls it and what this app sets locally before
    // the first poll. Both are the same state to a person, so both get the same
    // word. Measured live 2026-08-29 (MCP-SURFACE 25) -- until then `pending`
    // fell through to the default below and every waiting job read "Running",
    // including jobs that had not touched the GPU.
    case 'pending':
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

/**
 * `"12s"`, `"1m 12s"`.
 *
 * **A finished job's clock stops.** Once `finishedAt` is stamped it replaces
 * `now`, so the label freezes at the duration the job actually took instead of
 * counting on for ever -- which is what a cancelled row did for twenty minutes
 * while a producer watched it.
 *
 * `??` and not `||`: a `finishedAt` of 0 is a real instant, and `||` would
 * discard it. The same absent-versus-empty confusion has now cost this project
 * four separate bugs.
 */
export function elapsed(job: Job, now: number): string {
  const end = job.finishedAt ?? now
  const seconds = Math.max(0, Math.floor((end - job.submittedAt) / 1000))
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}

/** One row of the queue, with every decision already made. */
export interface QueueRow {
  id: string
  label: string
  /** The profile's display name -- never the raw id. */
  model: string
  profileId: string
  elapsed: string
  error: string | null
  canCancel: boolean
  /** For the row's CSS class, so the view composes no logic of its own. */
  status: string
}

/**
 * What a row calls the model that produced it.
 *
 * Falls back to the **id** rather than to [`UNKNOWN_MODEL`] when the map simply
 * does not have it: `ace-step-1.5-turbo` is ugly but it is information, and the
 * commonest reason for a miss is the models list not having loaded yet, where
 * blanking the column would make the row look broken for a moment.
 *
 * An **empty** display name counts as a miss, not as a name. `??` guards only
 * `undefined`, so a profile carrying `"display_name": ""` would otherwise blank
 * the column -- which is the one outcome this function exists to prevent.
 */
export function modelName(profileId: string, names: Record<string, string>): string {
  if (profileId.trim() === '') return UNKNOWN_MODEL
  const name = names[profileId]?.trim()
  return name !== undefined && name !== '' ? name : profileId
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
export function queueRows(
  jobs: Record<string, Job>,
  now: number,
  names: Record<string, string> = {},
): QueueRow[] {
  return Object.values(jobs)
    .toSorted((a, b) => {
      const live = Number(isDone(a)) - Number(isDone(b))
      if (live !== 0) return live
      return b.submittedAt - a.submittedAt
    })
    .map((job) => ({
      id: job.id,
      label: statusLabel(job),
      model: modelName(job.profileId, names),
      profileId: job.profileId,
      elapsed: elapsed(job, now),
      error: errorFor(job),
      canCancel: canCancel(job),
      status: job.status,
    }))
}
