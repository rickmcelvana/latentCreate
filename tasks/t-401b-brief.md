# T-401b — projects become first-class: the picker

**Lane: Aider.** UI wiring plus a decisions store in the shape T-303 and T-311e proved.
**Depends:** **T-401a landed** (the `projects_list`/`projects_create` commands and the
`default_project_slug` config field must exist). | **Crate/dir:** `app/src`.

**Files to create/modify:**

- `app/src/bridge/projects.ts` — **new**, typed mirror + wrappers
- `app/src/state/projects.ts` — **new**, the decisions and the store
- `app/src/state/projects.test.ts` — **new**
- `app/src/views/Library.tsx` — the project picker above the track list
- `app/src/theme.css` — new rules for every new class

---

## Goal

A project picker in the Library view: list the projects, create one, select one. Selecting
persists `default_project_slug` through the existing config store — the **same mechanism**
`default_profile_id` uses (T-303) — and the track list follows the selection, because
`library_tracks` already resolves through `projectctx::selected_project` (T-401a).

## The one design rule that decides everything

**The selection lives in config, nowhere else.** The picker does not keep a `selected` field in
its store; it derives the effective selection from `config.default_project_slug` plus the project
list, exactly as `state/profiles.ts` derives `selectedProfile` from config plus the model list. One
source of truth; two copies of "which project" is the phase file's trap one layer up.

## Spec

### 1. `app/src/bridge/projects.ts`

Mirror the Rust types with `Mirrors Rust ...` doc comments, following `bridge/library.ts`'s style:

```ts
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
```

### 2. `app/src/state/projects.ts` — every decision, and nothing else

The phase file's habit, applied before any JSX: the three decisions the view would otherwise
derive inline are functions a test can reach.

```ts
import { create } from 'zustand'
import {
  createProject,
  listProjects,
  type Project,
  type ProjectWarning,
} from '../bridge/projects'
import type { Config } from '../bridge/config'
import { useConfigStore } from './config'
import { useLibraryStore } from './library'

/**
 * The project the app is working in, derived the way the backend resolves it
 * (T-401a): the configured slug when it is still one of the listed projects,
 * else the first, else `null` (nothing to select yet -- the backend creates
 * `My First Song` on first use, but the frontend cannot create without a
 * name).
 *
 * `null` and a configured-but-gone slug both resolve to the first project, so
 * the picker never shows a selection the backend would not honour.
 */
export function effectiveProjectSlug(config: Config | null, projects: Project[]): string | null {
  const slugs = projects.map((project) => project.slug)
  const configured = config?.default_project_slug ?? null
  if (configured !== null && slugs.includes(configured)) return configured
  return slugs[0] ?? null
}

/** One picker row, with the date decision already made. */
export interface ProjectRow {
  slug: string
  /** The user's name -- never the slug; a rename keeps the slug. */
  name: string
  /** The date half of the RFC 3339 stamp, e.g. `'2026-08-30'`. */
  created: string
}

export function projectRow(project: Project): ProjectRow {
  return {
    slug: project.slug,
    name: project.name,
    // The stamp is already the truth; parsing it into a `Date` would
    // reintroduce a timezone. Same rule as the library store's `created`.
    created: project.created_at.split('T')[0],
  }
}

/**
 * A single sentence describing project warnings, or `null` when there are none.
 * Same shape as `useLibraryStore.warnings`: never a modal (CONVENTIONS).
 */
export function projectWarningLine(warnings: ProjectWarning[]): string | null {
  if (warnings.length === 0) return null
  const count = warnings.length
  const noun = count === 1 ? 'project' : 'projects'
  return `${count} project ${noun} could not be read; check the projects folder in the app data dir.`
}

interface ProjectsState {
  projects: Project[]
  /** The rendered warning line, or `null`. */
  warnings: string | null
  loading: boolean
  error: string | null
  load: () => Promise<void>
  /** Create a project and select it. Resolves `true` on success. */
  create: (name: string) => Promise<boolean>
  /** Persist the selection and reload the track list. */
  select: (slug: string) => Promise<boolean>
}

export const useProjectsStore = create<ProjectsState>((set, get) => ({
  projects: [],
  warnings: null,
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null })
    try {
      // Named `projectSet`, not `set`: zustand's `set` is the store updater,
      // and shadowing it with the response would call a ProjectSet as a
      // function. The library store names its response `trackSet` for the
      // same reason.
      const projectSet = await listProjects()
      set({
        projects: projectSet.projects,
        warnings: projectWarningLine(projectSet.warnings),
        loading: false,
      })
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err), loading: false })
    }
  },

  create: async (name) => {
    const trimmed = name.trim()
    if (trimmed === '') {
      set({ error: 'Name the project before creating it.' })
      return false
    }
    try {
      const project = await createProject(trimmed)
      // The backend minted it, so it is on disk now. Append rather than
      // re-list, and select it: creating a project is asking to work in it.
      set({ projects: [...get().projects, project], error: null })
      return await get().select(project.slug)
    } catch (err: unknown) {
      set({ error: err instanceof Error ? err.message : String(err) })
      return false
    }
  },

  select: async (slug) => {
    if (!get().projects.some((project) => project.slug === slug)) {
      set({ error: `No project named "${slug}" is loaded.` })
      return false
    }
    await useConfigStore.getState().save({ default_project_slug: slug })
    // The track list belongs to the selected project, so a switch must reload
    // it -- the frontend half of "generate, ingest, lyricdoc and tracks all
    // target the same project".
    await useLibraryStore.getState().load()
    return true
  },
}))
```

