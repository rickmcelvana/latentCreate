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
  return `${count} ${noun} could not be read; check the projects folder in the app data dir.`
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
