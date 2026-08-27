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