### 3. `app/src/state/projects.test.ts`

Mock the **bridges**, keep the **stores** real, preload state with `setState` — the pattern
`llm.test.ts` already uses (it mocks `../bridge/config` and drives the real `useConfigStore`).

Mocks: `../bridge/projects` (`listProjects`, `createProject` spies), `../bridge/config`
(`saveConfig` spy; the real config store calls it), `../bridge/library` (`listTracks` spy — the
real library store calls it when `select` reloads). `beforeEach` resets all spies and sets the
stores to a clean state.

Tests, each naming its invariant:

- **`effectiveProjectSlug`**:
  - configured slug present in the list → that slug.
  - configured `null` → the first project's slug.
  - **configured slug absent (deleted on disk) → the first project's slug, not the dead one and
    not `null`** — the case the picker must render truthfully (what the backend would resolve).
  - empty list → `null`.
- **`projectWarningLine`**: `[]` → `null`; one warning → a sentence containing `1 project`;
  two → `2 projects`.
- **`projectRow`**: name and slug pass through; `created` is the date half of the stamp
  (`'2026-08-30T10:00:00Z'` → `'2026-08-30'`).
- **Store `load`**: populates `projects` and the rendered warning line; a rejecting `listProjects`
  sets `error` (not `loading`).
- **Store `create`**:
  - `'   '` → `false`, `error` set, **`createProject` not called**.
  - a real name → `createProject` called with the trimmed name; the returned project is appended
    to the list; **`saveConfig` is called with `default_project_slug` equal to the created slug**
    (the create-then-select flow), and **`listTracks` is called** (the library reload).
- **Store `select`**:
  - a slug not in the list → `false`, `error` set, `saveConfig` not called.
  - a listed slug → `true`; `saveConfig` called with a config whose `default_project_slug` is that
    slug (preload the config store so the merge is observable); `listTracks` called.
- **Mutation check**: `effectiveProjectSlug` ignoring the configured slug must fail its first test
  — that is the phase file's trap in its frontend form.

No rendering tests: vitest runs in `node` with no DOM, and the view renders `projectRow` fields
with no logic of its own (T-301b's rule).

### 4. `app/src/views/Library.tsx` — the picker

Above the track list, a panel mirroring the profile picker in `AudioStudio.tsx` (radio rows,
selected highlighted):

```tsx
<section className="panel project-picker">
  <h2 className="project-picker-title">Project</h2>

  {projectError !== null ? <p className="library-error">{projectError}</p> : null}
  {projectWarnings !== null ? <p className="library-warning">{projectWarnings}</p> : null}

  <ul className="project-list">
    {rows.map((row) => (
      <ProjectRow
        key={row.slug}
        row={row}
        selected={row.slug === selected}
        onSelect={() => void selectProject(row.slug)}
      />
    ))}
  </ul>

  <ProjectCreate />
</section>
```

where `selected = effectiveProjectSlug(config, projects)` and `rows = projects.map(projectRow)` —
**both computed from store selectors, never inline**: `config` from `useConfigStore`, `projects`
from `useProjectsStore`.

`ProjectRow` mirrors `ProfilePickerRow`: a radio input named `"project"`, the name, and the
created date in a meta line. The track list below is **unchanged** — same store, same `TrackCard`,
same retry/warning/empty states. `Library.tsx`'s react import gains `useState` (it currently
imports only `useEffect`).

`ProjectCreate` is an inline form:

