import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { FileProgress, ProfileStatus, Readiness } from '../bridge/models'
import { useModelsStore, curatedFirst, formatBytes, installView, isTerminal, rowFor } from './models'

const mockModelsStatus = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/models', () => ({
  modelsStatus: () => mockModelsStatus(),
  modelsInstall: vi.fn(),
  modelsProgress: vi.fn(),
}))

vi.mock('../bridge/comfy', () => ({
  isTauri: () => mockIsTauri,
}))

/** The four ACE-Step files, as captured from Hugging Face on 2026-08-25. */
const ACE_FILES = [
  { file: 'acestep_v1.5_xl_turbo_bf16.safetensors', folder: 'diffusion_models', size_bytes: 9974719892 },
  { file: 'qwen_0.6b_ace15.safetensors', folder: 'text_encoders', size_bytes: 1191588248 },
  { file: 'qwen_4b_ace15.safetensors', folder: 'text_encoders', size_bytes: 8379154232 },
  { file: 'ace_1.5_vae.safetensors', folder: 'vae', size_bytes: 337431732 },
].map((f) => ({ ...f, source_url: `https://example.invalid/${f.file}`, license: null }))

/** Every state the backend can report, so the sweep below cannot go stale. */
const ALL_STATES: Readiness[] = [
  { state: 'ready' },
  { state: 'missing', files: ACE_FILES, total_bytes: 19882894104, installable: true },
  { state: 'undeclared' },
  { state: 'unknown' },
]

function profile(over: Partial<ProfileStatus>): ProfileStatus {
  return {
    id: 'x',
    display_name: 'X',
    kind: 'music',
    license: 'Apache-2.0',
    license_notes: null,
    source: 'shipped',
    vram_gb_min: null,
    template: null,
    readiness: { state: 'ready' },
    ...over,
  }
}

beforeEach(() => {
  mockModelsStatus.mockReset()
  mockIsTauri = true
  useModelsStore.setState({
    view: null,
    busy: false,
    installing: null,
    progress: [],
  })
})

describe('rowFor', () => {
  /**
   * Protects a product rule, not a rendering detail: CONVENTIONS requires
   * user-facing errors to say what to do next. Adding a state without a next
   * step fails here.
   */
  it('gives every state that is not ready a next step', () => {
    for (const readiness of ALL_STATES) {
      const row = rowFor(readiness)
      if (readiness.state === 'ready') {
        expect(row.nextStep).toBeNull()
        expect(row.tone).toBe('ok')
      } else {
        expect(row.nextStep, `${readiness.state} must say what to do next`).not.toBeNull()
        expect(row.nextStep).not.toBe('')
      }
    }
  })

  /**
   * Protects the most damaging confusion this step could make. A stopped
   * ComfyUI must never read as "not installed": ACE-Step is an 18.5 GiB
   * download, and a user who already has it must not be sent to fetch it again
   * because their server happens to be off.
   */
  it('never presents an uncheckable state as not installed', () => {
    for (const readiness of [{ state: 'unknown' } as const, { state: 'undeclared' } as const]) {
      const row = rowFor(readiness)
      expect(row.label).not.toContain('Not installed')
      expect(row.tone).toBe('neutral')
      expect(row.download).toBeNull()
    }
  })

  /** Protects: a stopped ComfyUI points at the step that fixes it. */
  it('sends an unknown row back to the ComfyUI step', () => {
    expect(rowFor({ state: 'unknown' }).nextStep).toContain('Start ComfyUI')
  })

  /**
   * Protects: the size reaches the user before they commit to the download.
   * 18.5 GiB is not something to start without being told.
   */
  it('states the download size and file count before installing', () => {
    const row = rowFor({
      state: 'missing',
      files: ACE_FILES,
      total_bytes: 19882894104,
      installable: true,
    })
    expect(row.nextStep).toContain('4 files')
    expect(row.nextStep).toContain('18.5 GiB')
    expect(row.tone).toBe('warn')
  })

  /**
   * Protects: an unknown total is omitted rather than guessed at. Showing a
   * partial sum as if it were the whole download understates the cost.
   */
  it('omits the size when the total is unknown', () => {
    const row = rowFor({ state: 'missing', files: ACE_FILES, total_bytes: null, installable: true })
    expect(row.nextStep).toContain('4 files')
    expect(row.nextStep).not.toContain('GiB')
    expect(row.nextStep).not.toContain('(')
  })

  /**
   * Protects: files the app cannot fetch get hand-placement instructions, not
   * an Install button that would half-work.
   */
  it('asks for hand placement when a file has no source', () => {
    const row = rowFor({
      state: 'missing',
      files: [{ ...ACE_FILES[0], source_url: null }],
      total_bytes: null,
      installable: false,
    })
    expect(row.nextStep).toContain('by hand')
    expect(row.download).toBeNull()
  })
})

