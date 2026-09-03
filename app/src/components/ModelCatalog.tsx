import { useEffect, useMemo, useRef } from 'react'
import type { CatalogKind, TemplateInfo } from '../bridge/catalog'
import type { ProfileStatus } from '../bridge/models'
import { RoleMapping } from './RoleMapping'
import { curatedIndex, rowViewFor, useCatalogStore } from '../state/catalog'
import { useImportStore } from '../state/import'
import { installView, rowFor, useModelsStore } from '../state/models'

const KINDS: { kind: CatalogKind; label: string }[] = [
  { kind: 'audio', label: 'Audio' },
  { kind: 'image', label: 'Image' },
]

/** Debounce the search so a browse does not fire on every keystroke. */
const SEARCH_DEBOUNCE_MS = 300

/**
 * The Setup "Model catalog" step: browse the gallery for one kind at a time,
 * search within it, and see per-row readiness. One step with an Audio | Image
 * toggle -- the store is a singleton, so one kind is shown at a time (T-505b
 * brief). Curated rows install with one click (T-505c); ready bare rows are
 * brought in as user profiles (T-505d).
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

  const modelsView = useModelsStore((s) => s.view)
  const refreshModels = useModelsStore((s) => s.refresh)
  useEffect(() => {
    void refreshModels()
  }, [refreshModels])
  const curated = useMemo(() => curatedIndex(modelsView), [modelsView])

  // The row that owns an open bring-in can leave the page: the kind toggle and
  // a search both replace `page`. Its mapping screen would go with it, and
  // because the store allows one flow at a time, the user could then reach
  // neither Cancel nor any new import -- a dead end needing a restart. So when
  // the row is gone, the step keeps the screen.
  const adopting = useImportStore((s) => s.adopting)
  const orphaned = adopting !== null && !(page?.rows ?? []).some((r) => r.name === adopting)

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
          {page.rows.map((row) => {
            // Branch here, not inside a row: a curated and a bare row call
            // different hooks, so they must be different components. When the
            // models view lands and a row turns curated, this swaps the whole
            // component (same key), a clean remount -- never a change in the
            // hook order of one instance (rules of hooks).
            const profile = curated.get(row.name)
            return profile !== undefined ? (
              <CuratedRow key={row.name} row={row} profile={profile} />
            ) : (
              <BareRow key={row.name} row={row} />
            )
          })}
        </ul>
      ) : null}

      {/* The open bring-in whose row is no longer listed. Named, because a
          mapping screen with no row above it says nothing about what it is
          mapping. */}
      {orphaned ? (
        <>
          <p className="setup-next-step">Bringing in {adopting}.</p>
          <RoleMapping savedLabel="It is in the Models step above." />
        </>
      ) : null}
    </section>
  )
}

/** A bare gallery row -- no shipped profile: its title, blurb, tags, and a
 *  readiness pill resolved from `local_check` when it first comes into view. */
function BareRow({ row }: { row: TemplateInfo }) {
  const verdict = useCatalogStore((s) => s.readiness[row.name])
  const check = useCatalogStore((s) => s.checkReadiness)
  const adopt = useImportStore((s) => s.adopt)
  const adopting = useImportStore((s) => s.adopting)
  const phase = useImportStore((s) => s.phase)
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
          for a URL -- MCP-SURFACE 33). */}
      {view !== null && view.reasons.length > 0 ? (
        <ul className="catalog-row-reasons">
          {view.reasons.map((reason, i) => (
            <li key={i}>{reason}</li>
          ))}
        </ul>
      ) : null}

      {/* Only a row this install can already run, and only when no other import
          flow is open -- the store refuses a second one, and a live button that
          silently does nothing is worse than a disabled one. */}
      {view !== null &&
      verdict !== undefined &&
      verdict !== 'checking' &&
      verdict.kind === 'ready' &&
      adopting === null ? (
        <div className="catalog-row-actions">
          <button
            type="button"
            className="setup-button setup-button-primary"
            onClick={() => void adopt(row.name, row.title)}
            disabled={phase.kind !== 'idle'}
          >
            Bring in
          </button>
        </div>
      ) : null}

      {/* The mapping screen, under the row it belongs to. */}
      {adopting === row.name ? (
        <RoleMapping savedLabel="It is in the Models step above." />
      ) : null}
    </li>
  )
}

/** A gallery row the app ships a profile for: readiness and one-click install
 *  come from the profile via the shared models store, never local_check. */
function CuratedRow({ row, profile }: { row: TemplateInfo; profile: ProfileStatus }) {
  const install = useModelsStore((s) => s.install)
  const installing = useModelsStore((s) => s.installing)
  const progress = useModelsStore((s) => s.progress)

  const view = rowFor(profile.readiness)
  const active = installing === profile.id
  const live = active ? installView(progress) : null

  return (
    <li className="catalog-row">
      <div className="catalog-row-head">
        <span className="catalog-row-title">{row.title || row.name}</span>
        <span className={`status-pill status-pill-${view.tone}`}>{view.label}</span>
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

      {/* Shown wherever a model is installed -- some weights are open with
          conditions the user takes on by generating (CONVENTIONS). */}
      <p className="model-row-license">
        <span className="model-row-license-name">{profile.license}</span>
        {profile.license_notes !== null ? ` -- ${profile.license_notes}` : null}
      </p>

      {view.nextStep !== null && !active ? (
        <p className="setup-next-step">{view.nextStep}</p>
      ) : null}

      {live !== null ? (
        <p className="setup-next-step">
          Downloading {live.done} of {live.total} files
          {live.percent === null ? '' : ` -- ${live.percent}%`}
          {live.failed.length > 0 ? ` -- ${live.failed.length} failed` : ''}
        </p>
      ) : null}

      {profile.readiness.state === 'missing' && profile.readiness.installable ? (
        <div className="catalog-row-actions">
          <button
            type="button"
            className="setup-button setup-button-primary"
            onClick={() => void install(profile.id)}
            disabled={installing !== null}
          >
            {active ? 'Downloading...' : 'Install'}
          </button>
        </div>
      ) : null}
    </li>
  )
}
