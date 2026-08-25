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
