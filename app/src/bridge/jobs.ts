import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/** Mirrors Rust `src-tauri/src/jobs.rs` `JobProgress` (snake_case on the wire). */
export interface JobProgress {
  id: string
  status: string
  outputs: string[]
}

/** Mirrors Rust `JobDone`. */
export interface JobDone {
  id: string
  outputs: string[]
}

/** Mirrors Rust `JobFailed`. */
export interface JobFailed {
  id: string
  error: string
}

/** Mirrors Rust `JobCancelled`: stopped on purpose, not a failure. */
export interface JobCancelled {
  id: string
}

/**
 * What `job(action="cancel")` managed. Mirrors Rust `mcp_bridge::JobCancel`.
 *
 * Reported rather than discarded: a cancel that found nothing and a cancel that
 * stopped a run used to be indistinguishable, because the command returned
 * `Ok(())` either way.
 */
export interface JobCancel {
  found: boolean
  queue_delete_ok: boolean
  interrupt_ok: boolean
}

/** One event from the job pump, tagged so the store can switch on it. */
export type JobEvent =
  | { kind: 'progress'; payload: JobProgress }
  | { kind: 'done'; payload: JobDone }
  | { kind: 'cancelled'; payload: JobCancelled }
  | { kind: 'failed'; payload: JobFailed }

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** Connect the backend to `comfy-mcp`. Defaults to `comfy-mcp` on PATH. */
export async function connectComfy(bin?: string): Promise<void> {
  await invoke('connect_comfy', { bin })
}

/** Submit a workflow and return its prompt id. */
export async function runWorkflow(workflowPath: string): Promise<string> {
  return await invoke<string>('run_workflow', { workflowPath })
}

/**
 * Ask ComfyUI to stop a job.
 *
 * The job's own `job://cancelled` event is what settles its row -- this only
 * reports what the cancel call itself managed.
 */
export async function cancelJob(id: string): Promise<JobCancel> {
  return await invoke<JobCancel>('cancel_job', { id })
}

/**
 * Subscribe to all four job events, dispatching each as a tagged [`JobEvent`].
 * Returns the unsubscribe function.
 */
export async function subscribeJobs(onEvent: (event: JobEvent) => void): Promise<UnlistenFn> {
  const unprogress = await listen<JobProgress>('job://progress', (event) => {
    onEvent({ kind: 'progress', payload: event.payload })
  })
  const undone = await listen<JobDone>('job://done', (event) => {
    onEvent({ kind: 'done', payload: event.payload })
  })
  const uncancelled = await listen<JobCancelled>('job://cancelled', (event) => {
    onEvent({ kind: 'cancelled', payload: event.payload })
  })
  const unfailed = await listen<JobFailed>('job://failed', (event) => {
    onEvent({ kind: 'failed', payload: event.payload })
  })
  return () => {
    unprogress()
    undone()
    uncancelled()
    unfailed()
  }
}
