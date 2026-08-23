import { invoke } from '@tauri-apps/api/core'

/**
 * Typed wrappers around Tauri commands. Components never import
 * `@tauri-apps/*` directly -- everything crosses the boundary here
 * (CONVENTIONS.md).
 */

/** Returns the Rust shell's crate version. Proves the bridge round-trips. */
export async function appVersion(): Promise<string> {
  return await invoke<string>('app_version')
}

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}
