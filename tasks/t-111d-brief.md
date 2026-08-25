# T-111d: the models bridge and store
**Depends:** T-111b, T-111c | **Crate/dir:** `app/src`
**Files to create:**
- `app/src/bridge/models.ts`
- `app/src/state/models.ts`
- `app/src/state/models.test.ts`

## Goal
Mirror the backend's tagged unions, and turn each state into words the user can act on. All
the wording logic is pure functions, so the whole step is testable without a Tauri bridge or a
running ComfyUI.

## Spec
Exactly the reference implementation below.

**The union tags must match the Rust exactly.** `Readiness` is
`#[serde(tag = "state", rename_all = "snake_case")]`, so the tags are `ready`, `missing`,
`undeclared`, `unknown`. A rename on either side silently breaks every branch.

**Four rules that are correctness, not wording taste:**

- **`unknown` and `undeclared` must never read as "not installed".** They are neutral, they
  get no Install button, and they list no files. ACE-Step is an 18.5 GiB download; a user who
  already has it must not be sent to fetch it again because their ComfyUI is stopped. The
  sweep test enforces this over every state.
- **The size is stated before the user commits.** `4 files (18.5 GiB)`, from the profile's
  declared sizes.
- **An unknown total is omitted, not guessed.** `total_bytes` is null when any file lacks a
  declared size; a partial sum shown as the whole understates the cost.
- **Progress is weighted by bytes, not by file count.** ACE-Step's four files run from 0.3 to
  9.3 GiB, so "1 of 4 done" is anywhere between 2% and 50%. A file-counted bar sits at 25%
  through most of a 9 GiB transfer.

**`isTerminal` is deliberately narrow.** Only `completed` and `failed` stop the poll.
`unknown` is what a failed *status call* reports (T-111c), and treating it as terminal would
declare a 9 GiB transfer finished because one poll timed out. Note that `completed` itself is
**inferred, not verified** -- MCP-SURFACE 11.3 records that no real download has been watched
to the end, which T-113 will finally do.

**`install` is never called on load.** It runs from the button only, and the store refuses a
second install while one is in flight.

## Reference implementation

### `app/src/bridge/models.ts` (create)
```ts
import { invoke } from '@tauri-apps/api/core'

/** One model file the user does not have. Mirrors Rust `MissingFile`. */
export interface MissingFile {
  file: string
  folder: string
  /** Null means the app cannot fetch it and the user must place it by hand. */
  source_url: string | null
  size_bytes: number | null
  license: string | null
}

/**
 * Mirrors Rust `src-tauri/src/models.rs` `Readiness`, a serde-tagged union
 * (`#[serde(tag = "state", rename_all = "snake_case")]`).
 *
 * `undeclared` and `unknown` are both "we could not check", kept apart because
 * they have different fixes: one is a profile that never listed its files, the
 * other is a ComfyUI that is not running. Neither is `ready`, and neither may
 * be rendered as "not installed" -- ACE-Step is an 18.5 GiB download.
 */
export type Readiness =
  | { state: 'ready' }
  | {
      state: 'missing'
      files: MissingFile[]
      /** Null when any missing file has no declared size. */
      total_bytes: number | null
      /** True only when every missing file carries a URL. */
      installable: boolean
    }
  | { state: 'undeclared' }
  | { state: 'unknown' }

/** Where a profile was read from. */
export type ProfileSource = 'shipped' | 'user'

/** One row of the models step. Mirrors Rust `ProfileStatus`. */
export interface ProfileStatus {
  id: string
  display_name: string
  kind: 'music' | 'image'
  /** Shown wherever the model is chosen or installed (CONVENTIONS). */
  license: string
  license_notes: string | null
  source: ProfileSource
  vram_gb_min: number | null
  readiness: Readiness
}

/** What the models step shows. Mirrors Rust `ModelsView`. */
export interface ModelsView {
  profiles: ProfileStatus[]
  warnings: unknown[]
  inventory_available: boolean
  inventory_detail: string | null
}

