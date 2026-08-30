import { invoke } from '@tauri-apps/api/core'

/**
 * Mirrors Rust `mcp_bridge::Slot`.
 *
 * Carried on the report but not read by the store: the suggestions are what a
 * mapping screen works from. It is here so a later "show me every slot" escape
 * hatch has the data without a second command.
 */
export interface Slot {
  address: string
  name: string
  type: string
  current_value: unknown
  instance_id: string
  node_type: string
}

/** Mirrors Rust `create_core::roles::Role`. */
export type Role =
  | 'tags'
  | 'lyrics'
  | 'negative'
  | 'duration_seconds'
  | 'seed'
  | 'steps'
  | 'cfg'

/**
 * Mirrors Rust `create_core::roles::Confidence`.
 *
 * **The pre-tick rule, and the reason it exists.** `strong` means the input
 * name and widget type both fit. `possible` means the candidate was reached by
 * following a link -- ACE-Step's seed is `109.value`, whose name and node class
 * say nothing about seeds; it is right because of the graph's shape. So it is
 * offered, never ticked on the user's behalf.
 */
export type Confidence = 'strong' | 'possible'

/** Mirrors Rust `create_core::roles::Candidate`. */
export interface Candidate {
  address: string
  node_type: string
  confidence: Confidence
  /** Why this was offered, in words a person can check against their graph. */
  reason: string
}

/** Mirrors Rust `create_core::roles::RoleSuggestion`. */
export interface RoleSuggestion {
  role: Role
  candidates: Candidate[]
}

/** Mirrors Rust `app::import::ImportReport`. */
export interface ImportReport {
  workflow_id: string
  stored_path: string
  slots: Slot[]
  suggestions: RoleSuggestion[]
  /** Advisory only. Never a reason to refuse (MCP-SURFACE 29.3). */
  warnings: string[]
}

/** Mirrors Rust `app::import::RoleMapping`. */
export interface RoleMapping {
  role: Role
  addresses: string[]
}

/** Mirrors Rust `app::import::SavedProfile`. */
export interface SavedProfile {
  profile_id: string
  path: string
}

/** Store a copy of the workflow at `source` and report what it exposes. */
export async function importWorkflow(source: string): Promise<ImportReport> {
  return await invoke<ImportReport>('import_workflow', { source })
}

/** Turn accepted mappings into a user profile the picker will list. */
export async function saveImportedProfile(
  workflowId: string,
  displayName: string,
  mappings: RoleMapping[],
): Promise<SavedProfile> {
  return await invoke<SavedProfile>('save_imported_profile', {
    workflowId,
    displayName,
    mappings,
  })
}
