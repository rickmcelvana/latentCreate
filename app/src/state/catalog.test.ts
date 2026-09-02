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
