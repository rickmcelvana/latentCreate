import { create } from 'zustand'
import {
  deleteTrack,
  exportTrack,
  pickExportPath,
  renameTrack,
  revealTrack,
  setTrackCover,
} from '../bridge/tracks'
import { useLibraryStore } from './library'

/** An action failure, remembered with the track it belongs to. */
export interface ActionError {
  trackId: string
  message: string
}

/** The error to show under one row, or `null`. Twin of `sendto` `failureFor`. */
export function errorFor(error: ActionError | null, trackId: string): string | null {
  if (error === null) return null
  return error.trackId === trackId ? error.message : null
}

/** Whether `trackId` is the row named by a `string | null` marker. */
export function isRow(marker: string | null, trackId: string): boolean {
  return marker === trackId
}

/**
 * A track title reduced to a legal filename **stem** (no extension), for the
 * export dialog's default name (T-409). Windows-illegal characters
 * (`\ / : * ? " < > |`) and control characters become `_`; a trailing dot or
 * space is stripped (Windows forbids them); leading and trailing underscores go
 * too, so a title of only illegal characters reduces to `''` -- the caller then
 * falls back to the track id, which is always safe.
 *
 * **Sanitise before the dialog, not after** (T-409 trap 4): the symptom of
 * getting this wrong is the OS refusing the save with its own message rather
 * than the app preventing it.
 */
export function filenameSafe(name: string): string {
  // Drop control characters without a control-char regex (which oxlint flags),
  // then map the Windows-illegal punctuation to `_`.
  const noControl = Array.from(name)
    .filter((ch) => ch.charCodeAt(0) >= 0x20)
    .join('')
  return noControl
    .replace(/[\\/:*?"<>|]/g, '_')
    .replace(/[.\s]+$/, '')
    .replace(/^_+|_+$/g, '')
    .trim()
}

interface TrackActionsState {
  /** A track id with an action in flight, or `null`. */
  busy: string | null
  error: ActionError | null
  /** A track id awaiting delete confirmation, or `null`. */
  confirming: string | null
  /** A track id whose title is being edited, or `null`. */
  renaming: string | null
  askDelete: (id: string) => void
  cancelDelete: () => void
  confirmDelete: (id: string) => Promise<boolean>
  startRename: (id: string) => void
  cancelRename: () => void
  submitRename: (id: string, title: string) => Promise<boolean>
  /** Set or clear a track's cover, then reload the library so the row shows it. */
  setCover: (id: string, cover: string | null) => Promise<boolean>
  runExport: (id: string, defaultName: string) => Promise<void>
  reveal: (id: string) => Promise<void>
}

function message(err: unknown): string {
  // Tauri rejects a `Result<(), String>` with the bare string, not an Error.
  return err instanceof Error ? err.message : String(err)
}

export const useTrackActionsStore = create<TrackActionsState>((set) => ({
  busy: null,
  error: null,
  confirming: null,
  renaming: null,

  askDelete: (id) => set({ confirming: id, error: null }),
  cancelDelete: () => set({ confirming: null }),

  confirmDelete: async (id) => {
    set({ busy: id, error: null })
    try {
      await deleteTrack(id)
      set({ busy: null, confirming: null })
      return true
    } catch (err: unknown) {
      // Keep `confirming` set so the row can retry or cancel.
      set({ busy: null, error: { trackId: id, message: message(err) } })
      return false
    }
  },

  startRename: (id) => set({ renaming: id, error: null }),
  cancelRename: () => set({ renaming: null }),

  submitRename: async (id, title) => {
    set({ busy: id, error: null })
    try {
      await renameTrack(id, title)
      set({ busy: null, renaming: null })
      return true
    } catch (err: unknown) {
      set({ busy: null, error: { trackId: id, message: message(err) } })
      return false
    }
  },

  setCover: async (id, cover) => {
    set({ busy: id, error: null })
    try {
      await setTrackCover(id, cover)
      await useLibraryStore.getState().load()
      set({ busy: null })
      return true
    } catch (err: unknown) {
      set({ busy: null, error: { trackId: id, message: message(err) } })
      return false
    }
  },

  runExport: async (id, defaultName) => {
    let dest: string | null
    try {
      dest = await pickExportPath(defaultName)
    } catch (err: unknown) {
      set({ error: { trackId: id, message: message(err) } })
      return
    }
    // Cancelling the dialog is the user's decision, not a failure.
    if (dest === null) return
    set({ busy: id, error: null })
    try {
      await exportTrack(id, dest)
      set({ busy: null })
    } catch (err: unknown) {
      set({ busy: null, error: { trackId: id, message: message(err) } })
    }
  },

  reveal: async (id) => {
    set({ error: null })
    try {
      await revealTrack(id)
    } catch (err: unknown) {
      set({ error: { trackId: id, message: message(err) } })
    }
  },
}))
