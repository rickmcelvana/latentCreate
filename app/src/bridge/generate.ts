import { invoke } from '@tauri-apps/api/core'
import type { InputValue } from '../state/params'

/** One LoRA in a spec. Mirrors Rust `create_core::generation::LoraRef`. */
export interface LoraRef {
  file: string
  strength: number
  enabled: boolean
}

/** Which lyric document and version. Mirrors Rust `LyricRef`. */
export interface LyricRef {
  doc_id: string
  /** 1-based version number within that document. */
  version: number
}

/**
 * Everything one generation needs. Mirrors Rust
 * `create_core::generation::GenerationSpec`.
 *
 * `inputs` is the **tagged** `InputValue` map: the tag comes from each
 * control's declared kind, never from what the value looks like at runtime,
 * because untagged a JSON `3` deserialises as `Int`, `Float` or `Seed` and a
 * seed demoted to an int makes a track unreproducible.
 */
export interface GenerationSpec {
  profile_id: string
  inputs: Record<string, InputValue>
  loras: LoraRef[]
  lyrics: LyricRef | null
  /**
   * The song title named at generation, carried to the track and its exported
   * filename (T-409). `null` is an untitled track -- the Library falls back to
   * the id. A snapshot: retitling the source lyric document later never
   * retitles a track already made.
   */
  title: string | null
}

/**
 * What was queued, and what the app could not check while queueing it.
 *
 * Mirrors Rust `src-tauri/src/generate.rs` `Submission`.
 */
export interface Submission {
  /** The handle every later job call keys on. */
  prompt_id: string
  /** The working copy actually submitted -- the record of what ran. */
  workflow_path: string
  /**
   * Resolved addresses the audit could not resolve.
   *
   * Subgraph interiors, and addresses naming a node the top-level graph does
   * not have. **Unverified rather than known-working** (MCP-SURFACE 18.5), so
   * they are reported rather than swallowed.
   */
  unchecked_slots: string[]
  /** Ids of the LoRA loader nodes spliced in, in apply order. */
  lora_nodes: string[]
  /** The format the save-node edit wrote; null when the profile opted out. */
  output_format: string | null
}

/**
 * Queue one generation.
 *
 * Returns as soon as ComfyUI has the job; progress arrives on the `job://`
 * events the pump emits. **The caller must register the returned `prompt_id`
 * with the jobs store** -- the pump is started by this command rather than by
 * `run_workflow`, and `applyJobEvent` drops events for ids the store does not
 * know about.
 *
 * This connects to comfy-mcp on its own if nothing is connected yet, so there
 * is no need to connect first, and no reason to gate the button on it.
 */
export async function generateAudio(spec: GenerationSpec): Promise<Submission> {
  return await invoke<Submission>('generate_audio', { spec })
}

/**
 * Queue one cover-art generation.
 *
 * Same contract as `generateAudio` -- including that **the caller must register
 * the returned `prompt_id` with the jobs store**, because this command starts
 * the pump itself and `applyJobEvent` drops events for ids the store does not
 * know. The backend refuses a music profile here, and says where it belongs.
 */
export async function generateImage(spec: GenerationSpec): Promise<Submission> {
  return await invoke<Submission>('generate_image', { spec })
}
