import { invoke } from '@tauri-apps/api/core'

/** Which gallery kind to browse. Mirrors Rust `CatalogKind` (snake_case). */
export type CatalogKind = 'audio' | 'image'

/** One gallery row. Mirrors Rust `mcp_bridge::TemplateInfo`. */
export interface TemplateInfo {
  /** Gallery id, the key `catalog_readiness` takes, e.g. `image_flux2`. */
  name: string
  title: string
  description: string
  /** `audio` | `image` | ...; absent on some rows. */
  output_type: string | null
  tags: string[]
  category_title: string | null
  /** True only for the paid hosted tier; the browse filters these out already. */
  api: boolean
}

/** One page of gallery rows for a kind. Mirrors Rust `CatalogPage`. */
export interface CatalogPage {
  rows: TemplateInfo[]
  /** Matches across the whole kind, so the UI knows if more pages exist. */
  total: number
  offset: number
  /** True when comfy-mcp broadened the query past an exact match; the UI must say so. */
  widened: boolean
}

/**
 * Whether a gallery row can run here. Mirrors Rust `mcp_bridge::LocalCheck`, a
 * serde-tagged tri-state (`#[serde(tag = "state")]`).
 *
 * `unknown` means no comparison was made -- usually ComfyUI is stopped -- and is
 * NOT "not installed". `errors` is third-party prose (missing filenames); it is
 * shown verbatim and never parsed for a URL (MCP-SURFACE 33).
 */
export type LocalCheck =
  | { state: 'checked'; runnable: boolean; summary: string | null; errors: unknown[] }
  | { state: 'unknown' }

/** Browse one kind's local gallery rows, optionally narrowed by a query. */
export async function catalogBrowse(
  kind: CatalogKind,
  query?: string,
  offset = 0,
  bin?: string,
): Promise<CatalogPage> {
  return await invoke<CatalogPage>('catalog_browse', { kind, query, offset, bin })
}

/** Check one gallery row's readiness. `unknown` when ComfyUI could not be compared. */
export async function catalogReadiness(name: string, bin?: string): Promise<LocalCheck> {
  return await invoke<LocalCheck>('catalog_readiness', { name, bin })
}
