import { describe, expect, it } from 'vitest'
import type { LlmModelRow, LlmStatus, LlmTestResult } from '../bridge/llm'
import { canTest, modelView, testSummary } from './llm'

function row(over: Partial<LlmModelRow>): LlmModelRow {
  return {
    id: 'model',
    can_chat: true,
    thinks: false,
    is_remote: false,
    remote_host: null,
    size_bytes: null,
    suggested: null,
    ...over,
  }
}

/** Capabilities all null, as any non-Ollama endpoint reports them. */
const UNCHECKED = row({ can_chat: null, thinks: null, is_remote: null })

function result(over: Partial<LlmTestResult>): LlmTestResult {
  return { ok: true, content: '', saw_reasoning: false, detail: null, ...over }
}

describe('modelView', () => {
  /**
   * Protects the sharpest rule in this step. Eight of the thirteen models on
   * the verification machine run on someone else's servers, and `/v1/models`
   * gives no way to tell. The user's unreleased lyrics leaving the machine is
   * a disclosure they must see wherever the model is chosen.
   */
  it('discloses that a remote model sends lyrics off the machine', () => {
    const view = modelView(
      row({ id: 'kimi-k3:cloud', is_remote: true, remote_host: 'https://ollama.com' }),
    )
    expect(view.disclosure).toContain('https://ollama.com')
    expect(view.disclosure).toContain('leave this machine')
    expect(view.chips).toContain('remote')
  })

  /**
   * Protects: **unknown is not local.** Against a non-Ollama endpoint nothing
   * can be checked, and a silent absence of disclosure reads as "this is
   * private". The row says its capabilities are unknown instead.
   */
  it('never implies privacy for a model it could not check', () => {
    const view = modelView(UNCHECKED)
    expect(view.disclosure).toBeNull()
    expect(view.chips).toContain('capabilities unknown')
  })

  /**
   * Protects: **unknown is not unusable either.** Hiding every unchecked model
   * would strand a user on a non-Ollama endpoint with an empty picker.
   */
  it('keeps an unchecked model selectable', () => {
    expect(modelView(UNCHECKED).selectable).toBe(true)
  })

  /**
   * Protects: an embedding model cannot be chosen. `/v1/models` lists
   * `all-minilm` indistinguishably from a chat model, and picking it fails
   * later at lyric time, far from this screen.
   */
  it('refuses a model that cannot chat', () => {
    const view = modelView(row({ id: 'all-minilm:latest', can_chat: false }))
    expect(view.selectable).toBe(false)
    expect(view.chips).toContain('cannot chat')
  })

  /** Protects: the recommendation chip reaches the row. */
  it('chips a suggested model', () => {
    const view = modelView(
      row({ id: 'gemma4:12b-32k', suggested: { label: 'Gemma 4 12B', why: 'x', vram_hint: null } }),
    )
    expect(view.chips).toContain('recommended for lyrics')
  })

  /**
   * Protects: the thinking flag is shown, because it explains a pause the user
   * would otherwise read as a hang.
   */
  it('marks a model that thinks before answering', () => {
    expect(modelView(row({ thinks: true })).chips).toContain('thinks first')
    expect(modelView(row({ thinks: false })).chips).not.toContain('thinks first')
  })
})

describe('testSummary', () => {
  /**
   * Protects the trap that only a live call reveals. A thinking model spends
   * the token budget on chain-of-thought and returns empty content on a
   * perfectly healthy endpoint. Calling that a failure sends the user to fix a
   * setup that already works (LLM-SURFACE 11.4).
   */
  it('treats a reasoning-only answer as success', () => {
    const summary = testSummary(result({ ok: true, content: '', saw_reasoning: true }))
    expect(summary).toContain('works')
    expect(summary).not.toContain('did not answer')
  })

  /** Protects: a real answer is quoted back, so the user sees the endpoint work. */
  it('quotes the answer when there is one', () => {
    expect(testSummary(result({ content: 'ok' }))).toContain('"ok"')
    expect(testSummary(result({ content: 'ok', saw_reasoning: true }))).toContain('after thinking')
  })

  /** Protects: a genuine failure keeps its reason rather than becoming generic. */
  it('keeps the failure detail', () => {
    const summary = testSummary(result({ ok: false, detail: 'connection refused' }))
    expect(summary).toBe('connection refused')
  })
})

describe('canTest', () => {
  /** Protects: no test call without an endpoint and a chosen model. */
  it('requires a ready endpoint and a model', () => {
    const ready: LlmStatus = {
      state: 'ready',
      models: [],
      enriched: true,
      missing_suggestions: [],
      preselect: null,
      has_key: false,
    }
    expect(canTest(ready, 'gemma4:12b-32k')).toBe(true)
    expect(canTest(ready, null)).toBe(false)
    expect(canTest(ready, '')).toBe(false)
    expect(canTest({ state: 'not_configured' }, 'gemma4:12b-32k')).toBe(false)
    expect(canTest(null, 'gemma4:12b-32k')).toBe(false)
  })
})
