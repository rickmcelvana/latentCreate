import { describe, expect, it } from 'vitest'
import aceProfile from '../../../profiles/ace-step-1.5-turbo.json'
import type { ProfileInputs } from '../bridge/profiles'
import {
  MAX_SAFE_SEED,
  defaults,
  groupsOf,
  panelModel,
  seedError,
  specInputs,
  type Control,
} from './params'

/**
 * The shipped ACE-Step profile's real declarations.
 *
 * TypeScript widens a JSON import's `type` fields to `string`, so the file
 * cannot satisfy the discriminated union on its own (the same problem
 * `config.test.ts` solved by re-declaring its fixture). Re-declaring seventeen
 * inputs by hand would be worse than the cast: it would be a second copy of the
 * profile that drifts. So the cast stands, and
 * `test_every_declared_input_is_accounted_for` is what keeps it honest -- it
 * walks the real file's keys and fails if any of them vanished on the way
 * through, which is exactly what a wrong cast would cause.
 */
const aceInputs = aceProfile.inputs as unknown as ProfileInputs

function named(controls: Control[], name: string): Control {
  const found = controls.find((c) => c.name === name)
  expect(found, `no control named ${name}`).toBeDefined()
  return found as Control
}

describe('panelModel', () => {
  /**
   * Protects: nothing the profile declares is silently lost. Every input is
   * either a control or an explicitly recorded omission.
   *
   * Also the guard on the JSON cast above: a cast that lied about the shape
   * would drop inputs here rather than fail to compile.
   */
  it('test_every_declared_input_is_accounted_for', () => {
    const model = panelModel(aceInputs)
    const seen = new Set([
      ...model.basic.map((c) => c.name),
      ...model.advanced.map((c) => c.name),
      ...model.omitted.map((o) => o.name),
    ])

    for (const declared of Object.keys(aceInputs)) {
      // `planner` is a group: it contributes its members, not itself.
      if (declared === 'planner') continue
      expect(seen, `${declared} reached neither a control nor an omission`).toContain(declared)
    }
    expect(model.basic.length + model.advanced.length).toBe(15)
  })

  /**
   * Protects: "this model has no negative prompt" is reported as a verified
   * fact, not as an absence.
   *
   * A missing negative-prompt box and a forgotten negative-prompt box look
   * identical on screen. The profile records that someone read
   * `TextEncodeAceStepAudio1.5`'s real schema; dropping the reason throws that
   * evidence away.
   */
  it('test_an_unsupported_input_is_recorded_with_its_reason', () => {
    const model = panelModel(aceInputs)

    expect(model.basic.map((c) => c.name)).not.toContain('negative')
    expect(model.advanced.map((c) => c.name)).not.toContain('negative')

    const omitted = model.omitted.find((o) => o.name === 'negative')
    expect(omitted).toBeDefined()
    expect(omitted?.reason).toContain('no negative')
  })

  /**
   * Protects: a member inherits its group's advanced flag.
   *
   * ACE-Step's `planner` group is `advanced: true` and all five of its members
   * declare `advanced: false`. Reading only the member's own flag puts five
   * LM-planner sampling controls in the basic panel -- cfg_scale, temperature,
   * top_p, top_k, min_p in front of someone who wanted to type style tags --
   * while the group that was supposed to hide them is itself hidden.
   */
  it('test_group_members_inherit_the_group_disclosure', () => {
    const model = panelModel(aceInputs)
    const planner = model.advanced.filter((c) => c.group !== null)

    expect(planner.map((c) => c.name).toSorted()).toEqual([
      'cfg_scale',
      'min_p',
      'temperature',
      'top_k',
      'top_p',
    ])
    expect(planner.every((c) => c.advanced)).toBe(true)
    expect(model.basic.filter((c) => c.group !== null)).toEqual([])
  })

  /**
   * Protects: the basic panel is what a musician reaches for, in that order.
   *
   * The profile's `inputs` is a `BTreeMap`, so it arrives alphabetically and
   * `bpm` comes before `tags` and `lyrics`. Rendering the map's own order is
   * the thing that looks like nobody thought about it.
   */
  it('test_the_basic_controls_are_ordered_for_a_musician_not_alphabetically', () => {
    const model = panelModel(aceInputs)

    expect(model.basic.map((c) => c.name)).toEqual([
      'tags',
      'lyrics',
      'duration_s',
      'bpm',
      'keyscale',
      'timesignature',
      'language',
      'seed',
    ])
  })

  /**
   * Protects: an unrecognised input still renders, after the known ones.
   *
   * A custom-imported workflow (ARCHITECTURE 5b) names its inputs whatever its
   * author named them. Ordering must not be a whitelist that hides them.
   */
  it('test_unknown_input_names_sort_after_the_known_ones', () => {
    const model = panelModel({
      ...aceInputs,
      zebra: { type: 'text', slots: ['1.x'], advanced: false },
      aardvark: { type: 'text', slots: ['1.y'], advanced: false },
    })

    const names = model.basic.map((c) => c.name)
    expect(names.slice(-2)).toEqual(['aardvark', 'zebra'])
    expect(names[0]).toBe('tags')
  })

  /**
   * Protects: an enum whose options live in the node schema is marked as such
   * and left empty -- not rendered as an empty dropdown indistinguishable from
   * a model that offers no choices.
   *
   * ACE-Step declares key/scale, time signature and language this way, with no
   * local list at all: 34 and 51 values that would rot on the first ComfyUI
   * update (MCP-SURFACE 11). Until something asks the node registry there is
   * nothing to show, and the panel has to say which of the two situations it
   * is in.
   */
  it('test_node_backed_enums_are_flagged_rather_than_faked', () => {
    const model = panelModel(aceInputs)

    for (const name of ['keyscale', 'timesignature', 'language']) {
      const control = named(model.basic, name)
      expect(control.kind).toBe('enum')
      expect(control.fromNode).toBe(true)
      expect(control.choices).toEqual([])
    }
  })

  /**
   * Protects: bounds and defaults come from the profile, never from constants.
   */
  it('test_bounds_and_defaults_come_from_the_profile', () => {
    const model = panelModel(aceInputs)

    expect(named(model.basic, 'bpm').range).toEqual({ min: 10, max: 300, step: 1 })
    expect(named(model.basic, 'bpm').default).toBe(120)
    expect(named(model.basic, 'duration_s').range?.max).toBe(300)
    expect(named(model.basic, 'duration_s').default).toBe(120)
    expect(named(model.advanced, 'steps').default).toBe(8)
    expect(named(model.advanced, 'shift').range).toEqual({ min: 0, max: 10, step: 0.1 })
  })

  it('test_defaults_covers_every_control_and_nothing_else', () => {
    const model = panelModel(aceInputs)
    const values = defaults(model)

    expect(Object.keys(values).toSorted()).toEqual(
      [...model.basic, ...model.advanced].map((c) => c.name).toSorted(),
    )
    expect(values.bpm).toBe(120)
    expect(values.tags).toBe('')
  })
})

