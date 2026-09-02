# T-505a — Model catalog: the browse/readiness store (frontend, no UI)

**Lane: Aider.** Frontend-only, three new files, fully unit-tested — the testable core of the
catalog before any component. **Depends:** T-504 landed (the `catalog_browse` / `catalog_readiness`
commands, [tasks/t-504-brief.md](t-504-brief.md)). **Dir:** `app/src`.

**Files to create:**

- `app/src/bridge/catalog.ts` — typed wrappers + the mirror types (`CatalogKind`, `TemplateInfo`,
  `CatalogPage`, `LocalCheck`)
- `app/src/state/catalog.ts` — the pure verdict/row derivations + the Zustand store
- `app/src/state/catalog.test.ts` — the tests

**No other files change.** The `<ModelCatalog>` component, the Setup wiring, curated one-click
install, and adopt-to-profile are **T-505b/c/d**.

---

## Goal

A `state/catalog.ts` store that drives the gallery browser: pick a kind (audio/image), search, list
rows, and resolve each row's readiness **lazily** from `catalog_readiness`. The Ready / Not-ready /
Unknown **verdict is derived here in TS** (`create-core` is pure and has no mcp-bridge dep, T-504),
the same way `state/models.ts::rowFor` and `state/queue.ts` derive display state. This lane is the
store; it renders nothing.

## Context you need (all verified — do not re-derive)

- **The two commands** (T-504, `src-tauri/src/catalog.rs`): `catalog_browse(kind, query, offset)` →
  `CatalogPage`, and `catalog_readiness(name)` → `LocalCheck`. Both also take an optional `bin`
  (custom comfy-mcp path), passed like `modelsStatus(bin)` in `bridge/models.ts`.
- **`LocalCheck` is a serde-tagged tri-state** (`crates/mcp-bridge/src/templates.rs`): `{state:
  "checked", runnable, summary, errors}` or `{state: "unknown"}`. `unknown` means "no comparison was
  made" (usually ComfyUI stopped) — it is **not** "not installed" (the same rule the models step
  lives by). `errors` is third-party prose naming missing files; **show it verbatim, never parse a
  URL out of it** (MCP-SURFACE §33).
- **`catalog_browse` can reject** when comfy-mcp cannot be read; that is a page-level error with a
  Retry (the `state/library.ts` error pattern), not per-row. `catalog_readiness` rejecting is a
  transport failure for one row — treat it as `unknown` (can't tell), never as not-installed.
- The store guards every bridge call with `isTauri()` from `../bridge/comfy` (as `state/models.ts`
  does), so it is inert in a non-Tauri context.

## Spec

### 1. `app/src/bridge/catalog.ts`

```ts
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
 * shown verbatim and never parsed for a URL (MCP-SURFACE §33).
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
```

### 2. `app/src/state/catalog.ts`

```ts
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
 * §33). `unknown` is "could not check" (ComfyUI stopped), kept apart from
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
```

### 3. `app/src/state/catalog.test.ts`

Mirror `state/projects.test.ts` for the bridge/`isTauri` mocking and `state/models.test.ts` for the
pure-function sweeps. Cover: `verdictFor` (every branch + the summary fallback + non-string errors),
`rowViewFor` (no non-ready state reads "Installed"; `unknown` never reads "Not installed"), and the
store (`open`/`search`/`reload`/`checkReadiness`, the empty-query→`undefined` mapping, readiness
reset on reload, the dedupe, and both reject paths).

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { CatalogPage, LocalCheck } from '../bridge/catalog'
import { rowViewFor, useCatalogStore, verdictFor } from './catalog'

const mockBrowse = vi.fn()
const mockReadiness = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/catalog', () => ({
  catalogBrowse: (kind: string, query?: string, offset?: number) =>
    mockBrowse(kind, query, offset),
  catalogReadiness: (name: string) => mockReadiness(name),
}))

vi.mock('../bridge/comfy', () => ({
  isTauri: () => mockIsTauri,
}))

function page(over: Partial<CatalogPage> = {}): CatalogPage {
  return { rows: [], total: 0, offset: 0, widened: false, ...over }
}

beforeEach(() => {
  mockBrowse.mockReset()
  mockReadiness.mockReset()
  mockIsTauri = true
  useCatalogStore.setState({
    kind: 'audio',
    query: '',
    page: null,
    busy: false,
    error: null,
    readiness: {},
  })
})

describe('verdictFor', () => {
  it('reads a runnable check as ready', () => {
    const check: LocalCheck = { state: 'checked', runnable: true, summary: 'ok', errors: [] }
    expect(verdictFor(check)).toEqual({ kind: 'ready' })
  })

  it('carries the error prose verbatim for a not-runnable check', () => {
    const check: LocalCheck = {
      state: 'checked',
      runnable: false,
      summary: '1 problem',
      errors: ["node 30: 'flux1-schnell-fp8.safetensors' is unavailable"],
    }
    const verdict = verdictFor(check)
    expect(verdict.kind).toBe('not_ready')
    if (verdict.kind === 'not_ready') {
      expect(verdict.reasons).toEqual([
        "node 30: 'flux1-schnell-fp8.safetensors' is unavailable",
      ])
    }
  })

  it('falls back to the summary when a not-runnable check has no usable errors', () => {
    const check: LocalCheck = { state: 'checked', runnable: false, summary: 'needs files', errors: [] }
    expect(verdictFor(check)).toEqual({ kind: 'not_ready', reasons: ['needs files'] })
  })

  it('coerces a non-string error rather than dropping it', () => {
    const check: LocalCheck = { state: 'checked', runnable: false, summary: null, errors: [{ x: 1 }] }
    const verdict = verdictFor(check)
    expect(verdict.kind).toBe('not_ready')
    if (verdict.kind === 'not_ready') expect(verdict.reasons).toEqual(['{"x":1}'])
  })

  it('reads an unknown check as unknown, never ready or not-ready', () => {
    expect(verdictFor({ state: 'unknown' })).toEqual({ kind: 'unknown' })
  })
})

