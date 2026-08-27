import { beforeEach, describe, expect, it, vi } from 'vitest'
import wireFile from '../../../testdata/wire/loaded-config.json'
import type { LoadedConfig } from '../bridge/config'
import { useConfigStore } from './config'

/**
 * The shared wire fixture, re-declared with literal types.
 *
 * TypeScript widens JSON imports to `string`, so `wireFile` can never satisfy the union
 * types (`ComfyMode`, `LlmProvider`) on its own -- the import alone proves nothing about
 * the TypeScript types. Declaring it here restores the compile-time check, and
 * `test_typed_fixture_matches_shared_wire_file` asserts the two are identical, so editing
 * one without the other fails. The JSON file therefore stays the single source of truth
 * shared with Rust's `test_wire_fixture_matches_current_types`.
 */
const fixture: LoadedConfig = {
  config: {
    schema_version: 1,
    comfy: { mode: 'cloud', url: 'http://127.0.0.1:8188', comfy_bin: null },
    llm: {
      provider: 'open_ai_compat',
      base_url: 'http://localhost:11434/v1',
      model: 'gemma4:12b',
      accepts_reasoning_effort: null,
    },
    default_profile_id: 'ace-step-1.5-turbo',
  },
  warnings: [
    { kind: 'corrupt', backup: 'C:/x/config.json.corrupt-1', detail: 'expected value' },
  ],
}

const mockLoadConfig = vi.fn()
const mockSaveConfig = vi.fn()
const mockHasSecret = vi.fn()
const mockSetSecret = vi.fn()
const mockDeleteSecret = vi.fn()
let mockIsTauri = true

vi.mock('../bridge/config', () => ({
  isTauri: () => mockIsTauri,
  loadConfig: () => mockLoadConfig(),
  saveConfig: (config: unknown) => mockSaveConfig(config),
  hasSecret: (name: string) => mockHasSecret(name),
  setSecret: (name: string, value: string) => mockSetSecret(name, value),
  deleteSecret: (name: string) => mockDeleteSecret(name),
}))

beforeEach(() => {
  mockIsTauri = true
  mockLoadConfig.mockReset()
  mockSaveConfig.mockReset()
  mockHasSecret.mockReset()
  mockSetSecret.mockReset()
  mockDeleteSecret.mockReset()
  useConfigStore.setState({
    config: null,
    warnings: [],
    status: 'idle',
    error: null,
    secrets: {},
  })
})

describe('config store', () => {
  it('test_typed_fixture_matches_shared_wire_file', () => {
    // Pins these TypeScript types to the exact bytes Rust round-trips. If Rust's wire
    // format changes, its own test fails; if the fixture is edited to match, this fails
    // until the typed declaration above is updated too.
    expect(fixture).toEqual(wireFile)
  })

  it('test_load_populates_config_and_warnings', async () => {
    mockLoadConfig.mockResolvedValue(fixture)
    await useConfigStore.getState().load()
    const state = useConfigStore.getState()
    expect(state.config?.llm?.provider).toBe('open_ai_compat')
    expect(state.config?.comfy.mode).toBe('cloud')
    expect(state.warnings).toHaveLength(1)
    expect(state.warnings[0]).toMatchObject({
      kind: 'corrupt',
      backup: expect.any(String),
      detail: expect.any(String),
    })
  })

  it('test_load_sets_unavailable_outside_tauri', async () => {
    mockIsTauri = false
    await useConfigStore.getState().load()
    const state = useConfigStore.getState()
    expect(state.status).toBe('unavailable')
    expect(state.error).toBeNull()
    expect(mockLoadConfig).not.toHaveBeenCalled()
  })

  it('test_load_sets_error_when_command_rejects', async () => {
    mockLoadConfig.mockRejectedValue(new Error('command failed'))
    await useConfigStore.getState().load()
    const state = useConfigStore.getState()
    expect(state.status).toBe('error')
    expect(state.error).toBe('Error: command failed')
  })

  it('test_save_merges_patch_over_current_config', async () => {
    const base = { ...fixture.config }
    useConfigStore.setState({ config: base, status: 'ready' })
    await useConfigStore.getState().save({ default_profile_id: 'new-profile' })
    expect(mockSaveConfig).toHaveBeenCalledTimes(1)
    const saved = mockSaveConfig.mock.calls[0][0]
    expect(saved.default_profile_id).toBe('new-profile')
    expect(saved.schema_version).toBe(base.schema_version)
    expect(saved.comfy).toEqual(base.comfy)
    expect(saved.llm).toEqual(base.llm)
  })

  it('test_save_before_load_does_not_call_backend', async () => {
    await useConfigStore.getState().save({ default_profile_id: 'x' })
    expect(mockSaveConfig).not.toHaveBeenCalled()
    expect(useConfigStore.getState().status).toBe('error')
    expect(useConfigStore.getState().error).toContain('cannot save before config is loaded')
  })

  it('test_refresh_secrets_records_presence_per_name', async () => {
    mockHasSecret.mockImplementation((name: string) => name === 'llm_api_key')
    await useConfigStore.getState().refreshSecrets(['comfy_cloud_api_key', 'llm_api_key'])
    const state = useConfigStore.getState()
    expect(state.secrets).toEqual({
      comfy_cloud_api_key: false,
      llm_api_key: true,
    })
  })

  it('test_refresh_secrets_is_not_called_outside_tauri', async () => {
    mockIsTauri = false
    await useConfigStore.getState().refreshSecrets(['comfy_cloud_api_key'])
    expect(mockHasSecret).not.toHaveBeenCalled()
  })
})
