import { create } from 'zustand'
import {
  cancelJob,
  connectComfy,
  isTauri,
  runWorkflow,
  subscribeJobs,
  type JobEvent,
} from '../bridge/jobs'

/**
 * What a row shows when nothing told it which model ran.
 *
 * `run(workflowPath)` submits a bare workflow with no profile behind it -- it
 * predates the Generate button and is not on the generation path.
 */
export const UNKNOWN_PROFILE = ''

/**
 * A generation job tracked by the queue.
 *
 * `status` is a bare `string` on purpose: it carries whatever ComfyUI said, and
 * this project has already been caught once assuming that vocabulary was known
 * (`cancelled` was missing from the terminal set until a producer found it).
 * `state/queue.ts` maps it to a label and treats anything unrecognised as still
 * running rather than rendering a blank row.
 *
 * Measured live 2026-08-29 by queuing two jobs at once (MCP-SURFACE 25):
 * the status poll says `pending` for a job waiting its turn and `running` for
 * the one on the GPU. **`queued` is not a poll value at all** -- it is what the
 * submit response says, and what [`newJob`] sets locally before the first poll
 * lands. Terminal: `completed`, `cancelled`, `error`; `failed` is inferred and
 * has never been seen (MCP-SURFACE 24). The list above this line used to say
 * `queued` and omit `pending`, which is how every waiting job came to read
 * "Running".
 */
export interface Job {
  id: string
  status: string
  outputs: string[]
  error: string | null
  /** The profile this was generated for, so a row can say which model it is. */
  profileId: string
  /**
   * `Date.now()` when this store first heard of the job.
   *
   * Local rather than server-sourced: `submitted_at` exists only on the record
   * comfy-cli keeps a state file for and is absent from the other store
   * (MCP-SURFACE 23.3), so a server timestamp would be present for some rows
   * and missing for others with nothing on screen explaining why.
   */
  submittedAt: number
  /**
   * `Date.now()` when a terminal event arrived, or `null` while the job runs.
   *
   * The elapsed label needs an end as well as a start. Without this the clock
   * ticks for ever on a finished row -- a producer watched a cancelled job pass
   * twenty minutes. Stamped locally for the same reason as [`Job.submittedAt`]:
   * the server timestamp is absent from one of comfy-cli's two stores
   * (MCP-SURFACE 23.3), so half the rows would have frozen and half would not.
   */
  finishedAt: number | null
}

/** A job as it starts life, before the pump has said anything about it. */
function newJob(id: string, profileId: string, now: number): Job {
  return {
    id,
    status: 'queued',
    outputs: [],
    error: null,
    profileId,
    submittedAt: now,
    finishedAt: null,
  }
}

/**
 * Fold one job event into the job map.
 *
 * Pure so it can be tested without a store or a Tauri bridge: it maps
 * `progress`/`done`/`failed` to a job's status and payload. An event for an
 * unknown id is ignored and the same map is returned unchanged.
 *
 * `now` is a parameter, defaulted, for the same reason `queueRows` takes one:
 * a `Date.now()` buried in here would make the stamped finish time untestable.
 *
 * **Every terminal event stamps `finishedAt`.** The three cases below are the
 * complete set: `monitor_job` matches exhaustively on `TerminalOutcome` and
 * emits exactly one of done/cancelled/failed for every ending it has, a poll
 * error included. A terminal status can therefore never arrive as `progress`,
 * which is what makes the stamp complete rather than best-effort.
 */
export function applyJobEvent(
  jobs: Record<string, Job>,
  event: JobEvent,
  now: number = Date.now(),
): Record<string, Job> {
  const job = jobs[event.payload.id]
  if (!job) return jobs
  switch (event.kind) {
    case 'progress':
      return {
        ...jobs,
        [event.payload.id]: { ...job, status: event.payload.status, outputs: event.payload.outputs },
      }
    case 'done':
      return {
        ...jobs,
        [event.payload.id]: {
          ...job,
          status: 'completed',
          outputs: event.payload.outputs,
          finishedAt: now,
        },
      }
    case 'cancelled':
      // Not `failed`: nothing went wrong, and an error string here would
      // report the user's own decision back to them as a fault.
      return { ...jobs, [event.payload.id]: { ...job, status: 'cancelled', finishedAt: now } }
    case 'failed':
      return {
        ...jobs,
        [event.payload.id]: {
          ...job,
          status: 'failed',
          error: event.payload.error,
          finishedAt: now,
        },
      }
  }
}

interface JobsState {
  jobs: Record<string, Job>
  connected: boolean
  listening: boolean
  connect: (bin?: string) => Promise<void>
  run: (workflowPath: string) => Promise<string>
  register: (id: string, profileId: string) => void
  cancel: (id: string) => Promise<void>
  startListening: () => Promise<void>
}

export const useJobsStore = create<JobsState>((set, get) => ({
  jobs: {},
  connected: false,
  listening: false,

  connect: async (bin) => {
    await connectComfy(bin)
    set({ connected: true })
  },

  run: async (workflowPath) => {
    const id = await runWorkflow(workflowPath)
    set((state) => ({
      jobs: { ...state.jobs, [id]: newJob(id, UNKNOWN_PROFILE, Date.now()) },
    }))
    return id
  },

  /**
   * Take ownership of a job someone else submitted.
   *
   * `generate_audio` submits and starts the pump itself, so nothing goes
   * through [`run`] on that path -- and [`applyJobEvent`] ignores events for
   * ids this store does not know, which it must, or a foreign job would invent
   * an entry. Without this call the two are correct separately and deaf
   * together: the generation runs to completion on the GPU and every progress,
   * done and failed event is discarded, leaving an empty queue and no error.
   *
   * Registering an id twice keeps the entry that is already there, so a
   * re-register cannot reset a job that has started reporting.
   */
  register: (id, profileId) => {
    if (get().jobs[id] !== undefined) return
    set((state) => ({
      jobs: { ...state.jobs, [id]: newJob(id, profileId, Date.now()) },
    }))
  },

  /**
   * Ask ComfyUI to stop a job.
   *
   * The row is **not** touched here. The pump sees the cancellation on its next
   * poll and emits `job://cancelled`, which is what settles the row -- and if
   * the cancel does not take, the pump keeps reporting the job that is still
   * running, which is the truth. Marking it cancelled optimistically would put
   * the lie back in a different place.
   */
  cancel: async (id) => {
    await cancelJob(id)
  },

  startListening: async () => {
    if (get().listening) return
    if (!isTauri()) return
    set({ listening: true })
    await subscribeJobs((event) => set((state) => ({ jobs: applyJobEvent(state.jobs, event) })))
  },
}))
