import { create } from 'zustand'
import {
  cancelJob,
  connectComfy,
  isTauri,
  runWorkflow,
  subscribeJobs,
  type JobEvent,
} from '../bridge/jobs'

/** A generation job tracked by the queue. */
export interface Job {
  id: string
  status: string // 'queued' | 'running' | 'completed' | 'failed'
  outputs: string[]
  error: string | null
}

/**
 * Fold one job event into the job map.
 *
 * Pure so it can be tested without a store or a Tauri bridge: it maps
 * `progress`/`done`/`failed` to a job's status and payload. An event for an
 * unknown id is ignored and the same map is returned unchanged.
 */
export function applyJobEvent(jobs: Record<string, Job>, event: JobEvent): Record<string, Job> {
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
        [event.payload.id]: { ...job, status: 'completed', outputs: event.payload.outputs },
      }
    case 'failed':
      return {
        ...jobs,
        [event.payload.id]: { ...job, status: 'failed', error: event.payload.error },
      }
  }
}

interface JobsState {
  jobs: Record<string, Job>
  connected: boolean
  listening: boolean
  connect: (bin?: string) => Promise<void>
  run: (workflowPath: string) => Promise<string>
  register: (id: string) => void
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
      jobs: { ...state.jobs, [id]: { id, status: 'queued', outputs: [], error: null } },
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
  register: (id) => {
    if (get().jobs[id] !== undefined) return
    set((state) => ({
      jobs: { ...state.jobs, [id]: { id, status: 'queued', outputs: [], error: null } },
    }))
  },

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
