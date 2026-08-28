import { beforeEach, describe, expect, it, vi } from 'vitest'
import aceProfile from '../../../profiles/ace-step-1.5-turbo.json'
import type { ProfileInputs } from '../bridge/profiles'
import { MAX_SAFE_SEED, panelModel } from './params'
import { freshSeed, initialValues, useParamPanelStore } from './paramPanel'

const aceInputs = aceProfile.inputs as unknown as ProfileInputs

let mockIsTauri = true
let mockInputs: ProfileInputs | null = aceInputs

vi.mock('../bridge/comfy', () => ({ isTauri: () => mockIsTauri }))
vi.mock('../bridge/profiles', () => ({
  getProfileInputs: () => Promise.resolve(mockInputs),
}))

beforeEach(() => {
  mockIsTauri = true
  mockInputs = aceInputs
  useParamPanelStore.setState({
    profileId: null,
    model: null,
    values: {},
    showAdvanced: false,
    error: null,
    busy: false,
  })
})

describe('freshSeed', () => {
  /**
   * Protects: a rolled seed is always one this app can carry exactly.
   *
   * The whole point of `MAX_SAFE_SEED` is defeated if the panel's own
   * generator produces values above it -- the app would reject what a user
   * typed while quietly writing a rounded seed of its own into the sidecar.
   */
  it('test_a_rolled_seed_is_always_a_safe_integer', () => {
    for (let i = 0; i < 2000; i++) {
      const seed = freshSeed()
      expect(Number.isSafeInteger(seed)).toBe(true)
      expect(seed).toBeGreaterThanOrEqual(0)
      expect(seed).toBeLessThanOrEqual(MAX_SAFE_SEED)
    }
  })

  /** Protects: the generator actually varies -- a constant would pass above. */
  it('test_rolled_seeds_vary', () => {
    const seen = new Set(Array.from({ length: 200 }, () => freshSeed()))
    expect(seen.size).toBeGreaterThan(190)
  })

  /**
   * Protects: the range is used, not just its bottom.
   *
   * Assembling the seed from one 32-bit draw instead of the 21+32 split still
   * passes both tests above while confining every track this app ever makes to
   * the first 0.05% of the seed space.
   */
  it('test_rolled_seeds_reach_beyond_32_bits', () => {
    const draws = Array.from({ length: 200 }, () => freshSeed())
    expect(draws.some((s) => s > 2 ** 32)).toBe(true)
  })
})

describe('initialValues', () => {
  /**
   * Protects: a fresh panel does not open on seed 0.
   *
   * Every first track of every session would otherwise be the same one, and 0
   * is a real seed rather than a sentinel -- nothing downstream could tell
   * "never chosen" from "deliberately zero".
   */
  it('test_a_fresh_panel_rolls_its_seed', () => {
    const model = panelModel(aceInputs)
    const values = initialValues(model, () => 4242)

    expect(values.seed).toBe(4242)
  })

  /** Protects: rolling the seed does not disturb the profile's own defaults. */
  it('test_every_other_default_comes_from_the_profile', () => {
    const model = panelModel(aceInputs)
    const values = initialValues(model, () => 4242)

    expect(values.bpm).toBe(120)
    expect(values.duration_s).toBe(120)
    expect(values.steps).toBe(8)
    expect(values.tags).toBe(
      'synthwave, retro, 80s, dreamy, female vocal, driving beat, 105 bpm',
    )
  })
})

describe('useParamPanelStore', () => {
  it('test_load_builds_the_model_and_seeds_the_values', async () => {
    await useParamPanelStore.getState().load('ace-step-1.5-turbo')
    const state = useParamPanelStore.getState()

    expect(state.profileId).toBe('ace-step-1.5-turbo')
    expect(state.model?.basic.map((c) => c.name)[0]).toBe('tags')
    expect(state.error).toBeNull()
    expect(Number.isSafeInteger(state.values.seed)).toBe(true)
  })

  /**
   * Protects: an unknown profile says so instead of showing an empty panel.
   *
   * `effectiveProfileId` returns the configured id whether or not a profile
   * answers to it (state/profiles.ts), so this is reachable by deleting a user
   * profile from disk. A panel with no controls and no message reads as a
   * broken app.
   */
  it('test_an_unknown_profile_explains_the_empty_panel', async () => {
    mockInputs = null
    await useParamPanelStore.getState().load('gone')
    const state = useParamPanelStore.getState()

    expect(state.model).toBeNull()
    expect(state.error).toContain('gone')
    expect(state.error).toContain('Pick another')
  })

  /**
   * Protects: reloading the same profile does not throw away what was typed.
   *
   * A view re-mounts on every tab switch. Re-running defaults there would wipe
   * the user's tags -- and re-rolling their seed would put a seed in the
   * sidecar that they never saw.
   */
  it('test_reloading_the_same_profile_keeps_edits_and_the_seed', async () => {
    await useParamPanelStore.getState().load('ace-step-1.5-turbo')
    useParamPanelStore.getState().setValue('tags', 'synthwave')
    const seed = useParamPanelStore.getState().values.seed

    await useParamPanelStore.getState().load('ace-step-1.5-turbo')
    const state = useParamPanelStore.getState()

    expect(state.values.tags).toBe('synthwave')
    expect(state.values.seed).toBe(seed)
  })

  it('test_setvalue_changes_one_control_only', async () => {
    await useParamPanelStore.getState().load('ace-step-1.5-turbo')
    const before = useParamPanelStore.getState().values
    useParamPanelStore.getState().setValue('bpm', 90)
    const after = useParamPanelStore.getState().values

    expect(after.bpm).toBe(90)
    expect(after.seed).toBe(before.seed)
    expect(after.duration_s).toBe(before.duration_s)
  })

  /** Protects: re-rolling touches the seed and nothing else. */
  it('test_reroll_changes_the_seed_and_leaves_the_rest', async () => {
    await useParamPanelStore.getState().load('ace-step-1.5-turbo')
    useParamPanelStore.getState().setValue('bpm', 90)
    const before = useParamPanelStore.getState().values

    useParamPanelStore.getState().rerollSeed()
    const after = useParamPanelStore.getState().values

    expect(after.seed).not.toBe(before.seed)
    expect(after.bpm).toBe(90)
  })
})
