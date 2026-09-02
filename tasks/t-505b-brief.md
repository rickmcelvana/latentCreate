# T-505b — Model catalog: the `<ModelCatalog>` component + Setup wiring

**Lane: Aider.** Frontend-only view work over the T-505a store: one new component, one Setup wiring,
its CSS. **Depends:** T-505a landed (`state/catalog.ts`, `bridge/catalog.ts`). **Dir:** `app/src`.
**This is the first catalog lane with a producer click-through.**

**Files to create/modify:**

- `app/src/components/ModelCatalog.tsx` — **new**: the browse step (Audio | Image toggle, search,
  rows with readiness pills)
- `app/src/views/Setup.tsx` — render `<ModelCatalog />` as a Setup step (import + one line)
- `app/src/theme.css` — the catalog styles

**No store change.** `state/catalog.ts` / `bridge/catalog.ts` are correct as landed and are
`--read` only.

---

## Goal

The catalog becomes visible: a "Model catalog" step on the Setup page where a user browses the
gallery for **audio or image** models (a toggle switches kind), searches, and sees per-row readiness
("Installed" / "Not installed" + what's missing / "Can't check"). This lane is **browse + readiness
only** — installing a curated model (T-505c) and adopting a gallery row into a profile (T-505d) are
later; the rows carry no install/adopt button yet.

## The one design decision, and why (do not change it)

The T-505a store is a **singleton** (`useCatalogStore`) holding one kind's page. The owner's
requirement is that **both audio and image models are browsable on the Setup page**. Two catalog
components rendered at once would fight over the singleton, and two store instances would force a
rules-of-hooks-awkward prop-store. So the catalog is **one step with an `Audio | Image` toggle** —
only one kind is shown at a time, the singleton is correct as-is, and the requirement is met. Do not
render two `<ModelCatalog>`s, and do not refactor the store.

## Readiness is resolved lazily, per visible row

The image gallery is ~163 rows and each readiness is a `catalog_readiness` (a `get_template`
round-trip). Checking every row on load would fire 163 calls. So a row checks its own readiness
**when it first scrolls into view** (an `IntersectionObserver`, disconnected after the first hit).
The store already dedupes `checkReadiness`, so repeat fires are safe. Like the visualizer, this
observer is DOM glue verified by click-through, not a unit test (WORKFLOW §5).

## Spec

### 1. `app/src/components/ModelCatalog.tsx`

```tsx
import { useEffect, useRef } from 'react'
import type { CatalogKind, TemplateInfo } from '../bridge/catalog'
import { rowViewFor, useCatalogStore } from '../state/catalog'

const KINDS: { kind: CatalogKind; label: string }[] = [
  { kind: 'audio', label: 'Audio' },
  { kind: 'image', label: 'Image' },
]

/** Debounce the search so a browse does not fire on every keystroke. */
const SEARCH_DEBOUNCE_MS = 300

/**
 * The Setup "Model catalog" step: browse the gallery for one kind at a time,
 * search within it, and see per-row readiness. One step with an Audio|Image
 * toggle -- the store is a singleton, so one kind is shown at a time (T-505b
 * brief). Installing a curated model (T-505c) and adopting a row into a profile
 * (T-505d) are not here; rows carry no action button yet.
 */
export function ModelCatalog() {
  const kind = useCatalogStore((s) => s.kind)
  const query = useCatalogStore((s) => s.query)
  const page = useCatalogStore((s) => s.page)
  const busy = useCatalogStore((s) => s.busy)
  const error = useCatalogStore((s) => s.error)
  const open = useCatalogStore((s) => s.open)
  const search = useCatalogStore((s) => s.search)
  const reload = useCatalogStore((s) => s.reload)

  // Load the default kind once on mount. Switching kinds is the toggle below.
  useEffect(() => {
    void open('audio')
  }, [open])

  // Debounced search: hold the timer across renders, fire the store call once
  // typing settles, and clear it on unmount so a late browse cannot land after
  // the step is gone.
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => () => {
    if (timer.current !== null) clearTimeout(timer.current)
  }, [])
  const onSearch = (value: string) => {
    if (timer.current !== null) clearTimeout(timer.current)
    timer.current = setTimeout(() => void search(value), SEARCH_DEBOUNCE_MS)
  }

  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">Model catalog</h2>
        <div className="catalog-kinds" role="tablist" aria-label="Model kind">
          {KINDS.map((k) => (
            <button
              key={k.kind}
              type="button"
              role="tab"
              aria-selected={kind === k.kind}
              className={`catalog-kind ${kind === k.kind ? 'catalog-kind-active' : ''}`}
              onClick={() => void open(k.kind)}
            >
              {k.label}
            </button>
          ))}
        </div>
      </header>

      <p className="setup-next-step">
        Browse the models your ComfyUI can run, and see which you already have. Bringing a new one in
        comes next.
      </p>

      <input
        type="search"
        className="lyrics-input catalog-search"
        defaultValue={query}
        // `key` on the kind resets the field when the kind changes, since `open`
        // clears the query and this is an uncontrolled input.
        key={kind}
        placeholder={`Search ${kind} models`}
        aria-label={`Search ${kind} models`}
        onChange={(e) => onSearch(e.target.value)}
      />

      {error !== null ? (
        <p className="setup-next-step">
          {error}{' '}
          <button type="button" className="setup-button" onClick={() => void reload()}>
            Retry
          </button>
        </p>
      ) : null}

      {busy && page === null ? <p className="setup-next-step">Loading...</p> : null}

      {page !== null && page.widened ? (
        <p className="setup-next-step">No exact match -- showing the closest models.</p>
      ) : null}

      {page !== null && page.rows.length === 0 && !busy ? (
        <p className="setup-next-step">No {kind} models match.</p>
      ) : null}

      {page !== null && page.rows.length > 0 ? (
        <ul className="catalog-list">
          {page.rows.map((row) => (
            <CatalogRow key={row.name} row={row} />
          ))}
        </ul>
      ) : null}
    </section>
  )
}

/** One gallery row: its title, blurb, tags, and a readiness pill resolved when
 *  the row first comes into view. */
function CatalogRow({ row }: { row: TemplateInfo }) {
  const verdict = useCatalogStore((s) => s.readiness[row.name])
  const check = useCatalogStore((s) => s.checkReadiness)
  const ref = useRef<HTMLLIElement | null>(null)

  // Resolve readiness once, when the row first scrolls into view. The store
  // dedupes, so a re-observe is harmless; disconnect after the first hit keeps
  // it to one call per row.
  useEffect(() => {
    const el = ref.current
    if (el === null) return
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        void check(row.name)
        observer.disconnect()
      }
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [check, row.name])

  // No pill until the row has been checked; the observer populates it.
  const view = verdict === undefined ? null : rowViewFor(verdict)

  return (
    <li className="catalog-row" ref={ref}>
      <div className="catalog-row-head">
        <span className="catalog-row-title">{row.title || row.name}</span>
        {view !== null ? (
          <span className={`status-pill status-pill-${view.tone}`}>{view.label}</span>
        ) : null}
      </div>

      {row.description !== '' ? <p className="catalog-row-desc">{row.description}</p> : null}

      {row.tags.length > 0 ? (
        <div className="catalog-tags">
          {row.tags.map((tag) => (
            <span key={tag} className="catalog-tag">
              {tag}
            </span>
          ))}
        </div>
      ) : null}

      {/* What's missing, when a row is not runnable here -- the filenames from
          local_check.errors, shown verbatim (third-party prose, never parsed
          for a URL -- MCP-SURFACE §33). */}
      {view !== null && view.reasons.length > 0 ? (
        <ul className="catalog-row-reasons">
          {view.reasons.map((reason, i) => (
            <li key={i}>{reason}</li>
          ))}
        </ul>
      ) : null}
    </li>
  )
}
```

### 2. `app/src/views/Setup.tsx`

- Import at the top with the other component/state imports:
  ```tsx
  import { ModelCatalog } from '../components/ModelCatalog'
  ```
- Render it as a step, **after `<ModelsStep />`** and before `<LlmStep />`, in the `Setup` return:
  ```tsx
        <ModelsStep />
        <ModelCatalog />
        <LlmStep />
  ```

The existing `ModelsStep` (curated shipped audio profiles + install) stays exactly as it is — the
catalog is the *gallery browse* beside it, not a replacement. Nothing else in `Setup.tsx` changes.

### 3. `app/src/theme.css`

Add catalog styles near the other `setup-*` / `model-row` rules. Match the existing look: rows read
like `.model-row`, the toggle like a segmented control, the pill reuses `.status-pill`. Use the
existing tokens (`--gap-*`, `--panel`, `--border`, `--radius`, `--accent`, `--text-muted`,
`--bg`). Reference values (adjust to sit with the surrounding file, but keep the class names — the
component depends on them):

```css
/* --- Model catalog (Setup) --- */

.catalog-kinds {
  display: flex;
  gap: var(--gap-xs);
}

.catalog-kind {
  padding: var(--gap-xs) var(--gap-md);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
  transition:
    border-color var(--transition),
    color var(--transition);
}

.catalog-kind-active {
  border-color: var(--accent);
  color: var(--accent);
}

.catalog-search {
  width: 100%;
  margin: var(--gap-sm) 0;
}

.catalog-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  /* The gallery is long; keep the step from taking over the page. */
  max-height: 460px;
  overflow-y: auto;
}

.catalog-row {
  padding: var(--gap-md);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
}

.catalog-row-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-md);
}

.catalog-row-title {
  color: var(--text);
  font-size: 14px;
  font-weight: 600;
}

.catalog-row-desc {
  margin: var(--gap-xs) 0 0;
  color: var(--text-muted);
  font-size: 13px;
}

.catalog-tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-xs);
  margin-top: var(--gap-sm);
}

.catalog-tag {
  padding: 2px var(--gap-sm);
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 999px;
  color: var(--text-muted);
  font-size: 11px;
}

.catalog-row-reasons {
  margin: var(--gap-sm) 0 0;
  padding-left: var(--gap-lg);
  color: var(--text-muted);
  font-size: 12px;
}
```

If any token above is not defined in `theme.css`, use the nearest one that is (do not invent new
`--vars`); `.model-row` and `.status-pill` are the rules to match against.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] The step renders under Setup with an `Audio | Image` toggle; switching kinds reloads the list
      and clears the search field.