/**
 * Report every known profile and whether its models are installed.
 *
 * Rejects only when the app itself fails. A stopped ComfyUI comes back as
 * `inventory_available: false` with every row `unknown`.
 */
export async function modelsStatus(bin?: string): Promise<ModelsView> {
  return await invoke<ModelsView>('models_status', { bin })
}

/** One file's download, once submitted. Mirrors Rust `StartedFile`. */
export interface StartedFile {
  file: string
  download_id: string | null
  error: string | null
}

/** Progress for one file. Mirrors Rust `FileProgress`. */
export interface FileProgress {
  download_id: string
  /** `starting` | `downloading` | `completed` | `failed` | `unknown`. */
  status: string
  completed_bytes: number | null
  total_bytes: number | null
  percent: number | null
  error: string | null
}

/**
 * Start downloading everything a profile is missing.
 *
 * **Only from an explicit user action.** ACE-Step 1.5 is 18.5 GiB across four
 * files. Each file is reported separately, so a partial start is visible.
 */
export async function modelsInstall(id: string, bin?: string): Promise<StartedFile[]> {
  return await invoke<StartedFile[]>('models_install', { id, bin })
}

/** Poll every in-flight download in one round trip. */
export async function modelsProgress(ids: string[], bin?: string): Promise<FileProgress[]> {
  return await invoke<FileProgress[]>('models_progress', { ids, bin })
}
```

### `app/src/state/models.ts` (create)
```ts
import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import {
  modelsInstall,
  modelsProgress,
  modelsStatus,
  type FileProgress,
  type ModelsView,
  type ProfileStatus,
  type Readiness,
} from '../bridge/models'

/** How one model row should read. */
export interface RowView {
  /** Drives the pill's colour class. */
  tone: 'ok' | 'warn' | 'neutral'
  label: string
  /** One sentence saying what to do next, or null when nothing is needed. */
  nextStep: string | null
  /** The download this row would start, when it is one the app can start. */
  download: string | null
}

/**
 * Bytes as a short human string, or null when unknown.
 *
 * GiB because that is what every model host shows. Unknown stays unknown: a
 * total assembled from some of the files understates a download that is
 * already large enough to matter.
 */
export function formatBytes(bytes: number | null): string | null {
  if (bytes === null || bytes <= 0) return null
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(0)} MiB`
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB`
}

/**
 * Map one profile's readiness to what the user sees.
 *
 * Pure, so the wording of every state is testable without a Tauri bridge or a
 * running ComfyUI. CONVENTIONS requires user-facing errors to say what to do
 * next; that rule is enforced here by `nextStep` being non-null for every
 * state that is not `ready`.
 */
export function rowFor(readiness: Readiness): RowView {
  switch (readiness.state) {
    case 'ready':
      return { tone: 'ok', label: 'Installed', nextStep: null, download: null }
    case 'missing': {
      const size = formatBytes(readiness.total_bytes)
      const count = readiness.files.length
      const files = `${count} file${count === 1 ? '' : 's'}`
      return {
        tone: 'warn',
        label: 'Not installed',
        nextStep: readiness.installable
          ? `Install to download ${files}${size === null ? '' : ` (${size})`}.`
          : `Place ${files} in your ComfyUI models folder by hand, then Retry.`,
        download: readiness.installable ? size : null,
      }
    }
    case 'undeclared':
      return {
        tone: 'neutral',
        label: 'Cannot check',
        nextStep: 'This profile does not list the model files it needs.',
        download: null,
      }
    case 'unknown':
      return {
        tone: 'neutral',
        label: 'Cannot check',
        nextStep: 'Start ComfyUI on the step above, then Retry.',
        download: null,
      }
  }
}

/**
 * Curated models first.
 *
 * Shipped profiles are the ones this app has verified against a real install;
 * a user's own profile is theirs and appears after. Within a group, ready
 * models come first -- a user who has something working should see it without
 * reading past what they have not installed.
 */
