import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Provenance } from './library'

/** Mirrors Rust `create_core::provenance::Artwork`. */
export interface Artwork {
  id: string
  title: string | null
  /** Relative to the project directory, e.g. `art/ar-0001.png`. */
  file: string
  /** Pixel size read off the file's own header; `null` when unreadable. */
  width: number | null
  height: number | null
  provenance: Provenance
}

/** Mirrors Rust `library::art::ArtSet`. */
export interface ArtSet {
  art: Artwork[]
  warnings: ArtWarning[]
}

/** Mirrors Rust `library::art::ArtWarning`. */
export type ArtWarning =
  | { kind: 'missing'; id: string }
  | { kind: 'unreadable'; id: string; detail: string }
  | { kind: 'malformed'; id: string; detail: string }

/** Payload of `art://saved`. Mirrors Rust `jobs::ArtSaved`. */
export interface ArtSaved {
  id: string
  project_slug: string
  file: string
}

/** List every artwork in the selected project, with warnings for bad sidecars. */
export async function listArt(): Promise<ArtSet> {
  return await invoke<ArtSet>('library_art')
}

/**
 * Resolve an artwork id to a URL the webview can display.
 *
 * The twin of `bridge/player.ts`'s `trackAudioUrl`, and for the same reason both
 * halves live here: the backend returns an absolute path (`art_image_path`,
 * which validates the id and the stored `file`), and `convertFileSrc` turns it
 * into an asset URL. **Resolving is not checking** -- the backend refuses a path
 * that escapes the project, but it does not stat the file, so a URL from here is
 * not a promise that an image is behind it. The view handles `onError` (T-506d).
 */
export async function artImageUrl(id: string): Promise<string> {
  const absolute = await invoke<string>('art_image_path', { id })
  return convertFileSrc(absolute)
}

/**
 * Delete an artwork: image and sidecar to the OS trash, the id unlisted, and
 * every track and album cover naming it cleared. The caller must reload the
 * library and the albums as well as the gallery -- records the frontend already
 * holds were changed on disk by this call.
 */
export async function deleteArt(id: string): Promise<void> {
  await invoke('delete_art', { id })
}

/**
 * Subscribe to `art://saved`.
 *
 * Re-load on every save rather than appending, exactly as `subscribeTracks`
 * does: the event carries id, slug and file, not the provenance a row needs.
 */
export async function subscribeArt(onSaved: (e: ArtSaved) => void): Promise<UnlistenFn> {
  return await listen<ArtSaved>('art://saved', (event) => {
    onSaved(event.payload)
  })
}
