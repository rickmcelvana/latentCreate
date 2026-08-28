import { invoke } from '@tauri-apps/api/core'

/** One worked example. Mirrors Rust `PromptExampleView`. */
export interface PromptExample {
  tags: string
  lyrics: string | null
}

/** Mirrors Rust `src-tauri/src/profile.rs` `ProfileGuideView`. */
export interface ProfileGuide {
  display_name: string
  /** Hint for the style-tags field, e.g. "comma-separated short tags". */
  tag_style: string | null
  /** The first example's `tags` prefills the brief form. */
  examples: PromptExample[]
}

/**
 * The selected profile's authoring guide, or null when the profile does not
 * exist. A profile with no guide still comes back with `display_name` and an
 * empty `examples`.
 */
export async function getProfileGuide(profileId: string): Promise<ProfileGuide | null> {
  return await invoke<ProfileGuide | null>('profile_guide', { profileId })
}

/**
 * One declared input, mirroring Rust `create_core::profile::InputSpec`.
 *
 * Internally tagged on `type`, which is how it is written in `profiles/*.json`
 * and how it serialises -- so this is the profile's own shape, not a separate
 * view type invented for the webview.
 *
 * `unsupported` is a declaration, not an absence: it records that someone
 * checked a live node schema and the model has no such input.
 */
export type InputSpec =
  | { type: 'text'; slots: string[]; label?: string | null; advanced: boolean }
  | {
      type: 'lyrics'
      slots: string[]
      structure_tags: string[]
      label?: string | null
      advanced: boolean
    }
  | {
      type: 'int'
      slots: string[]
      min: number
      max: number
      default: number
      label?: string | null
      advanced: boolean
    }
  | {
      type: 'float'
      slots: string[]
      min: number
      max: number
      default: number
      step?: number | null
      label?: string | null
      advanced: boolean
    }
  | { type: 'seed'; slots: string[] }
  | {
      type: 'enum'
      slots: string[]
      from_node_choices: boolean
      choices: string[]
      label?: string | null
      advanced: boolean
    }
  | {
      type: 'group'
      members: ProfileInputs
      label?: string | null
      advanced: boolean
    }
  | { type: 'unsupported'; reason?: string | null }

/** A profile's `inputs`: semantic name -> declaration. */
export type ProfileInputs = Record<string, InputSpec>
