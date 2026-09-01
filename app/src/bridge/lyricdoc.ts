import { invoke } from '@tauri-apps/api/core'
import type { LyricBrief } from './lyrics'

/**
 * Where a lyric version came from. Mirrors Rust `create-core::project::LyricSource`
 * (`#[serde(tag = "kind", rename_all = "snake_case")]`).
 */
export type LyricSource =
  | { kind: 'human' }
  | { kind: 'llm'; model: string; prompt_optimized: boolean }
  | { kind: 'edited'; from_version: number }

/** One immutable revision. Mirrors Rust `LyricVersion`. */
export interface LyricVersion {
  /** 1-based, monotonically increasing within a document. */
  number: number
  text: string
  /** RFC 3339. */
  created_at: string
  source: LyricSource
}

/**
 * A lyric document and every version inside it. Mirrors Rust `LyricDoc`.
 *
 * `id` is a bare string on the wire (`LyricDocId` is serde-transparent).
 */
export interface LyricDoc {
  id: string
  title: string | null
  versions: LyricVersion[]
  /** The `number` of the version approved for audio, if any. */
  approved: number | null
}

/**
 * One advisory finding. Mirrors Rust `create-core::lyrics::lint::LintFinding`
 * (`#[serde(tag = "kind", rename_all = "snake_case")]`).
 *
 * Severity is not on the wire -- the backend's `severity()` derives it -- so it
 * is re-derived here by [`lintSeverity`].
 */
export type LintFinding =
  | { kind: 'unknown_tag'; tag: string; line: number }
  | { kind: 'missing_section'; section: string }
  | { kind: 'out_of_order'; requested: string[] }
  | { kind: 'extra_section'; tag: string; line: number }
  | { kind: 'text_after_tag'; text: string; line: number }
  | { kind: 'no_structure_tags' }

/** How loudly a finding is shown. Mirrors Rust `LintSeverity`. */
export type LintSeverity = 'warning' | 'info'

/**
 * The severity of a finding, mirroring the backend's rule: an extra section is
 * information (most lyrics add an outro), everything else is a warning.
 */
export function lintSeverity(finding: LintFinding): LintSeverity {
  return finding.kind === 'extra_section' ? 'info' : 'warning'
}

/**
 * Something about listing a project's documents worth reporting. Mirrors Rust
 * `library::lyrics::LyricWarning` (`#[serde(tag = "kind", rename_all = "snake_case")]`).
 */
export type LyricWarning =
  | { kind: 'missing'; id: string }
  | { kind: 'unreadable'; id: string; detail: string }
  | { kind: 'malformed'; id: string; detail: string }

/** Every document a project offers, plus anything worth reporting. Mirrors Rust `LyricDocSet`. */
export interface LyricDocSet {
  docs: LyricDoc[]
  warnings: LyricWarning[]
}

/**
 * Open a document by id, or -- with no id -- the project's first document,
 * creating one on first use. The id retires the Phase 2 one-document shortcut.
 */
export async function openLyricDoc(id?: string): Promise<LyricDoc> {
  return await invoke<LyricDoc>('lyrics_open', { docId: id ?? null })
}

/** Every lyric document in the selected project, in creation order. */
export async function listLyricDocs(): Promise<LyricDocSet> {
  return await invoke<LyricDocSet>('lyrics_list')
}

/** Create a new, empty document and return it. */
export async function createLyricDoc(title?: string): Promise<LyricDoc> {
  return await invoke<LyricDoc>('lyrics_create', { title: title ?? null })
}

/**
 * Delete a whole document -- file to OS trash -- refusing (with a message naming
 * the tracks) when a track references any of its versions. Returns the remainder.
 */
export async function deleteLyricDoc(docId: string): Promise<LyricDocSet> {
  return await invoke<LyricDocSet>('lyrics_delete_doc', { docId })
}

/** Persist a document, versions and approval included. */
export async function saveLyricDoc(doc: LyricDoc): Promise<void> {
  await invoke('lyrics_save', { doc })
}

/**
 * Delete one version, refusing (with a message naming the tracks) when a track's
 * provenance references it. Returns the updated document, so the caller replaces
 * its copy rather than editing `versions` locally -- a local edit followed by
 * `saveLyricDoc` would bypass the backend's refusal check.
 */
export async function deleteLyricVersion(docId: string, number: number): Promise<LyricDoc> {
  return await invoke<LyricDoc>('lyrics_delete_version', { docId, version: number })
}

/** Lint text against a profile and brief, returning advisory findings. */
export async function lintLyrics(
  profileId: string,
  brief: LyricBrief,
  text: string,
): Promise<LintFinding[]> {
  return await invoke<LintFinding[]>('lyrics_lint', { profileId, brief, text })
}
