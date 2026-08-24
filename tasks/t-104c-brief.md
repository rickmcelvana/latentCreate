# T-104c: frontend jobs bridge + store + queue panel
**Depends:** T-104b | **Crate/dir:** `app/` | **Executor:** Aider

**Files to create:** `app/src/bridge/jobs.ts`, `app/src/bridge/jobs.test.ts`, `app/src/state/jobs.ts`, `app/src/state/jobs.test.ts`, `app/src/components/JobQueue.tsx`

**Files to modify:** `app/src/views/AudioStudio.tsx`, `app/src/theme.css`

> Closes the gap T-104b left: the Rust job pump has no frontend consumer. This brief is the
> typed bridge wrappers, the jobs Zustand store, and a minimal queue panel — the frontend half of
> the generation path. No new Rust; this is the CONVENTIONS "invoke/listen only in `bridge/`" rule
> applied to the job surface.

## Goal
The frontend can `connectComfy` / `runWorkflow` / `cancelJob` through `app/src/bridge/jobs.ts`,
and the `job://progress|done|failed` events stream into a `useJobsStore` queue that a `JobQueue`
component renders in AudioStudio. Mirrors the Rust `JobProgress`/`JobDone`/`JobFailed` payloads
exactly, and every event name is spelled once, in the bridge.

## Verified, not recalled
- `@tauri-apps/api` v2's `listen<T>(event, handler) -> Promise<UnlistenFn>` and the `UnlistenFn`
  type were read from `node_modules/@tauri-apps/api/event.d.ts` — not remembered. The event-name
  charset (alphanumeric + `-` `/` `:` `_`) matches the Rust `is_event_name_valid`, so `job://progress`
  is valid on both sides.
- The reference code **type-checks, lints, and tests green**: `tsc -b` clean, `oxlint` 0 warnings,
  21 vitest tests pass (9 new), `vite build` succeeds. The `vi.mock` pattern in the store test is
  copied from the working `state/config.test.ts` (the `mock`-prefixed hoisted-variable convention).

## Reference code

### `app/src/bridge/jobs.ts` — full file
```ts
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

/** One event from the job pump, tagged so the store can switch on it. */
export type JobEvent =
  | { kind: 'progress'; payload: JobProgress }
  | { kind: 'done'; payload: JobDone }
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

/** Cancel a running job. */
export async function cancelJob(id: string): Promise<void> {
  await invoke('cancel_job', { id })
}

/**
 * Subscribe to all three job events, dispatching each as a tagged [`JobEvent`].
 * Returns the unsubscribe function.
 */
export async function subscribeJobs(onEvent: (event: JobEvent) => void): Promise<UnlistenFn> {
  const unprogress = await listen<JobProgress>('job://progress', (event) => {
    onEvent({ kind: 'progress', payload: event.payload })
  })
  const undone = await listen<JobDone>('job://done', (event) => {
    onEvent({ kind: 'done', payload: event.payload })
  })
  const unfailed = await listen<JobFailed>('job://failed', (event) => {
    onEvent({ kind: 'failed', payload: event.payload })
  })
  return () => {
    unprogress()
    undone()
    unfailed()
  }
}
```

### `app/src/state/jobs.ts` — full file
```ts
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
```

### `app/src/components/JobQueue.tsx` — full file
```tsx
import { useJobsStore, type Job } from '../state/jobs'

/** Renders the active job queue. Empty when nothing has been generated. */
export function JobQueue() {
  const jobs = useJobsStore((state) => state.jobs)
  const cancel = useJobsStore((state) => state.cancel)
  const entries = Object.values(jobs)
  if (entries.length === 0) return null
  return (
    <ul className="job-queue">
      {entries.map((job) => (
        <JobItem key={job.id} job={job} onCancel={cancel} />
      ))}
    </ul>
  )
}

function JobItem({ job, onCancel }: { job: Job; onCancel: (id: string) => void }) {
  const running = job.status !== 'completed' && job.status !== 'failed'
  return (
    <li className={`job-item job-item-${job.status}`}>
      <span className="job-status">{job.status}</span>
      {job.error !== null ? <span className="job-error">{job.error}</span> : null}
      {running ? (
        <button type="button" className="job-cancel" onClick={() => onCancel(job.id)}>
          Cancel
        </button>
      ) : null}
    </li>
  )
}
```

### `app/src/views/AudioStudio.tsx` — full file (replaces the placeholder)
```tsx
import { useEffect } from 'react'
import { JobQueue } from '../components/JobQueue'
import { useJobsStore } from '../state/jobs'

export function AudioStudio() {
  const startListening = useJobsStore((state) => state.startListening)

  useEffect(() => {
    void startListening()
  }, [startListening])

  return (
    <>
      <h1 className="view-title">Audio</h1>
      <p className="view-subtitle">
        Style tags, lyrics, and the settings worth changing.
      </p>
      <JobQueue />
      <div className="panel muted">
        No generations yet. Finish Setup to enable audio.
      </div>
    </>
  )
}
```

