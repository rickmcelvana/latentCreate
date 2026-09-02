import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import {
  catalogBrowse,
  catalogReadiness,
  type CatalogKind,
  type CatalogPage,
  type LocalCheck,
} from '../bridge/catalog'

/**
 * The Ready / Not-ready / Unknown verdict for one gallery row.
 *
 * `not_ready` carries the reasons verbatim from `local_check.errors` -- missing
 * filenames written by the gallery, shown as-is and never parsed (MCP-SURFACE
 * 33). `unknown` is "could not check" (ComfyUI stopped), kept apart from
 * `not_ready` because their fixes differ, exactly as the models step keeps
 * `unknown` apart from `missing`.
 */
export type CatalogVerdict =
  | { kind: 'ready' }
  | { kind: 'not_ready'; reasons: string[] }
  | { kind: 'unknown' }

/**
 * Derive the verdict from a `LocalCheck`. Pure, so every branch is testable
 * without a bridge.
 *
 * A not-runnable check with no usable `errors` still needs one line, so it falls
 * back to `summary` -- a not-ready row that says nothing is indistinguishable
 * from a bug.
 */
export function verdictFor(check: LocalCheck): CatalogVerdict {
  if (check.state === 'unknown') return { kind: 'unknown' }
  if (check.runnable) return { kind: 'ready' }
  const reasons = check.errors
    .map((e) => (typeof e === 'string' ? e : JSON.stringify(e)))
    .map((s) => s.trim())
    .filter((s) => s !== '')
  if (reasons.length === 0 && check.summary !== null && check.summary.trim() !== '') {
    reasons.push(check.summary.trim())
  }
  return { kind: 'not_ready', reasons }
}

/** How one row's readiness should read: a pill and its detail. */
export interface CatalogRowView {
  tone: 'ok' | 'warn' | 'neutral'
  label: string
  /** The missing-file lines, empty for every state but `not_ready`. */
  reasons: string[]
}

/**
 * Map a verdict (or the in-flight `'checking'` marker) to what the user sees.
 *
 * `unknown` reads "Can't check" and points at ComfyUI, never "Not installed" --
 * the same rule the models step enforces, because a stopped server is not an
 * empty install.
 */
export function rowViewFor(verdict: CatalogVerdict | 'checking'): CatalogRowView {
  if (verdict === 'checking') {
    return { tone: 'neutral', label: 'Checking...', reasons: [] }
  }
  switch (verdict.kind) {
    case 'ready':
      return { tone: 'ok', label: 'Installed', reasons: [] }
    case 'not_ready':
      return { tone: 'warn', label: 'Not installed', reasons: verdict.reasons }
    case 'unknown':
      return { tone: 'neutral', label: "Can't check", reasons: [] }
  }
}

interface CatalogState {
  kind: CatalogKind
  query: string
  page: CatalogPage | null
  busy: boolean
  /** Set when the gallery itself could not be read; the UI shows Retry. */
  error: string | null
  /** Readiness per row name: absent = not yet checked, `'checking'` = in flight. */
  readiness: Record<string, CatalogVerdict | 'checking'>
  /** Load a kind's gallery from the top, clearing the query and readiness. */
  open: (kind: CatalogKind) => Promise<void>
  /** Set the search text and reload the current kind from the top. */
  search: (query: string) => Promise<void>
  /** Reload the current kind + query (the Retry action). */
  reload: () => Promise<void>
  /** Resolve one row's readiness, once. A second call while known/in-flight is a no-op. */
  checkReadiness: (name: string) => Promise<void>
}

async function load(
  set: (partial: Partial<CatalogState>) => void,
  kind: CatalogKind,
  query: string,
): Promise<void> {
  if (!isTauri()) return
  set({ busy: true, error: null })
  try {
    const page = await catalogBrowse(kind, query === '' ? undefined : query, 0)
    // Rows changed, so any readiness resolved for the old page no longer applies.
    set({ page, readiness: {} })
  } catch (e) {
    set({ error: String(e) })
  } finally {
    set({ busy: false })
  }
}

export const useCatalogStore = create<CatalogState>((set, get) => ({
  kind: 'audio',
  query: '',
  page: null,
  busy: false,
  error: null,
  readiness: {},

  open: async (kind: CatalogKind) => {
    set({ kind, query: '', page: null, readiness: {} })
    await load(set, kind, '')
  },

  search: async (query: string) => {
    set({ query })
    await load(set, get().kind, query)
  },

  reload: async () => {
    await load(set, get().kind, get().query)
  },

  checkReadiness: async (name: string) => {
    if (!isTauri()) return
    // Resolve once: absent means never checked; anything else is a verdict or
    // an in-flight `'checking'`, and re-checking would flicker the row.
    if (get().readiness[name] !== undefined) return
    set((s) => ({ readiness: { ...s.readiness, [name]: 'checking' } }))
    try {
      const check = await catalogReadiness(name)
      set((s) => ({ readiness: { ...s.readiness, [name]: verdictFor(check) } }))
    } catch {
      // A failed readiness poll is "can't tell", never "not installed".
      set((s) => ({ readiness: { ...s.readiness, [name]: { kind: 'unknown' } } }))
    }
  },
}))