- [ ] Rows show a readiness pill that resolves as they scroll into view; a not-installed row lists
      its missing files; nothing renders a URL or a download/adopt button (out of scope).
- [ ] A search narrows the list (debounced), and an empty search restores the full kind.
- [ ] `ModelsStep` and every other Setup step are unchanged.
- [ ] Only the three listed files change.

## Producer click-through (after the gate)

1. Setup → the **Model catalog** step. Audio is selected; the list fills with audio models, each
   getting an "Installed" / "Not installed" / "Can't check" pill as it appears.
2. Toggle **Image** → the list swaps to image models (the 163-row gallery), search field clears.
3. Type "flux" → the list narrows; clear it → it restores.
4. A not-installed row shows what it's missing; no row offers a download or adopt button yet.
5. Stop ComfyUI and Retry the step (or open it stopped) → rows read "Can't check", never "Not
   installed", and no console errors.

## Out of scope (T-505c/d, T-506)

- **Install / adopt buttons on a row** — curated one-click install (T-505c) and adopt-to-profile
  (T-505d).
- **Paging past the first 100 rows** — the store loads `offset: 0`; a "load more" is later if needed.
- **Merging the curated `ModelsStep` into the catalog** — they coexist for now.
- **ARCHITECTURE §10/§10a wording** (the step layout — one catalog with a toggle rather than a
  separate Cover-art step) — the architect updates the doc when this lands; the executor does not
  touch it.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-505b-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read app/src/state/catalog.ts --read app/src/bridge/catalog.ts --read app/src/components/AlbumPanel.tsx --file app/src/components/ModelCatalog.tsx --file app/src/views/Setup.tsx --file app/src/theme.css
```

`state/catalog.ts` and `bridge/catalog.ts` are `--read`: the component consumes the store and its
types without editing them (WORKFLOW §3). `components/AlbumPanel.tsx` is `--read` as a nearby example
of a store-consuming list component in this codebase's style.
