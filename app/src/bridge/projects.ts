import { invoke } from '@tauri-apps/api/core'

/** Mirrors Rust `create_core::project::Project`. */
export interface Project {
  slug: string
  name: string
  created_at: string
  /** `TrackId` is a transparent string on the wire. */
  tracks: string[]
  /** `LyricDocId` is a transparent string on the wire. */
  lyrics: string[]
  albums: AlbumList[]
  next_lyric_seq: number
  next_track_seq: number
}

/** Mirrors Rust `create_core::project::AlbumList`. */
export interface AlbumList {
  name: string
  tracks: string[]
}

/** Mirrors Rust `library::projects::ProjectSet`. */
export interface ProjectSet {
  projects: Project[]
  warnings: ProjectWarning[]
}

/** Mirrors Rust `library::projects::ProjectWarning`. */
export type ProjectWarning =
  | { kind: 'dir_unreadable'; dir: string; detail: string }
  | { kind: 'unreadable'; slug: string; detail: string }
  | { kind: 'malformed'; slug: string; detail: string }
  | { kind: 'slug_mismatch'; directory: string; recorded: string }

/** List every project. Never rejects for a bad project -- that is a warning. */
export async function listProjects(): Promise<ProjectSet> {
  return await invoke<ProjectSet>('projects_list')
}

/** Create a project and return its record (slug already minted by the backend). */
export async function createProject(name: string): Promise<Project> {
  return await invoke<Project>('projects_create', { name })
}
