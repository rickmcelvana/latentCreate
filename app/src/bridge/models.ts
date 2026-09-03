import { invoke } from '@tauri-apps/api/core'

/** One model file the user does not have. Mirrors Rust `MissingFile`. */
export interface MissingFile {
  file: string
  folder: string
  /** Null means the app cannot fetch it and the user must place it by hand. */
  source_url: string | null
  size_bytes: number | null
  license: string | null
}

/**
 * Mirrors Rust `src-tauri/src/models.rs` `Readiness`, a serde-tagged union
 * (`#[serde(tag = "state", rename_all = "snake_case")]`).
 *
 * `undeclared` and `unknown` are both "we could not check", kept apart because
 * they have different fixes: one is a profile that never listed its files, the
 * other is a ComfyUI that is not running. Neither is `ready`, and neither may
 * be rendered as "not installed" -- ACE-Step is an 18.5 GiB download.
 */
export type Readiness =
  | { state: 'ready' }
  | {
      state: 'missing'
      files: MissingFile[]
      /** Null when any missing file has no declared size. */
      total_bytes: number | null
      /** True only when every missing file carries a URL. */
      installable: boolean
    }
  | { state: 'undeclared' }
  | { state: 'unknown' }

/** Where a profile was read from. */
export type ProfileSource = 'shipped' | 'user'

/** One row of the models step. Mirrors Rust `ProfileStatus`. */
export interface ProfileStatus {
  id: string
  display_name: string
  kind: 'music' | 'image'
  /** Shown wherever the model is chosen or installed (CONVENTIONS). */
  license: string
  license_notes: string | null
  source: ProfileSource
  vram_gb_min: number | null
  /**
   * The gallery template this profile rides (Rust `ComfySpec.template`), or null
   * for an imported-workflow profile. The model catalog joins a gallery row to a
   * profile on this: a row whose `name` equals it is the same model.
   */
  template: string | null
  readiness: Readiness
}

/** What the models step shows. Mirrors Rust `ModelsView`. */
export interface ModelsView {
  profiles: ProfileStatus[]
  warnings: unknown[]
  inventory_available: boolean
  inventory_detail: string | null
}

/**
 * Report every known profile and whether its models are installed.
 *
 * Rejects only when the app itself fails. A stopped ComfyUI comes back as
 * `inventory_available: false` with every row `unknown`.
 */
export async function modelsStatus(bin?: string): Promise<ModelsView> {
  return await invoke<ModelsView>('models_status', { bin })
}

/** One file's download, once submitted. Mirrors Rust `StartedFile`. */
export interface StartedFile {
  file: string
  download_id: string | null
  error: string | null
}

/** Progress for one file. Mirrors Rust `FileProgress`. */
export interface FileProgress {
  download_id: string
  /** `starting` | `downloading` | `completed` | `failed` | `unknown`. */
  status: string
  completed_bytes: number | null
  total_bytes: number | null
  percent: number | null
  error: string | null
}

/**
 * Start downloading everything a profile is missing.
 *
 * **Only from an explicit user action.** ACE-Step 1.5 is 18.5 GiB across four
 * files. Each file is reported separately, so a partial start is visible.
 */
export async function modelsInstall(id: string, bin?: string): Promise<StartedFile[]> {
  return await invoke<StartedFile[]>('models_install', { id, bin })
}

/** Poll every in-flight download in one round trip. */
export async function modelsProgress(ids: string[], bin?: string): Promise<FileProgress[]> {
  return await invoke<FileProgress[]>('models_progress', { ids, bin })
}
