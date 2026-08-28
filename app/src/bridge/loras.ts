import { invoke } from '@tauri-apps/api/core'

/**
 * One selectable adapter. Mirrors Rust `create_core::loras::LoraEntry`.
 *
 * `path` is the choice **verbatim**, backslashes and all -- it is the identity
 * handed to the loader node, and normalising the separators would produce a
 * value ComfyUI rejects as `unknown_enum_value` (MCP-SURFACE 19.3).
 */
export interface LoraEntry {
  path: string
  label: string
  /** `n` when this is step `n` of a training run. */
  epoch: number | null
  is_final: boolean
}

/** Adapters sharing a top-level directory. Mirrors Rust `LoraGroup`. */
export interface LoraGroup {
  /** The first path segment; **empty** for files loose in the `loras` root. */
  name: string
  primary: LoraEntry[]
  /** Superseded training steps, newest first. */
  superseded: LoraEntry[]
}

/** A choice that did not become an entry. Mirrors Rust `Excluded`. */
export interface Excluded {
  path: string
  reason: 'not_an_adapter' | 'case_duplicate'
}

/** Mirrors Rust `create_core::profile::StrengthRange`. */
export interface StrengthRange {
  min: number
  max: number
  default: number
  step: number | null
}

/** Whether the installed LoRAs could be read. Mirrors Rust `CatalogState`. */
export type CatalogState =
  | { state: 'loaded'; groups: LoraGroup[]; excluded: Excluded[]; cached: boolean }
  | { state: 'unavailable'; detail: string }

/** Mirrors Rust `src-tauri/src/loras.rs` `LoraPanel`. */
export interface LoraPanel {
  /** The profile's range, never the node's -100..100. */
  strength: StrengthRange
  max_stack: number
  catalog: CatalogState
}

/**
 * The LoRA panel for one profile, or null when there is no panel to show.
 *
 * Null means *render nothing*: either no profile answers to this id, or the
 * profile declares no `loras` block. Every other state is a **visible** panel
 * carrying a sentence -- a panel that vanished when ComfyUI was down would tell
 * the user their model cannot take LoRAs.
 */
export async function getLoraPanel(profileId: string): Promise<LoraPanel | null> {
  return await invoke<LoraPanel | null>('lora_panel', { profileId })
}
