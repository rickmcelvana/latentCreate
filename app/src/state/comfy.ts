import { create } from 'zustand'
import { comfyStatus, isTauri, type ComfyStatus } from '../bridge/comfy'

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
    case 'not_found':
      return {
        tone: 'warn',
        label: 'ComfyUI not found',
        nextStep: status.configured
          ? `Nothing answered at ${status.tried.join(' or ')}. Check that ComfyUI is running there and the address is right, then Retry.`
          : `Nothing answered at ${status.tried.join(' or ')}. Start ComfyUI, then Retry -- or enter its address below.`,
      }
    case 'unreachable':
      return {
        tone: 'warn',
        label: 'ComfyUI unreachable',
        nextStep: `${status.detail} Check the address, then Retry.`,
      }
    case 'ready':
      return {
        tone: 'ok',
        label: `ComfyUI ready at ${status.url}`,
        nextStep: null,
      }
  }
}

/**
 * Total VRAM as a short human string, or null when it is unknown.
 *
 * Unknown stays unknown: a CPU-only ComfyUI reports no GPU, and showing
 * `0 GB` on a working machine reads as a broken app. GiB, because that is what
 * every GPU tool shows -- 17102733312 bytes is a "16 GB" card at 15.9 GiB.
 */
export function formatVram(bytes: number | null): string | null {
  if (bytes === null || bytes <= 0) return null
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB VRAM`
}

interface ComfyState {
  status: ComfyStatus | null
  /** True while a probe is in flight. */
  busy: boolean
  /** Probe `url` if given, else the configured address or the local defaults. */
  refresh: (url?: string) => Promise<void>
}

export const useComfyStore = create<ComfyState>((set) => ({
  status: null,
  busy: false,

  refresh: async (url?: string) => {
    if (!isTauri()) return
    set({ busy: true })
    try {
      set({ status: await comfyStatus(url) })
    } finally {
      set({ busy: false })
    }
  },
}))