export function curatedFirst(profiles: ProfileStatus[]): ProfileStatus[] {
  return profiles.toSorted(
    (a, b) => rank(a) - rank(b) || a.display_name.localeCompare(b.display_name),
  )
}

/** Sort key for [`curatedFirst`]: shipped before user, ready before not. */
function rank(profile: ProfileStatus): number {
  return (profile.source === 'shipped' ? 0 : 2) + (profile.readiness.state === 'ready' ? 0 : 1)
}

/**
 * Whether a download has stopped moving.
 *
 * `completed` is comfy-cli's own success value but is **inferred, not
 * verified** (MCP-SURFACE 11.3 -- the capture run failed on purpose and no real
 * download has been watched to the end). `failed` is verified. Anything else is
 * treated as still running, which errs towards polling one tick too long
 * rather than declaring a half-finished 9 GiB file done.
 */
export function isTerminal(status: string): boolean {
  return status === 'completed' || status === 'failed'
}

/** How an install in flight should read. */
export interface InstallView {
  done: number
  total: number
  /** Overall percent across every file, or null while any size is unknown. */
  percent: number | null
  /** Files that failed, by download id. */
  failed: FileProgress[]
}

/**
 * Summarise an install in flight.
 *
 * The percentage is byte-weighted rather than file-weighted: ACE-Step's four
 * files run from 0.3 GiB to 9.3 GiB, so "1 of 4 done" can mean anywhere between
 * 2% and 50% of the transfer.
 */
export function installView(progress: FileProgress[]): InstallView {
  const done = progress.filter((p) => p.status === 'completed').length
  const failed = progress.filter((p) => p.status === 'failed')
  const sized = progress.filter((p) => p.total_bytes !== null && p.total_bytes > 0)
  let percent: number | null = null
  if (sized.length === progress.length && progress.length > 0) {
    const total = sized.reduce((sum, p) => sum + (p.total_bytes ?? 0), 0)
    const got = sized.reduce((sum, p) => sum + (p.completed_bytes ?? 0), 0)
    percent = total > 0 ? Math.round((got / total) * 100) : null
  }
  return { done, total: progress.length, percent, failed }
}

interface ModelsState {
  view: ModelsView | null
  busy: boolean
  /** Profile currently downloading, or null. */
  installing: string | null
  progress: FileProgress[]
  refresh: () => Promise<void>
  install: (id: string) => Promise<void>
}

/** How often an in-flight download is polled. */
const POLL_MS = 2000

export const useModelsStore = create<ModelsState>((set, get) => ({
  view: null,
  busy: false,
  installing: null,
  progress: [],

  refresh: async () => {
    if (!isTauri()) return
    set({ busy: true })
    try {
      set({ view: await modelsStatus() })
    } finally {
      set({ busy: false })
    }
  },

  /**
   * Download everything one profile is missing.
   *
   * Called only from the Install button, never on load: this starts a transfer
   * measured in gigabytes.
   */
  install: async (id: string) => {
    if (!isTauri() || get().installing !== null) return
    set({ installing: id, progress: [] })
    try {
      const started = await modelsInstall(id)
      const ids = started
        .map((s) => s.download_id)
        .filter((d): d is string => d !== null)
      if (ids.length === 0) return

      let progress = await modelsProgress(ids)
      set({ progress })
      while (!progress.every((p) => isTerminal(p.status))) {
        await new Promise((resolve) => setTimeout(resolve, POLL_MS))
        progress = await modelsProgress(ids)
        set({ progress })
      }
    } finally {
      set({ installing: null })
      await get().refresh()
    }
  },
}))
```

### `app/src/state/models.test.ts` (create)
```ts
import { describe, expect, it } from 'vitest'
import type { FileProgress, ProfileStatus, Readiness } from '../bridge/models'
import { curatedFirst, formatBytes, installView, isTerminal, rowFor } from './models'