describe('rowViewFor', () => {
  /** Protects the models-step rule, re-stated for the catalog: an uncheckable
   *  row is never "Not installed". */
  it('never presents unknown or checking as not installed', () => {
    for (const v of [{ kind: 'unknown' } as const, 'checking' as const]) {
      const view = rowViewFor(v)
      expect(view.label).not.toContain('Not installed')
      expect(view.tone).toBe('neutral')
    }
  })

  it('reads ready as an ok pill and not-ready as a warn pill with reasons', () => {
    expect(rowViewFor({ kind: 'ready' })).toEqual({ tone: 'ok', label: 'Installed', reasons: [] })
    const warn = rowViewFor({ kind: 'not_ready', reasons: ['missing X'] })
    expect(warn.tone).toBe('warn')
    expect(warn.reasons).toEqual(['missing X'])
  })
})

describe('useCatalogStore', () => {
  it('open loads a kind from the top and clears query and readiness', async () => {
    mockBrowse.mockResolvedValue(page({ total: 19, rows: [] }))
    useCatalogStore.setState({ query: 'stale', readiness: { x: { kind: 'ready' } } })
    await useCatalogStore.getState().open('audio')
    const s = useCatalogStore.getState()
    expect(s.kind).toBe('audio')
    expect(s.query).toBe('')
    expect(s.page?.total).toBe(19)
    expect(s.readiness).toEqual({})
    // Empty query is sent as undefined -- an empty string is a different comfy-cli path.
    expect(mockBrowse).toHaveBeenCalledWith('audio', undefined, 0)
  })

  it('search forwards the query and resets readiness for the new rows', async () => {
    mockBrowse.mockResolvedValue(page())
    useCatalogStore.setState({ readiness: { old: { kind: 'ready' } } })
    await useCatalogStore.getState().search('ace')
    expect(mockBrowse).toHaveBeenCalledWith('audio', 'ace', 0)
    expect(useCatalogStore.getState().readiness).toEqual({})
  })

  it('surfaces a browse rejection as a retryable error, not a throw', async () => {
    mockBrowse.mockRejectedValue(new Error('comfy-mcp not found'))
    await useCatalogStore.getState().open('image')
    const s = useCatalogStore.getState()
    expect(s.error).toContain('comfy-mcp not found')
    expect(s.busy).toBe(false)
  })

  it('checkReadiness resolves a row once and dedupes a second call', async () => {
    mockReadiness.mockResolvedValue({ state: 'checked', runnable: true, summary: null, errors: [] })
    await useCatalogStore.getState().checkReadiness('image_flux2')
    expect(useCatalogStore.getState().readiness['image_flux2']).toEqual({ kind: 'ready' })
    await useCatalogStore.getState().checkReadiness('image_flux2')
    expect(mockReadiness).toHaveBeenCalledTimes(1)
  })

  it('reads a failed readiness poll as unknown, never not-installed', async () => {
    mockReadiness.mockRejectedValue(new Error('transport'))
    await useCatalogStore.getState().checkReadiness('x')
    expect(useCatalogStore.getState().readiness['x']).toEqual({ kind: 'unknown' })
  })

  it('is inert without Tauri', async () => {
    mockIsTauri = false
    await useCatalogStore.getState().open('audio')
    expect(mockBrowse).not.toHaveBeenCalled()
  })
})
```

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] `verdictFor` maps `runnable:true`→ready, `runnable:false`→not_ready carrying the errors
      **verbatim** (falling back to summary), `{state:'unknown'}`→unknown. Flagship:
      `carries the error prose verbatim` — if it parsed or dropped the errors it must fail.
- [ ] `rowViewFor` gives **no** non-ready state the label "Installed", and `unknown`/`checking`
      never read "Not installed".
- [ ] `search('')` sends `undefined` for the query (not `''`); `checkReadiness` calls the bridge at
      most once per name; a `catalog_readiness` reject becomes `unknown`, never a throw or a
      not-installed row.
- [ ] Store methods are inert when `isTauri()` is false.
- [ ] Only the three new files exist; nothing else changes.

## Out of scope (T-505b/c/d)

- **The `<ModelCatalog>` component and the Setup wiring** (Music-models step audio, a Cover-art step
  image) — T-505b.
- **Curated one-click install** — reuses `models_install`/`models_progress` (already in
  `bridge/models.ts` and `state/models.ts`); wired in T-505c.
- **Adopt an installed row into a profile** — the T-313 import path — T-505d.
- **Paging past the first page** — the store loads `offset: 0`; whether the UI pages is later.
- **Debouncing the search box** — a UI concern for T-505b.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-505a-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read app/src/bridge/models.ts --read app/src/state/models.ts --read app/src/state/models.test.ts --read app/src/state/projects.test.ts --read app/src/bridge/comfy.ts --read src-tauri/src/catalog.rs --file app/src/bridge/catalog.ts --file app/src/state/catalog.ts --file app/src/state/catalog.test.ts
```

`bridge/models.ts` / `state/models.ts` are `--read` as the bridge+store+pure-fn pattern to mirror;
`state/models.test.ts` and `state/projects.test.ts` for the pure-sweep and bridge-mock test styles;
`bridge/comfy.ts` for `isTauri`; `src-tauri/src/catalog.rs` so the TS mirror types match the Rust
they cross the boundary from (WORKFLOW §3: definitions in view, not editable).
