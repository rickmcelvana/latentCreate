import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { type GenerationSpec } from '../bridge/generate'
import type { InputValue } from '../state/params'

/** Mirrors Rust `create_core::provenance::Track`. */
export interface Track {
  id: string
  title: string | null
  file: string
  duration_s: number | null
  provenance: Provenance
}

/** Mirrors Rust `create_core::provenance::Provenance`. */
export interface Provenance {
  profile_id: string
  profile_display_name: string
  model_license: string
  template: string | null
  spec: GenerationSpec
  resolved_slots: Record<string, InputValue>
  comfy: ComfyServerInfo | null
  created_at: string
  prompt_id: string | null
}

/** Mirrors Rust `create_core::provenance::ComfyServerInfo`. */
export interface ComfyServerInfo {
  comfyui_version: string | null
  comfy_cli_version: string | null
  url: string | null
}

/** Mirrors Rust `library::tracks::TrackSet`. */
export interface TrackSet {
  tracks: Track[]
  warnings: TrackWarning[]
}

/** Mirrors Rust `library::tracks::TrackWarning`. */
export type TrackWarning =
  | { kind: 'missing'; id: string }
  | { kind: 'unreadable'; id: string; detail: string }
  | { kind: 'malformed'; id: string; detail: string }

/** Payload of `track://saved`. Mirrors Rust T-311b event shape. */
export interface TrackSaved {
  id: string
  project_slug: string
  file: string
}

/** List every track in the default project, with warnings for bad sidecars. */
export async function listTracks(): Promise<TrackSet> {
  return await invoke<TrackSet>('library_tracks')
}

/**
 * Subscribe to `track://saved`.
 *
 * Re-load on every save rather than appending: the event carries only id,
 * project slug and file, not the full provenance a row needs.
 */
export async function subscribeTracks(onSaved: (e: TrackSaved) => void): Promise<UnlistenFn> {
  return await listen<TrackSaved>('track://saved', (event) => {
    onSaved(event.payload)
  })
}