### `app/src/theme.css` — append after the `.status-pill-warn` rule
```css
/* --- Job queue (AudioStudio) --- */

.job-queue {
  list-style: none;
  margin: 0 0 var(--gap-lg);
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
}

.job-item {
  display: flex;
  align-items: center;
  gap: var(--gap-md);
  padding: var(--gap-sm) var(--gap-md);
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: var(--radius);
}

.job-status {
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  color: var(--text-muted);
}

.job-item-completed .job-status {
  color: var(--success);
}

.job-item-failed .job-status {
  color: var(--danger);
}

.job-error {
  color: var(--danger);
  font-size: 12px;
}

.job-cancel {
  margin-left: auto;
  padding: var(--gap-xs) var(--gap-sm);
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  transition: color var(--transition), border-color var(--transition);
}

.job-cancel:hover {
  color: var(--danger);
  border-color: var(--danger);
}
```

### `app/src/bridge/jobs.test.ts` — full file
```ts
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
```

### `app/src/state/jobs.test.ts` — full file
```ts
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
```

## Tests
Nine new tests, none needing a live ComfyUI or a Tauri window (the bridge is mocked at the
module boundary, exactly as `state/config.test.ts` does).

- `test_subscribe_jobs_registers_the_three_event_names` — **protects:** the three event names are
  spelled exactly once and exactly right. A misspelled `job://progress` would never fire, and the
  Rust side would emit into the void — the same silent-failure class the Rust arg-name tests guard.
- `test_subscribe_jobs_dispatches_tagged_events` — **protects:** each handler dispatches the right
  `kind` tag with its payload, so the store can switch on it.
- `test_run_adds_a_queued_job` — **protects:** `run` adds the job to the queue only after the
  backend accepts it (a failed run adds nothing).
- `test_connect_sets_connected` / `test_cancel_calls_the_backend` — the two one-line command
  wrappers reach the bridge.
- `test_start_listening_subscribes_once` — **protects:** idempotence; a second `startListening` must
  not stack a second set of listeners.
- `test_start_listening_is_skipped_outside_tauri` — **protects:** the browser-preview guard
  (mirrors the config store's `unavailable` behavior).
- `test_apply_event_folds_progress_done_failed` — **protects:** the mapping — `progress` carries the
  status through, `done` → `completed` + outputs, `failed` → `failed` + error.
- `test_apply_event_ignores_unknown_ids` — **protects:** an event for an id the store does not know
  is a no-op returning the same reference, so the queue never fabricates a job from a stray event.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] All nine named tests present and passing; the existing 12 vitest tests still pass
- [ ] `tsc -b` and `oxlint` clean
- [ ] Every new `className` has a rule in `theme.css`
- [ ] `invoke`/`listen` appear **only** in `app/src/bridge/jobs.ts`
- [ ] No changes outside the seven listed files
- [ ] No new dependency

## Out of scope
A "Generate" button that actually produces a workflow — the run trigger needs the §7 pipeline
(fetch_template → set_slots) and the profile loader (T-107), so the queue stays empty until then.
`connect` UI (the setup wizard, T-110). The library import of `job://done` outputs. The
`ComfyBackend` trait (deferred).

## Notes for the executor
- Follow the `state/config.test.ts` `vi.mock` pattern exactly: `mock`-prefixed `vi.fn()` variables
  declared *above* `vi.mock`, referenced by the hoisted factory. Do not name them without the
  `mock` prefix — vitest hoists `vi.mock` and the factory cannot see un-prefixed variables.
- `JobQueue` must select `state.jobs` (a stable reference) and call `Object.values` in the
  component body — never `Object.values(state.jobs)` inside the selector, which would return a new
  array per call and re-render on every unrelated store change (WORKFLOW §4.10).
- Mirror the Rust field names exactly: `id`, `status`, `outputs`, `error` — snake_case on the wire
  because the Rust structs use serde's default. Do not camelCase them.
- `type JobEvent` is imported with the inline `type` modifier (`verbatimModuleSyntax`).
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
The patterns this mirrors are `--read`: the config bridge/store/tests, and the Rust job payloads.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/bridge/config.ts --read app/src/bridge/shell.ts --read app/src/state/config.ts --read app/src/state/config.test.ts --read app/src/state/nav.ts --read src-tauri/src/jobs.rs --file app/src/bridge/jobs.ts --file app/src/bridge/jobs.test.ts --file app/src/state/jobs.ts --file app/src/state/jobs.test.ts --file app/src/components/JobQueue.tsx --file app/src/views/AudioStudio.tsx --file app/src/theme.css
```
