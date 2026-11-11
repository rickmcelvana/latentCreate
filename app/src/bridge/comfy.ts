import { invoke } from '@tauri-apps/api/core'

/**
 * Mirrors Rust `src-tauri/src/comfy.rs` `ComfyStatus`, a serde-tagged union
 * (`#[serde(tag = "state", rename_all = "snake_case")]`).
 *
 * Every failure ComfyUI can present is a variant here rather than a thrown
 * error, so the view renders a pill with a next step instead of parsing
 * message strings (CONVENTIONS: degraded services degrade, never block).
 */
export type ComfyStatus =
  /** Nobody answered at any address tried. `configured`: the user set the
   *  address, so only it was tried. */
  | { state: 'not_found'; tried: string[]; configured: boolean }
  /** Something answered, but not as a ComfyUI the app can use. */
  | { state: 'unreachable'; url: string; detail: string }
  | {
      state: 'ready'
      url: string
      /** Total VRAM in bytes; null is unknown, never zero. */
      vram_bytes: number | null
      comfyui_version: string | null
      /** On this machine, by address -- the condition for one-click Install. */
      local: boolean
    }

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * Find ComfyUI and report what the Setup card should show.
 *
 * `url` tests an address before it is saved; without it the configured address
 * is used, and with none configured the usual local ports are probed. Rejects
 * only when the app itself fails (it could not open its own session log).
 */
export async function comfyStatus(url?: string): Promise<ComfyStatus> {
  return await invoke<ComfyStatus>('comfy_status', { url: url ?? null })
}
