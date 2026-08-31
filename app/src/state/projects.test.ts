import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Config } from '../bridge/config'
import type { Project, ProjectWarning } from '../bridge/projects'
import { useConfigStore } from './config'
import { useLibraryStore } from './library'
import {
  effectiveProjectSlug,
  projectRow,
  projectWarningLine,
  useProjectsStore,
} from './projects'

const mockListProjects = vi.fn()
const mockCreateProject = vi.fn()
const mockSaveConfig = vi.fn()
const mockLoadConfig = vi.fn()
const mockListTracks = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/projects', () => ({
  listProjects: () => mockListProjects(),
  createProject: (name: string) => mockCreateProject(name),
}))

vi.mock('../bridge/config', () => ({
  isTauri: () => mockIsTauri,
  loadConfig: () => mockLoadConfig(),
  saveConfig: (config: unknown) => mockSaveConfig(config),
  hasSecret: vi.fn(),
  setSecret: vi.fn(),
  deleteSecret: vi.fn(),
}))

vi.mock('../bridge/library', () => ({
  listTracks: () => mockListTracks(),
  subscribeTracks: vi.fn(),
}))

vi.mock('../bridge/jobs', () => ({
  isTauri: () => mockIsTauri,
}))

function baseConfig(over: Partial<Config> = {}): Config {
  return {
    schema_version: 1,
    comfy: { mode: 'local', url: null, comfy_bin: null },
    llm: null,
    default_profile_id: null,
    default_project_slug: null,
    ...over,
  }
}

function makeProject(over: Partial<Project> = {}): Project {
  return {
    slug: 'my-first-song',
    name: 'My First Song',
    created_at: '2026-08-30T10:00:00Z',
    tracks: [],
    lyrics: [],
    albums: [],
    next_lyric_seq: 1,
    next_track_seq: 1,
    ...over,
  }
}

function makeWarning(kind: ProjectWarning['kind']): ProjectWarning {
  switch (kind) {
    case 'dir_unreadable':
      return { kind, dir: '/some/dir', detail: 'denied' }
    case 'unreadable':
      return { kind, slug: 'x', detail: 'denied' }
    case 'malformed':
      return { kind, slug: 'x', detail: 'bad json' }
    case 'slug_mismatch':
      return { kind, directory: 'x', recorded: 'y' }
  }
}

describe('effectiveProjectSlug', () => {
  /**
   * Mutation check: if this function ever ignores the configured slug, the
   * picker will render a selection the backend would not honour.
   */
  it('returns the configured slug when it is still in the list', () => {
    const projects = [makeProject({ slug: 'a' }), makeProject({ slug: 'b' })]
    const config = baseConfig({ default_project_slug: 'b' })
    expect(effectiveProjectSlug(config, projects)).toBe('b')
  })

  it('falls back to the first project when nothing is configured', () => {
    const projects = [makeProject({ slug: 'a' }), makeProject({ slug: 'b' })]
    expect(effectiveProjectSlug(baseConfig(), projects)).toBe('a')
  })

  it('falls back to the first project when the configured slug is gone', () => {
    const projects = [makeProject({ slug: 'a' }), makeProject({ slug: 'b' })]
    const config = baseConfig({ default_project_slug: 'deleted' })
    expect(effectiveProjectSlug(config, projects)).toBe('a')
  })

  it('returns null when there are no projects', () => {
    expect(effectiveProjectSlug(baseConfig(), [])).toBeNull()
  })
})

describe('projectWarningLine', () => {
  it('returns null when there are no warnings', () => {
    expect(projectWarningLine([])).toBeNull()
  })

  it('uses the singular for one warning', () => {
    expect(projectWarningLine([makeWarning('unreadable')])).toContain('1 project')
  })

  it('uses the plural for multiple warnings', () => {
    expect(
      projectWarningLine([makeWarning('unreadable'), makeWarning('malformed')]),
    ).toContain('2 projects')
  })
})

describe('projectRow', () => {
  it('keeps the slug and name, and takes only the date half of the stamp', () => {
    const row = projectRow(
      makeProject({ slug: 's', name: 'N', created_at: '2026-08-30T10:00:00Z' }),
    )
    expect(row.slug).toBe('s')
    expect(row.name).toBe('N')
    expect(row.created).toBe('2026-08-30')
  })
})