/** The four ACE-Step files, as captured from Hugging Face on 2026-08-25. */
const ACE_FILES = [
  { file: 'acestep_v1.5_xl_turbo_bf16.safetensors', folder: 'diffusion_models', size_bytes: 9974719892 },
  { file: 'qwen_0.6b_ace15.safetensors', folder: 'text_encoders', size_bytes: 1191588248 },
  { file: 'qwen_4b_ace15.safetensors', folder: 'text_encoders', size_bytes: 8379154232 },
  { file: 'ace_1.5_vae.safetensors', folder: 'vae', size_bytes: 337431732 },
].map((f) => ({ ...f, source_url: `https://example.invalid/${f.file}`, license: null }))

/** Every state the backend can report, so the sweep below cannot go stale. */
const ALL_STATES: Readiness[] = [
  { state: 'ready' },
  { state: 'missing', files: ACE_FILES, total_bytes: 19882894104, installable: true },
  { state: 'undeclared' },
  { state: 'unknown' },
]

function profile(over: Partial<ProfileStatus>): ProfileStatus {
  return {
    id: 'x',
    display_name: 'X',
    kind: 'music',
    license: 'Apache-2.0',
    license_notes: null,
    source: 'shipped',
    vram_gb_min: null,
    readiness: { state: 'ready' },
    ...over,
  }
}

describe('rowFor', () => {
  /**
   * Protects a product rule, not a rendering detail: CONVENTIONS requires
   * user-facing errors to say what to do next. Adding a state without a next
   * step fails here.
   */
  it('gives every state that is not ready a next step', () => {
    for (const readiness of ALL_STATES) {
      const row = rowFor(readiness)
      if (readiness.state === 'ready') {
        expect(row.nextStep).toBeNull()
        expect(row.tone).toBe('ok')
      } else {
        expect(row.nextStep, `${readiness.state} must say what to do next`).not.toBeNull()
        expect(row.nextStep).not.toBe('')
      }
    }
  })

  /**
   * Protects the most damaging confusion this step could make. A stopped
   * ComfyUI must never read as "not installed": ACE-Step is an 18.5 GiB
   * download, and a user who already has it must not be sent to fetch it again
   * because their server happens to be off.
   */
  it('never presents an uncheckable state as not installed', () => {
    for (const readiness of [{ state: 'unknown' } as const, { state: 'undeclared' } as const]) {
      const row = rowFor(readiness)
      expect(row.label).not.toContain('Not installed')
      expect(row.tone).toBe('neutral')
      expect(row.download).toBeNull()
    }
  })

  /** Protects: a stopped ComfyUI points at the step that fixes it. */
  it('sends an unknown row back to the ComfyUI step', () => {
    expect(rowFor({ state: 'unknown' }).nextStep).toContain('Start ComfyUI')
  })

  /**
   * Protects: the size reaches the user before they commit to the download.
   * 18.5 GiB is not something to start without being told.
   */
  it('states the download size and file count before installing', () => {
    const row = rowFor({
      state: 'missing',
      files: ACE_FILES,
      total_bytes: 19882894104,
      installable: true,
    })
    expect(row.nextStep).toContain('4 files')
    expect(row.nextStep).toContain('18.5 GiB')
    expect(row.tone).toBe('warn')
  })

  /**
   * Protects: an unknown total is omitted rather than guessed at. Showing a
   * partial sum as if it were the whole download understates the cost.
   */
  it('omits the size when the total is unknown', () => {
    const row = rowFor({ state: 'missing', files: ACE_FILES, total_bytes: null, installable: true })
    expect(row.nextStep).toContain('4 files')
    expect(row.nextStep).not.toContain('GiB')
    expect(row.nextStep).not.toContain('(')
  })

  /**
   * Protects: files the app cannot fetch get hand-placement instructions, not
   * an Install button that would half-work.
   */
  it('asks for hand placement when a file has no source', () => {
    const row = rowFor({
      state: 'missing',
      files: [{ ...ACE_FILES[0], source_url: null }],
      total_bytes: null,
      installable: false,
    })
    expect(row.nextStep).toContain('by hand')
    expect(row.download).toBeNull()
  })
})