```tsx
function ProjectCreate() {
  const [name, setName] = useState('')
  const create = useProjectsStore((state) => state.create)
  return (
    <form
      className="project-create"
      onSubmit={(event) => {
        event.preventDefault()
        void create(name).then((ok) => {
          if (ok) setName('')
        })
      }}
    >
      <input
        className="project-create-input"
        type="text"
        value={name}
        placeholder="New project name"
        onChange={(event) => setName(event.target.value)}
      />
      <button type="submit" className="project-create-button" disabled={name.trim() === ''}>
        Create
      </button>
    </form>
  )
}
```

The input keeps its text when creation fails, so the user can fix and retry; it clears only on
success. The store's empty-name guard is the tested one; the disabled button is convenience.

The Library's existing `useEffect(() => { void load() }, [load])` mount effects stay — the
projects store loads its own list in its own mount effect. Order does not matter: whichever store
loads first, the backend resolves the same effective project for the track list.

### 5. `theme.css`

Rules for every new class (WORKFLOW §4.5), **existing tokens only, no existing rule changed**:

- `.project-picker`, `.project-picker-title` — mirror `.profile-picker` / `.profile-picker-title`
  (theme.css lines ~557-570).
- `.project-list`, `.project-row`, `.project-row-selected`, `.project-row-pick`,
  `.project-row-name`, `.project-row-meta`, `.project-row-created` — mirror `.profile-list` /
  `.profile-row` / `.profile-row-selected` / `.profile-row-pick` / `.profile-row-name` /
  `.profile-row-meta` (lines ~578-621).
- `.project-create`, `.project-create-input`, `.project-create-button` — the input mirrors
  `.import-field input` (line ~1752) and the button mirrors `.import-button` (line ~1693), with a
  disabled state mirroring `.import-button:disabled`.

The picker's error and warning lines **reuse `.library-error` and `.library-warning`** — they are
in the same view, and a fourth copy of the retry-button styling is exactly the debt PROJECT.md's
backlog names. No retry button: `projects_list` never fails (warnings live inside `ProjectSet`),
so the only errors are create/select validation, which the user fixes by acting.

## Acceptance criteria

- [ ] `npm run gate` green (this task's Rust half is untouched, but the gate runs it).
- [ ] All tests above pass; the mutation check on `effectiveProjectSlug` is run.
- [ ] No changes outside the listed files; no existing `theme.css` rule modified.
- [ ] No `invoke`/`listen` outside `app/src/bridge/` (WORKFLOW §4 item 5).

**Producer click-through** — the persistence half is invisible to the gate (the T-212/T-303
lesson):
- [ ] Create a second project from the Library; it appears selected immediately, and the track
      list is empty (fresh project). **Open `config.json` and confirm `default_project_slug` is
      written** — not the UI showing it, the file.
- [ ] Switch back to the first project: its tracks return.
- [ ] Restart the app: the selection is still there, and the Library opens on the selected
      project's tracks.
- [ ] Generate from the Audio Studio with project B selected: the track lands in
      `projects/<b-slug>/tracks/`, and the Library (still on B) shows it.

## Out of scope

- **The backend seam** — T-401a; this task assumes it landed.
- **Tracks UI beyond the existing list** — playback, delete, rename, export, reveal, send-to are
  T-402 … T-405.
- **Album lists** — T-403.
- **A "rename project" affordance** — `library::projects` has no rename; not in this phase's
  T-401 scope.
- **The LyricsStudio reacting to a mid-session selection change** — cannot happen today: the
  picker lives only in the Library view, and view switching remounts the studios, so `lyrics_open`
  re-resolves the selected project on every visit. If a picker ever appears in another view, that
  seam gets its own task.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-401b-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/views/AudioStudio.tsx --read app/src/state/profiles.ts --read app/src/state/library.ts --read app/src/state/config.ts --read app/src/state/llm.test.ts --read app/src/theme.css --file app/src/bridge/projects.ts --file app/src/state/projects.ts --file app/src/state/projects.test.ts --file app/src/views/Library.tsx --file app/src/theme.css
```

`AudioStudio.tsx` is the picker template (radio rows + `save({ ... })`), `state/profiles.ts` the
selector template (`effectiveProfileId`/`selectedProfile`), `state/library.ts` the store template
and the store this one reloads, `state/config.ts` the store this one saves through, and
`state/llm.test.ts` the mocking pattern for a store test that drives another store. `theme.css` is
`--file` (new rules) and `--read` (the `.profile-*` block to mirror); the brief forbids editing
existing rules.
