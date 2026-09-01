import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'

/** Move a track's files to the OS trash and unlist its id. */
export async function deleteTrack(id: string): Promise<void> {
  await invoke('delete_track', { id })
}

/** Set or clear a track's title. An empty title clears it. */
export async function renameTrack(id: string, title: string): Promise<void> {
  await invoke('rename_track', { id, title })
}

/** Copy a track's audio file to `dest`. */
export async function exportTrack(id: string, dest: string): Promise<void> {
  await invoke('export_track', { id, dest })
}

/** Reveal a track's audio file in the OS file manager. */
export async function revealTrack(id: string): Promise<void> {
  await invoke('reveal_track', { id })
}

/**
 * Show the OS save dialog for an export; `null` if the user cancelled.
 *
 * The dialog lives in the bridge, not the view -- the one Tauri surface
 * `ImportWorkflow` reaches for directly, kept here so every crossing is in
 * `bridge/` (CONVENTIONS).
 */
export async function pickExportPath(defaultName: string): Promise<string | null> {
  const chosen = await save({ defaultPath: defaultName })
  return typeof chosen === 'string' ? chosen : null
}