describe('formatBytes', () => {
  /** Protects: unknown stays unknown, and zero is not a size. */
  it('returns null for unknown or zero', () => {
    expect(formatBytes(null)).toBeNull()
    expect(formatBytes(0)).toBeNull()
  })

  /** Protects: the units the model hosts themselves use. */
  it('formats the captured sizes', () => {
    expect(formatBytes(19882894104)).toBe('18.5 GiB')
    expect(formatBytes(337431732)).toBe('322 MiB')
  })
})

describe('curatedFirst', () => {
  /**
   * Protects: shipped-and-working first, user profiles last. The order is what
   * makes the step readable at a glance -- a user who has a model installed
   * should not have to read past three they do not.
   */
  it('puts shipped ready profiles first and user profiles last', () => {
    const ordered = curatedFirst([
      profile({ id: 'mine', display_name: 'Mine', source: 'user' }),
      profile({ id: 'ace', display_name: 'ACE', readiness: { state: 'unknown' } }),
      profile({ id: 'minimax', display_name: 'MiniMax' }),
    ])
    expect(ordered.map((p) => p.id)).toEqual(['minimax', 'ace', 'mine'])
  })
})

describe('isTerminal', () => {
  /**
   * Protects: only the two states that actually stop are terminal. `unknown`
   * is what a failed *poll* reports -- treating it as terminal would declare a
   * 9 GiB transfer finished because one status call timed out.
   */
  it('treats only completed and failed as terminal', () => {
    expect(isTerminal('completed')).toBe(true)
    expect(isTerminal('failed')).toBe(true)
    for (const live of ['starting', 'downloading', 'unknown', '']) {
      expect(isTerminal(live), `${live} is still running`).toBe(false)
    }
  })
})

function at(status: string, done: number, total: number | null): FileProgress {
  return {
    download_id: `${status}-${done}`,
    status,
    completed_bytes: done,
    total_bytes: total,
    percent: null,
    error: null,
  }
}

describe('installView', () => {

  /**
   * Protects: progress is weighted by bytes, not by file count. ACE-Step's
   * four files run from 0.3 to 9.3 GiB, so "1 of 4" can mean 2% or 50% -- a
   * file-counted bar would sit at 25% through most of a 9 GiB transfer.
   */
  it('weights the percentage by bytes, not by file count', () => {
    const view = installView([
      at('completed', 337431732, 337431732),
      at('downloading', 0, 9974719892),
    ])
    expect(view.done).toBe(1)
    expect(view.total).toBe(2)
    expect(view.percent).toBe(3)
  })

  /**
   * Protects: an unknown size makes the whole bar unknown rather than wrong.
   * `total_bytes` is null until the server sends a content length, so a bar
   * computed from the files that have reported would leap backwards.
   */
  it('reports no percentage while any size is unknown', () => {
    const view = installView([at('downloading', 10, null), at('downloading', 10, 100)])
    expect(view.percent).toBeNull()
    expect(view.done).toBe(0)
  })

  /** Protects: failures are surfaced, not averaged away into the bar. */
  it('collects failed files', () => {
    const view = installView([at('failed', 0, 100), at('completed', 100, 100)])
    expect(view.failed).toHaveLength(1)
    expect(view.done).toBe(1)
  })
})
```

## Acceptance criteria
- `npm run gate` green, **including zero oxlint warnings** -- `curatedFirst` uses `toSorted`,
  not `sort`, and `rank`/`at` are module-scope functions, both because oxlint's `unicorn`
  rules reject the alternatives.
- vitest goes 28 -> **41** tests across **8** files.
- **No non-ASCII characters anywhere in the diff.**

## Out of scope
The view and the CSS (T-111e). Do not import anything from `views/`.

## If unclear
Follow the reference implementation exactly.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read src-tauri/src/models.rs --read src-tauri/src/install.rs --read app/src/state/comfy.ts --read app/src/bridge/comfy.ts --file app/src/bridge/models.ts --file app/src/state/models.ts --file app/src/state/models.test.ts
```