describe('groupsOf', () => {
  /**
   * Protects: the advanced disclosure renders one fieldset per group, in the
   * order the groups appear, with no group listed twice.
   *
   * This started life inside `<ParamPanel>`, where nothing could reach it --
   * vitest runs in `node` with no DOM, so a decision about what is rendered
   * and in what order was a decision the gate could not see.
   */
  it('test_groups_are_listed_once_in_the_order_they_appear', () => {
    const model = panelModel(aceInputs)

    expect(groupsOf(model.advanced)).toEqual(['Planner sampling'])
    expect(groupsOf(model.basic)).toEqual([])
  })

  /**
   * Protects: appearance order, not alphabetical order.
   *
   * The shipped profile has exactly one group, so on the real fixture every
   * ordering rule here is satisfied by accident -- sort the groups
   * alphabetically and nothing notices. Same trap the LoRA catalog hit in
   * T-307, so the same answer: the real declarations plus one more group,
   * labelled so that the two orders disagree.
   */
  it('test_groups_keep_appearance_order_not_alphabetical_order', () => {
    const model = panelModel({
      ...aceInputs,
      zzz_extra: {
        type: 'group',
        advanced: true,
        label: 'Aaa later group',
        members: { extra_knob: { type: 'int', slots: ['1.k'], min: 0, max: 1, default: 0, advanced: false } },
      },
    })

    expect(groupsOf(model.advanced)).toEqual(['Planner sampling', 'Aaa later group'])
  })

  it('test_a_control_with_no_group_contributes_none', () => {
    expect(groupsOf([])).toEqual([])
  })
})

