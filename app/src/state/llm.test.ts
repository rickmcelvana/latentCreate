import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { LlmModelRow, LlmStatus, LlmTestResult } from '../bridge/llm'
import type { Config } from '../bridge/config'
import {
  canTest,
  DEFAULT_BASE_URL,
  effectiveBaseUrl,
  keyField,
  modelView,
  testSummary,
  useLlmStore,
} from './llm'
import { useConfigStore } from './config'

const mockLlmProbe = vi.fn()
const mockLlmTest = vi.fn()
const mockSaveConfig = vi.fn()
const mockLoadConfig = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/comfy', () => ({
  isTauri: () => mockIsTauri,
}))

vi.mock('../bridge/llm', () => ({
  llmProbe: (baseUrl: string | null, configuredModel: string | null) =>
    mockLlmProbe(baseUrl, configuredModel),
  llmTest: (baseUrl: string, model: string) => mockLlmTest(baseUrl, model),
}))

vi.mock('../bridge/config', () => ({
  isTauri: () => mockIsTauri,
  loadConfig: () => mockLoadConfig(),
  saveConfig: (config: unknown) => mockSaveConfig(config),
  hasSecret: vi.fn(),
  setSecret: vi.fn(),
  deleteSecret: vi.fn(),
}))

function baseConfig(over: Partial<Config> = {}): Config {
  return {
    schema_version: 1,
    comfy: { mode: 'local', url: null, comfy_bin: null },
    llm: null,
    default_profile_id: null,
    ...over,
  }
}

function row(over: Partial<LlmModelRow>): LlmModelRow {
  return {
    id: 'model',
    can_chat: true,
    thinks: false,
    is_remote: false,
    remote_host: null,
    size_bytes: null,
    ...over,
  }
}

/** Capabilities all null, as any non-Ollama endpoint reports them. */
const UNCHECKED = row({ can_chat: null, thinks: null, is_remote: null })