describe('formatBytes', () => {
  /** Protects: unknown stays unknown, and zero is not a size. */
  it('returns null for unknown or zero', () => {
    expect(formatBytes(null)).toBeNull()
    expect(formatBytes(0)).toBeNull()
  })

  /** Protects: the units the model hosts themselves use. */
  it('formats the captured sizes', () => {
    expect(formatBytes(19882894104)).toBe('18.5 GiB')
    expect(formatBytes(337431732)).toBe('322 MiB')
  })
})

describe('curatedFirst', () => {
  /**
   * Protects: shipped-and-working first, user profiles last. The order is what
   * makes the step readable at a glance -- a user who has a model installed
   * should not have to read past three they do not.
   */
  it('puts shipped ready profiles first and user profiles last', () => {
    const ordered = curatedFirst([
      profile({ id: 'mine', display_name: 'Mine', source: 'user' }),
      profile({ id: 'ace', display_name: 'ACE', readiness: { state: 'unknown' } }),
      profile({ id: 'minimax', display_name: 'MiniMax' }),
    ])
    expect(ordered.map((p) => p.id)).toEqual(['minimax', 'ace', 'mine'])
  })
})

describe('isTerminal', () => {
  /**
   * Protects: only the two states that actually stop are terminal. `unknown`
   * is what a failed *poll* reports -- treating it as terminal would declare a
   * 9 GiB transfer finished because one status call timed out.
   */
  it('treats only completed and failed as terminal', () => {
    expect(isTerminal('completed')).toBe(true)
    expect(isTerminal('failed')).toBe(true)
    for (const live of ['starting', 'downloading', 'unknown', '']) {
      expect(isTerminal(live), `${live} is still running`).toBe(false)
    }
  })
})

function at(status: string, done: number, total: number | null): FileProgress {
  return {
    download_id: `${status}-${done}`,
    status,
    completed_bytes: done,
    total_bytes: total,
    percent: null,
    error: null,
  }
}

describe('installView', () => {

  /**
   * Protects: progress is weighted by bytes, not by file count. ACE-Step's
   * four files run from 0.3 to 9.3 GiB, so "1 of 4" can mean 2% or 50% -- a
   * file-counted bar would sit at 25% through most of a 9 GiB transfer.
   */
  it('weights the percentage by bytes, not by file count', () => {
    const view = installView([
      at('completed', 337431732, 337431732),
      at('downloading', 0, 9974719892),
    ])
    expect(view.done).toBe(1)
    expect(view.total).toBe(2)
    expect(view.percent).toBe(3)
  })

  /**
   * Protects: an unknown size makes the whole bar unknown rather than wrong.
   * `total_bytes` is null until the server sends a content length, so a bar
   * computed from the files that have reported would leap backwards.
   */
  it('reports no percentage while any size is unknown', () => {
    const view = installView([at('downloading', 10, null), at('downloading', 10, 100)])
    expect(view.percent).toBeNull()
    expect(view.done).toBe(0)
  })

  /** Protects: failures are surfaced, not averaged away into the bar. */
  it('collects failed files', () => {
    const view = installView([at('failed', 0, 100), at('completed', 100, 100)])
    expect(view.failed).toHaveLength(1)
    expect(view.done).toBe(1)
  })
})

describe('useModelsStore', () => {
  /** Protects: two mount-time refreshes collapse to one round trip. */
  it('dedupes a second refresh while one is in flight', async () => {
    mockModelsStatus.mockResolvedValue({
      profiles: [],
      warnings: [],
      inventory_available: true,
      inventory_detail: null,
    })
    const first = useModelsStore.getState().refresh()
    const second = useModelsStore.getState().refresh()
    await Promise.all([first, second])
    expect(mockModelsStatus).toHaveBeenCalledTimes(1)
  })

  /** Protects: the busy guard is the early-return path, not the bridge. */
  it('returns early when busy is already true', async () => {
    useModelsStore.setState({ busy: true })
    await useModelsStore.getState().refresh()
    expect(mockModelsStatus).not.toHaveBeenCalled()
  })
})