describe('seedError', () => {
  /**
   * Protects: a seed too large for JavaScript is refused, not rounded.
   *
   * ACE-Step's seed runs to `u64::MAX` and `create-core` carries it as a `u64`
   * on purpose -- `InputValue::Seed` exists so a seed cannot be demoted, and
   * its tests pin `Seed(u64::MAX)`. JavaScript cannot hold that: above 2^53-1
   * the value changes on the way through, `invoke` serialises via JSON so a
   * BigInt cannot cross either, and 18446744073709551615 arrives in Rust as
   * 18446744073709551616. That is accepted, generated with, and written into
   * the provenance sidecar -- the exact silent corruption the Rust type was
   * introduced to prevent, reappearing one layer above it. Refusing is visible;
   * rounding is a sidecar that lies.
   */
  it('test_a_seed_beyond_javascripts_reach_is_refused_not_rounded', () => {
    expect(seedError('18446744073709551615')).not.toBeNull()
    expect(seedError(String(MAX_SAFE_SEED))).toBeNull()
    expect(seedError(String(MAX_SAFE_SEED + 2))).not.toBeNull()
  })

  it('test_a_seed_must_be_digits', () => {
    expect(seedError('42')).toBeNull()
    expect(seedError('0')).toBeNull()
    expect(seedError('-1')).not.toBeNull()
    expect(seedError('1.5')).not.toBeNull()
    expect(seedError('abc')).not.toBeNull()
    expect(seedError('  ')).not.toBeNull()
  })
})

describe('specInputs', () => {
  /**
   * Protects: the tag comes from the control's declared kind, not from what
   * the value looks like at runtime.
   *
   * `InputValue` is adjacently tagged precisely because an untagged JSON `3`
   * deserialises as `Int`, `Float` or `Seed`, and a seed demoted to an `Int`
   * makes a track unreproducible (generation.rs). Typing off `typeof value`
   * would send `{"type":"int"}` for a seed of 8 and `{"type":"float"}` for a
   * bpm of 120.5 -- handing Rust the guess its own encoding refuses to make.
   */
  it('test_values_are_tagged_by_declared_kind_not_by_runtime_shape', () => {
    const model = panelModel(aceInputs)
    const inputs = specInputs(model, {
      ...defaults(model),
      tags: 'synthwave, driving',
      seed: 8,
      bpm: 8,
      duration_s: 8,
      keyscale: 'C major',
    })

    // Four inputs, one value, four different tags.
    expect(inputs.seed).toEqual({ type: 'seed', value: 8 })
    expect(inputs.bpm).toEqual({ type: 'int', value: 8 })
    expect(inputs.duration_s).toEqual({ type: 'float', value: 8 })
    expect(inputs.keyscale).toEqual({ type: 'enum', value: 'C major' })
    expect(inputs.tags).toEqual({ type: 'text', value: 'synthwave, driving' })
  })

  /**
   * Protects: an empty control is left out rather than sent as a default.
   *
   * `resolve_slots` rejects an input the profile does not declare and applies
   * every input it is given, so sending a value nobody chose is how a form
   * quietly overrides the workflow's own setting.
   */
  it('test_untouched_controls_are_not_sent', () => {
    const model = panelModel(aceInputs)
    const inputs = specInputs(model, { ...defaults(model), bpm: 90 })

    expect(inputs.bpm).toEqual({ type: 'int', value: 90 })
    expect(inputs.tags).toBeUndefined()
    expect(inputs.keyscale).toBeUndefined()
    expect(inputs.lyrics).toBeUndefined()
  })

  /** Protects: an input the panel never rendered cannot reach the spec. */
  it('test_an_unsupported_input_never_reaches_the_spec', () => {
    const model = panelModel(aceInputs)
    const inputs = specInputs(model, { ...defaults(model), negative: 'muddy, clipping' })

    expect(inputs.negative).toBeUndefined()
  })
})