function result(over: Partial<LlmTestResult>): LlmTestResult {
  return {
    ok: true,
    content: '',
    saw_reasoning: false,
    accepts_reasoning_effort: null,
    detail: null,
    ...over,
  }
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

describe('effectiveBaseUrl', () => {
  /**
   * Protects: the configured endpoint wins over the default. The default is
   * only a prefill, not a fallback that strands a user on Ollama.
   */
  it('returns the configured URL when one is set', () => {
    const config = baseConfig({
      llm: { provider: 'open_ai_compat', base_url: 'https://api.openai.com/v1', model: null, accepts_reasoning_effort: null },
    })
    expect(effectiveBaseUrl(config)).toBe('https://api.openai.com/v1')
  })

  /**
   * Protects: a null, empty or whitespace stored value is treated as unset,
   * so clearing the field does not leave the step probing nothing forever.
   */
  it('falls back to the default when no endpoint is stored', () => {
    expect(effectiveBaseUrl(baseConfig())).toBe(DEFAULT_BASE_URL)
    expect(
      effectiveBaseUrl(
        baseConfig({ llm: { provider: 'open_ai_compat', base_url: '', model: null, accepts_reasoning_effort: null } }),
      ),
    ).toBe(DEFAULT_BASE_URL)
    expect(
      effectiveBaseUrl(
        baseConfig({ llm: { provider: 'open_ai_compat', base_url: '   ', model: null, accepts_reasoning_effort: null } }),
      ),
    ).toBe(DEFAULT_BASE_URL)
  })
})

describe('llm store', () => {
  beforeEach(() => {
    mockIsTauri = true
    mockLlmProbe.mockReset()
    mockLlmTest.mockReset()
    mockSaveConfig.mockReset()
    mockLoadConfig.mockReset()
    useLlmStore.setState({ status: null, busy: false, testing: false, result: null, model: null })
    useConfigStore.setState({ config: baseConfig(), warnings: [], status: 'ready', error: null })
  })

  /**
   * The regression test for the bug the T-211 click-through found: the wizard
   * let a model be picked and tested, and the Lyrics Studio then reported "no
   * lyric LLM configured" because nothing had ever written `config.json`.
   *
   * The test call could not have caught it -- `llm_test` takes the endpoint and
   * model as arguments, so it passes against a config that does not exist. The
   * thing to assert is the write.
   */
  it('test_choose_persists_the_endpoint_and_model', async () => {
    await useLlmStore.getState().choose('http://127.0.0.1:11434/v1', 'gemma4:12b-32k')

    expect(useLlmStore.getState().model).toBe('gemma4:12b-32k')
    expect(mockSaveConfig).toHaveBeenCalledTimes(1)
    expect(mockSaveConfig.mock.calls[0]![0]).toMatchObject({
      llm: {
        provider: 'open_ai_compat',
        base_url: 'http://127.0.0.1:11434/v1',
        model: 'gemma4:12b-32k',
      },
    })
  })

  /**
   * Protects: what generation reads is what the picker wrote. `configured_llm`
   * needs all three of provider, base URL and model, and returns a different
   * error for each -- a saved block missing one is the same failure wearing a
   * different message.
   */
  it('test_choose_writes_every_field_generation_requires', async () => {
    await useLlmStore.getState().choose('http://127.0.0.1:11434/v1', 'gemma4:12b-32k')

    const saved = mockSaveConfig.mock.calls[0]![0] as Config
    expect(saved.llm?.provider).not.toBeNull()
    expect(saved.llm?.base_url).not.toBeNull()
    expect(saved.llm?.model).not.toBeNull()
    // The rest of the config survives the patch.
    expect(saved.comfy).toEqual(baseConfig().comfy)
    expect(saved.schema_version).toBe(1)
  })

  /** Protects: choosing still selects in a plain browser, where nothing persists. */
  it('test_choose_selects_without_saving_outside_tauri', async () => {
    mockIsTauri = false
    await useLlmStore.getState().choose('http://127.0.0.1:11434/v1', 'gemma4:12b-32k')

    expect(useLlmStore.getState().model).toBe('gemma4:12b-32k')
    expect(mockSaveConfig).not.toHaveBeenCalled()
  })

  /**
   * Protects: the probe is told what is already configured. The backend's
   * preselect exists so a configured model wins, full stop, and passing null
   * makes that rule unreachable -- the second half of the same bug, which would
   * have shown as the wizard forgetting the choice on reopen.
   */
  it('test_probe_forwards_the_configured_model', async () => {
    mockLlmProbe.mockResolvedValue({ state: 'not_configured' })
    await useLlmStore.getState().probe('http://127.0.0.1:11434/v1', 'gemma4:12b-32k')

    expect(mockLlmProbe).toHaveBeenCalledWith('http://127.0.0.1:11434/v1', 'gemma4:12b-32k')
  })

  /**
   * Protects: changing the endpoint must not drop the model the user already
   * chose. `useConfigStore.save` does a shallow merge, so a partial `llm`
   * patch would replace the whole block and lose the model (T-212).
   */
  it('test_save_endpoint_preserves_the_configured_model', async () => {
    useConfigStore.setState({
      config: baseConfig({
        llm: { provider: 'open_ai_compat', base_url: 'http://old', model: 'gemma4:12b-32k', accepts_reasoning_effort: null },
      }),
    })

    await useLlmStore.getState().saveEndpoint('http://new')

    const saved = mockSaveConfig.mock.calls[0]![0] as Config
    expect(saved.llm).toEqual({
      provider: 'open_ai_compat',
      base_url: 'http://new',
      model: 'gemma4:12b-32k',
      accepts_reasoning_effort: null,
    })
  })

  /**
   * Protects: the verdict is a fact about one endpoint. Carrying it to a
   * different endpoint would present a stale verified-fact as true, which is
   * worse than unknown.
   */
  it('test_save_endpoint_clears_the_reasoning_effort_verdict', async () => {
    useConfigStore.setState({
      config: baseConfig({
        llm: {
          provider: 'open_ai_compat',
          base_url: 'http://old',
          model: 'gemma4:12b-32k',
          accepts_reasoning_effort: true,
        },
      }),
    })

    await useLlmStore.getState().saveEndpoint('http://new')

    const saved = mockSaveConfig.mock.calls[0]![0] as Config
    expect(saved.llm).toEqual({
      provider: 'open_ai_compat',
      base_url: 'http://new',
      model: 'gemma4:12b-32k',
      accepts_reasoning_effort: null,
    })
  })

  /**
   * Protects: the test call's verdict is persisted so generation uses it.
   * Without this write, the backend would fall back to the `thinks` rule and
   * miss endpoints that accept the field but are not Ollama.
   */
  it('test_test_persists_the_reasoning_effort_verdict', async () => {
    useLlmStore.setState({ model: 'gemma4:12b-32k' })
    mockLlmTest.mockResolvedValue(result({ accepts_reasoning_effort: true }))

    await useLlmStore.getState().test('http://127.0.0.1:11434/v1')

    expect(mockLlmTest).toHaveBeenCalledWith('http://127.0.0.1:11434/v1', 'gemma4:12b-32k')
    expect(mockSaveConfig).toHaveBeenCalledTimes(1)
    expect(mockSaveConfig.mock.calls[0]![0]).toMatchObject({
      llm: {
        provider: 'open_ai_compat',
        base_url: 'http://127.0.0.1:11434/v1',
        model: 'gemma4:12b-32k',
        accepts_reasoning_effort: true,
      },
    })
  })
})

describe('keyField', () => {
  /**
   * Protects: a stored key hides the input. The write-only rule itself is
   * guaranteed by construction -- no command returns a secret value (T-004) --
   * so what a test can reach is the branch, and this is it.
   */
  it('shows the stored affordance only when the endpoint reports a key', () => {
    const ready = (has_key: boolean): LlmStatus => ({
      state: 'ready',
      models: [],
      enriched: true,
      preselect: null,
      has_key,
    })
    expect(keyField(ready(true))).toBe('stored')
    expect(keyField(ready(false))).toBe('entry')
  })

  /**
   * Protects: before the endpoint has answered, the app must not claim a key
   * is stored. `has_key` is only known on `ready`.
   */
  it('offers the input whenever no endpoint has answered', () => {
    expect(keyField(null)).toBe('entry')
    expect(keyField({ state: 'not_configured' })).toBe('entry')
    expect(keyField({ state: 'unreachable', detail: 'refused', hint: null })).toBe('entry')
  })
})
