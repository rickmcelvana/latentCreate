import { create } from 'zustand'
import { comfyLaunch, comfyStatus, isTauri, type ComfyStatus } from '../bridge/comfy'

/** How the pill should read for a given status. */
export interface PillView {
  /** `ok` | `warn` | `neutral` -- drives the pill's colour class. */
  tone: 'ok' | 'warn' | 'neutral'
  label: string
  /** One sentence saying what to do next, or null when nothing is needed. */
  nextStep: string | null
}

/**
 * Map a status to what the user sees.
 *
 * Pure, so the wording of every degraded state is testable without a Tauri
 * bridge or a running ComfyUI. CONVENTIONS requires user-facing errors to say
 * what to do next, not just what failed -- that rule is enforced here by
 * `nextStep` being non-null for every state that is not `ready`.
 */
export function pillFor(status: ComfyStatus | null): PillView {
  if (status === null) {
    return { tone: 'neutral', label: 'Checking ComfyUI...', nextStep: null }
  }
  switch (status.state) {
    case 'not_installed':
      return {
        tone: 'warn',
        label: 'comfy-mcp not found',
        nextStep: `Run ${status.install_command}, then Retry.`,
      }
    case 'unreachable':
      return {
        tone: 'warn',
        label: 'ComfyUI unreachable',
        nextStep: `${status.detail} Check the install, then Retry.`,
      }
    case 'server_down':
      return {
        tone: 'warn',
        label: 'ComfyUI is not running',
        nextStep: 'Start ComfyUI, then Retry.',
      }
    case 'ready':
      return {
        tone: 'ok',
        label: status.url === null ? 'ComfyUI ready' : `ComfyUI ready at ${status.url}`,
        nextStep: null,
      }
  }
}

/**
 * Total VRAM as a short human string, or null when it is unknown.
 *
 * Unknown stays unknown: comfy-cli does not always report a GPU, and showing
 * `0 GB` on a working machine reads as a broken app. GiB, because that is what
 * every GPU tool shows -- 17102733312 bytes is a "16 GB" card at 15.9 GiB.
 */
export function formatVram(bytes: number | null): string | null {
  if (bytes === null || bytes <= 0) return null
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB VRAM`
}

interface ComfyState {
  status: ComfyStatus | null
  /** True while a status or launch call is in flight. */
  busy: boolean
  refresh: () => Promise<void>
  launch: () => Promise<void>
}

export const useComfyStore = create<ComfyState>((set) => ({
  status: null,
  busy: false,

  refresh: async () => {
    if (!isTauri()) return
    set({ busy: true })
    try {
      set({ status: await comfyStatus() })
    } finally {
      set({ busy: false })
    }
  },

  launch: async () => {
    if (!isTauri()) return
    set({ busy: true })
    try {
      set({ status: await comfyLaunch() })
    } finally {
      set({ busy: false })
    }
  },
}))