describe('projects store', () => {
  beforeEach(() => {
    mockIsTauri = true
    mockListProjects.mockReset()
    mockCreateProject.mockReset()
    mockSaveConfig.mockReset()
    mockLoadConfig.mockReset()
    mockListTracks.mockReset()

    useProjectsStore.setState({
      projects: [],
      warnings: null,
      loading: false,
      error: null,
    })
    useConfigStore.setState({
      config: baseConfig(),
      warnings: [],
      status: 'ready',
      error: null,
      secrets: {},
    })
    useLibraryStore.setState({
      tracks: [],
      warnings: null,
      loading: false,
      error: null,
      listening: false,
    })

    mockListTracks.mockResolvedValue({ tracks: [], warnings: [] })
  })

  it('load_populates_projects_and_the_warning_line', async () => {
    mockListProjects.mockResolvedValue({
      projects: [makeProject({ slug: 'a' }), makeProject({ slug: 'b' })],
      warnings: [makeWarning('unreadable')],
    })

    await useProjectsStore.getState().load()

    expect(useProjectsStore.getState().projects).toHaveLength(2)
    expect(useProjectsStore.getState().warnings).toContain('1 project')
    expect(useProjectsStore.getState().loading).toBe(false)
  })

  it('load_surfaces_an_error_when_listProjects_rejects', async () => {
    mockListProjects.mockRejectedValue(new Error('disk failed'))

    await useProjectsStore.getState().load()

    expect(useProjectsStore.getState().error).toBe('disk failed')
    expect(useProjectsStore.getState().loading).toBe(false)
  })

  it('create_rejects_an_empty_name_without_calling_the_backend', async () => {
    const ok = await useProjectsStore.getState().create('   ')

    expect(ok).toBe(false)
    expect(mockCreateProject).not.toHaveBeenCalled()
    expect(useProjectsStore.getState().error).toBe('Name the project before creating it.')
  })

  it('create_trims_the_name_then_creates_selects_and_reloads_tracks', async () => {
    mockCreateProject.mockResolvedValue(
      makeProject({ slug: 'new-project', name: 'New Project' }),
    )

    const ok = await useProjectsStore.getState().create('  New Project  ')

    expect(ok).toBe(true)
    expect(mockCreateProject).toHaveBeenCalledTimes(1)
    expect(mockCreateProject).toHaveBeenLastCalledWith('New Project')
    expect(useProjectsStore.getState().projects).toHaveLength(1)
    expect(useProjectsStore.getState().projects[0]!.slug).toBe('new-project')
    expect(mockSaveConfig).toHaveBeenCalledTimes(1)
    expect(mockSaveConfig.mock.calls[0]![0]).toMatchObject({
      default_project_slug: 'new-project',
    })
    expect(mockListTracks).toHaveBeenCalledTimes(1)
  })

  it('select_rejects_a_slug_that_is_not_loaded', async () => {
    useProjectsStore.setState({ projects: [makeProject({ slug: 'a' })] })

    const ok = await useProjectsStore.getState().select('missing')

    expect(ok).toBe(false)
    expect(mockSaveConfig).not.toHaveBeenCalled()
    expect(mockListTracks).not.toHaveBeenCalled()
    expect(useProjectsStore.getState().error).toContain('missing')
  })

  it('select_saves_the_slug_and_reloads_tracks', async () => {
    useConfigStore.setState({
      config: baseConfig({ default_profile_id: 'ace-step-1.5-turbo' }),
    })
    useProjectsStore.setState({
      projects: [makeProject({ slug: 'a' }), makeProject({ slug: 'b' })],
    })

    const ok = await useProjectsStore.getState().select('b')

    expect(ok).toBe(true)
    expect(mockSaveConfig).toHaveBeenCalledTimes(1)
    const saved = mockSaveConfig.mock.calls[0]![0] as Config
    expect(saved.default_project_slug).toBe('b')
    expect(saved.default_profile_id).toBe('ace-step-1.5-turbo')
    expect(mockListTracks).toHaveBeenCalledTimes(1)
  })
})
