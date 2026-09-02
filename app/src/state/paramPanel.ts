import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import { getEnumChoices, getProfileInputs } from '../bridge/profiles'
import {
  MAX_SAFE_SEED,
  defaults,
  panelModel,
  withChoices,
  type ControlValue,
  type PanelModel,
} from './params'

/**
 * The param panel's state: which profile is loaded, what the user has changed,
 * and whether the advanced disclosure is open.
 *
 * Everything derived lives in `params.ts`. This holds only what a person has
 * actually done, which is the split the Phase 2 milestone paid for: a value
 * computed on the way to the screen is a value no test can reach.
 */

/**
 * A fresh seed, uniform across the whole range this app can carry exactly.
 *
 * 53 bits, assembled from two 32-bit draws: 21 high bits times 2^32 plus 32
 * low ones is exactly `Number.MAX_SAFE_INTEGER`. Anything wider would be a
 * seed the app cannot record honestly -- see `MAX_SAFE_SEED`.
 *
 * `crypto` rather than `Math.random` because this value is written into a
 * provenance sidecar as the identity of a track; a weak generator collides,
 * and two different tracks claiming one seed is a lie the sidecar cannot
 * survive.
 */
export function freshSeed(): number {
  const draw = crypto.getRandomValues(new Uint32Array(2))
  return (draw[0] % 2 ** 21) * 2 ** 32 + draw[1]
}

/**
 * The values a freshly loaded panel starts with.
 *
 * The profile's declared defaults, except the seed, which is rolled. A panel
 * that opened on seed 0 every time would make every first track of every
 * session the same one, and 0 is a real seed rather than a sentinel -- nothing
 * downstream could tell "never chosen" from "deliberately zero".
 *
 * `seed` is injectable so the rest of this is testable; nothing else in here
 * is non-deterministic.
 */
export function initialValues(
  model: PanelModel,
  seed: () => number = freshSeed,
): Record<string, ControlValue> {
  const values = defaults(model)
  for (const control of [...model.basic, ...model.advanced]) {
    if (control.kind === 'seed') values[control.name] = seed()
  }
  return values
}

interface ParamPanelState {
  /** The profile these values belong to; `null` before the first load. */
  profileId: string | null
  model: PanelModel | null
  values: Record<string, ControlValue>
  showAdvanced: boolean
  /**
   * Whether the user deliberately chose the seed -- typed it, or hit Reroll.
   *
   * The panel auto-rolls a seed on load, and a fresh Generate re-rolls it
   * unless this is true (T-316). A seed the app rolled is not a choice; a seed
   * the user set is, and re-rolling it would put a seed in the sidecar they
   * never saw.
   */
  seedPinned: boolean
  /** Why the panel is empty, when it is empty for a reason. */
  error: string | null
  busy: boolean
  load: (profileId: string) => Promise<void>
  /**
   * Load a profile and set the panel to a past track's values, seed **pinned**
   * (T-406 "re-use these settings"). Unlike `load`, it overwrites the values
   * even when the same profile is already selected, and never re-rolls the seed.
   */
  hydrate: (profileId: string, values: Record<string, ControlValue>) => Promise<void>
  setValue: (name: string, value: ControlValue) => void
  rerollSeed: () => void
  /** Set the seed value without pinning it -- Generate's own re-roll. */
  setSeed: (value: number) => void
  toggleAdvanced: () => void
  refreshChoices: () => Promise<void>
}

export const useParamPanelStore = create<ParamPanelState>((set, get) => ({
  profileId: null,
  model: null,
  values: {},
  showAdvanced: false,
  seedPinned: false,
  error: null,
  busy: false,

  /**
   * Load one profile's declarations and reset the panel to its defaults.
   *
   * Reloading the same profile is a no-op: switching views must not silently
   * discard the tags someone typed, and re-rolling their seed behind their
   * back would be worse -- the sidecar would record a seed they never saw.
   */
  load: async (profileId: string) => {
    if (!isTauri()) return
    if (get().profileId === profileId && get().model !== null) return

    set({ busy: true })
    try {
      const inputs = await getProfileInputs(profileId)
      if (inputs === null) {
        set({
          profileId,
          model: null,
          values: {},
          error: `No profile answers to ${profileId}, so there are no settings to show. Pick another model profile.`,
        })
        return
      }
      const model = panelModel(inputs)
      set({ profileId, model, values: initialValues(model), seedPinned: false, error: null })
    } finally {
      set({ busy: false })
    }
    await get().refreshChoices()
  },

  hydrate: async (profileId, values) => {
    if (!isTauri()) return

    set({ busy: true })
    try {
      const inputs = await getProfileInputs(profileId)
      if (inputs === null) {
        set({
          profileId,
          model: null,
          values: {},
          error: `No profile answers to ${profileId}, so there are no settings to show. Pick another model profile.`,
        })
        return
      }
      const model = panelModel(inputs)
      // Defaults for controls the spec never set, the spec's own values over
      // them; `seedPinned: true` so the next Generate reproduces this track
      // rather than re-rolling the seed the sidecar recorded (T-316's opposite).
      set({
        profileId,
        model,
        values: { ...initialValues(model), ...values },
        seedPinned: true,
        error: null,
      })
    } finally {
      set({ busy: false })
    }
    await get().refreshChoices()
  },

  /**
   * Ask the node registry for the live enum options, and fold them in.
   *
   * Separate from `load` and safe to call again, because ComfyUI is very often
   * started *after* the app -- an options list that can only be fetched once
   * leaves the user with three dead dropdowns and no way back. A failure here
   * leaves the panel exactly as it was: the controls already carry a sentence
   * saying the options are not loaded, which is more useful than replacing a
   * working panel with an error.
   */
  refreshChoices: async () => {
    const { profileId, model } = get()
    if (!isTauri() || profileId === null || model === null) return
    try {
      set({ model: withChoices(model, await getEnumChoices(profileId)) })
    } catch {
      // Left as it was, note intact.
    }
  },

  setValue: (name: string, value: ControlValue) => {
    const model = get().model
    const isSeed =
      model !== null &&
      [...model.basic, ...model.advanced].some((c) => c.kind === 'seed' && c.name === name)
    set({
      values: { ...get().values, [name]: value },
      seedPinned: isSeed ? true : get().seedPinned,
    })
  },

  /** Roll a new seed, leaving every other value alone. */
  rerollSeed: () => {
    const model = get().model
    if (model === null) return
    const seed = [...model.basic, ...model.advanced].find((c) => c.kind === 'seed')
    if (seed === undefined) return
    set({ values: { ...get().values, [seed.name]: freshSeed() }, seedPinned: true })
  },

  /** Set the seed value without pinning it -- Generate's own re-roll. */
  setSeed: (value: number) => {
    const model = get().model
    if (model === null) return
    const seed = [...model.basic, ...model.advanced].find((c) => c.kind === 'seed')
    if (seed === undefined) return
    set({ values: { ...get().values, [seed.name]: value } })
  },

  toggleAdvanced: () => set({ showAdvanced: !get().showAdvanced }),
}))

/** Re-exported so the view imports its ceiling from one place. */
export { MAX_SAFE_SEED }
