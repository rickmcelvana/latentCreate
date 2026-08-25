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
  | { state: 'not_installed'; install_command: string }
  | { state: 'unreachable'; detail: string }
  | { state: 'server_down'; workspace: string | null }
  | {
      state: 'ready'
      url: string | null
      vram_bytes: number | null
      workspace: string | null
      comfy_cli_version: string | null
      update_available: boolean
    }

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * Connect if needed and report what the wizard should show.
 *
 * Rejects only when the app itself fails (it could not open its own session
 * log). A missing `comfy-mcp`, a dead ComfyUI and a broken connection all
 * resolve to a status.
 */
export async function comfyStatus(bin?: string): Promise<ComfyStatus> {
  return await invoke<ComfyStatus>('comfy_status', { bin })
}

/**
 * Start ComfyUI, then report the resulting status.
 *
 * Only offered when the status is `server_down`. A port already in use is not
 * treated as a failure -- the following health check reports what is really
 * there.
 */
export async function comfyLaunch(bin?: string): Promise<ComfyStatus> {
  return await invoke<ComfyStatus>('comfy_launch', { bin })
}
