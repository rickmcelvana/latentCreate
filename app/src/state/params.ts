import type { InputSpec, ProfileInputs } from '../bridge/profiles'

/**
 * The param panel's model: a profile's declared inputs, turned into an ordered
 * list of controls with defaults, bounds and a basic/advanced split.
 *
 * Pure functions rather than JSX. Every defect the Phase 2 milestone found that
 * `tsc`, `oxlint` and 109 tests could not see was correct logic derived inline
 * in a view (PROJECT.md, 2026-08-26), and this panel derives more than anything
 * in Phase 2: what to show at all, in what order, behind which disclosure, with
 * which bounds, and how a typed value reaches Rust. vitest runs in `node` with
 * no DOM here, so a rule that lives in JSX is a rule no test can reach.
 */

/**
 * The largest seed this app will accept from a person.
 *
 * ACE-Step's seed input runs to `u64::MAX`, and `create-core` carries it as a
 * `u64` on purpose -- `InputValue::Seed` exists precisely so a seed cannot be
 * demoted to some other number type, and its tests pin `Seed(u64::MAX)`.
 *
 * JavaScript has no such integer. Anything above 2^53-1 loses precision the
 * moment it becomes a `number`, and `invoke` serialises through JSON, so a
 * `BigInt` cannot cross the bridge either. A seed typed as 18446744073709551615
 * would arrive in Rust as 18446744073709551616 -- accepted, recorded in the
 * provenance sidecar, and not the seed the user asked for.
 *
 * So the panel **refuses** seeds above this, rather than rounding them. A
 * refused seed is on screen; a rounded one is a sidecar that lies. This does
 * cap the app below the model's range, which is a real limitation and belongs
 * in the UI copy, not in a comment alone.
 */
export const MAX_SAFE_SEED = Number.MAX_SAFE_INTEGER

/**
 * Semantic input names in the order a musician wants them, most-used first.
 *
 * The profile's `inputs` is a `BTreeMap`, so it arrives alphabetically --
 * `bpm, duration_s, keyscale, language, lyrics, negative, planner, seed,
 * shift, steps, tags, timesignature`. Rendering that order puts bpm above the
 * style tags and buries lyrics in the middle.
 *
 * A constant here rather than a new field on the profile schema: ordering is a
 * property of this panel, not of the model, and a `display_order` in every
 * profile would be one more thing a user profile (ARCHITECTURE 5b) has to get
 * right to look correct. Names this list does not know are appended
 * alphabetically, which is what a custom-imported workflow gets.
 */
const PRESENTATION_ORDER = [
  'tags',
  'lyrics',
  'negative',
  'duration_s',
  'bpm',
  'keyscale',
  'timesignature',
  'language',
  'seed',
  'steps',
  'shift',
]

/** Numeric bounds, as the control renders them. */
export interface Range {
  min: number
  max: number
  /** `null` when the profile declares none -- the view picks its own. */
  step: number | null
}

/** What one control holds. Enum and text values are strings. */
export type ControlValue = string | number

/** What every control carries, whatever its kind. */
interface ControlBase {
  /**
   * The key this becomes in `GenerationSpec.inputs`.
   *
   * **Group members are dotted** -- `planner.cfg_scale`, not `cfg_scale` --
   * because that is what `ModelProfile::flat_inputs` calls them, and a spec
   * built with the bare name is rejected by `resolve_slots` with
   * "<profile> has no input named cfg_scale". Two groups could also each
   * declare a `seed`, and bare names would silently collide.
   *
   * This was wrong for four tasks and no test could see it: the frontend
   * flattened one way, Rust the other, and each side's tests only ever looked
   * at its own. `testdata/profiles/ace-step-flat-inputs.json` now pins the list
   * from both directions.
   */
  name: string
  /** What the user reads: the profile's label when it has one, else the name. */
  label: string
  /** Label of the group this belongs to, or `null` at top level. */
  group: string | null
  /** Behind the advanced disclosure. Inherited from an advanced group. */
  advanced: boolean
  /** The profile's declared default, or a neutral value for kinds without one. */
  default: ControlValue
}

/**
 * One control the panel renders.
 *
 * A union rather than one interface with nullable fields, so that "a numeric
 * control always has bounds" is a fact the compiler knows instead of one the
 * caller has to take on trust. It was written the loose way first, and the
 * component that consumed it could not compile `min={control.range.min}`
 * without a null check for a state that cannot happen -- a dead branch that
 * reads as if the bounds were optional. Narrowing on `kind` now gives the view
 * exactly the fields that kind has.
 */
export type Control =
  | (ControlBase & {
      kind: 'text' | 'lyrics'
      range: null
      choices: string[]
      fromNode: false
    })
  /** Numeric kinds always carry bounds; `seed`'s are 0..[`MAX_SAFE_SEED`]. */
  | (ControlBase & {
      kind: 'int' | 'float' | 'seed'
      range: Range
      choices: string[]
      fromNode: false
    })
  | (ControlBase & {
      kind: 'enum'
      range: null
      choices: string[]
      /**
       * This enum's options come from the live node schema, not the profile.
       *
       * ACE-Step's key/scale, time signature and language are all declared
       * `from_node_choices` with **no** local list -- 34 and 51 values that
       * would rot the first time ComfyUI updates (MCP-SURFACE 11). So
       * `choices` is empty until [`withChoices`] fills it, and a control in
       * this state is not the same as one the model does not support.
       */
      fromNode: boolean
      /**
       * Why this list is empty, or why it should not be fully trusted.
       *
       * `null` only when the options are known good. Every other state has a
       * sentence, and the sentence lives here rather than in the view because
       * vitest runs in `node` with no DOM: wording put in JSX is wording no
       * test can read.
       */
      optionsNote: string | null
    })

/**
 * An input the profile declares but the panel deliberately does not render.
 *
 * Reported rather than filtered away. `Unsupported` is a **verified** fact --
 * "TextEncodeAceStepAudio1.5 exposes no negative input", checked against a
 * live node schema, recorded so that "we looked" is distinguishable from
 * "nobody thought about it" (profile.rs). Dropping it silently throws away the
 * only evidence that anyone checked, and leaves a missing negative-prompt box
 * looking exactly like a bug.
 */
export interface Omitted {
  name: string
  reason: string | null
}

/** Everything the panel needs from one profile's declarations. */
export interface PanelModel {
  basic: Control[]
  advanced: Control[]
  omitted: Omitted[]
}

/**
 * Turn a profile's declared inputs into the panel's model.
 *
 * Groups are flattened: their members become controls tagged with the group's
 * label, so the view renders one fieldset without walking a tree. **A member
 * inherits its group's `advanced` flag** -- ACE-Step's planner group is
 * `advanced: true` while all five of its members declare `advanced: false`,
 * and honouring only the member's own flag puts five sampler controls in the
 * basic panel while the group holding them is hidden.
 */
export function panelModel(inputs: ProfileInputs): PanelModel {
  const controls: Control[] = []
  const omitted: Omitted[] = []
  collect(inputs, null, null, false, controls, omitted)

  return {
    basic: ordered(controls.filter((c) => !c.advanced)),
    advanced: ordered(controls.filter((c) => c.advanced)),
    omitted: omitted.toSorted((a, b) => a.name.localeCompare(b.name)),
  }
}

function collect(
  inputs: ProfileInputs,
  group: string | null,
  /** The group key path a member sits under, or `null` at top level. */
  prefix: string | null,
  inheritedAdvanced: boolean,
  controls: Control[],
  omitted: Omitted[],
): void {
  for (const [name, spec] of Object.entries(inputs)) {
    const advanced = inheritedAdvanced || advancedOf(spec)
    const qualified = prefix === null ? name : `${prefix}.${name}`

    if (spec.type === 'unsupported') {
      omitted.push({ name: qualified, reason: spec.reason ?? null })
      continue
    }
    if (spec.type === 'group') {
      // The **key** carries the qualifier; the **label** is what the fieldset
      // shows. They are different strings -- ACE-Step's `planner` group is
      // labelled "Planner sampling" -- and using the label to qualify would
      // produce names Rust has never heard of.
      collect(spec.members, spec.label ?? name, qualified, advanced, controls, omitted)
      continue
    }
    controls.push(control(qualified, name, spec, group, advanced))
  }
}

function control(
  /** The name a `GenerationSpec` uses: dotted for a group member. */
  name: string,
  /** The member's own key, which is what a label falls back to. */
  bare: string,
  spec: Exclude<InputSpec, { type: 'unsupported' } | { type: 'group' }>,
  group: string | null,
  advanced: boolean,
): Control {
  const base: ControlBase = {
    name,
    label: 'label' in spec ? (spec.label ?? bare) : bare,
    group,
    advanced,
    default: '',
  }
  const plain = { range: null, choices: [] as string[], fromNode: false } as const
  const numeric = { choices: [] as string[], fromNode: false } as const

  switch (spec.type) {
    case 'text':
    case 'lyrics':
      // The profile's own starting text, when it declares one. ACE-Step
      // prefills its style tags: the field is a format most people have not
      // met ("comma-separated short tags"), and an example on screen teaches
      // it in a way a placeholder cannot. Lyrics declare none -- prefilled
      // lyrics would be words the app put in the user's mouth.
      return { ...base, ...plain, kind: spec.type, default: spec.default ?? '' }
    case 'int':
      return {
        ...base,
        ...numeric,
        kind: 'int',
        range: { min: spec.min, max: spec.max, step: 1 },
        default: spec.default,
      }
    case 'float':
      return {
        ...base,
        ...numeric,
        kind: 'float',
        range: { min: spec.min, max: spec.max, step: spec.step ?? null },
        default: spec.default,
      }
    case 'seed':
      // Not a number control with a huge max: see MAX_SAFE_SEED. The range is
      // stated so the view can show it, and `seedError` is what enforces it.
      return {
        ...base,
        ...numeric,
        kind: 'seed',
        range: { min: 0, max: MAX_SAFE_SEED, step: 1 },
        default: 0,
      }
    case 'enum':
      return {
        ...base,
        range: null,
        kind: 'enum',
        choices: spec.from_node_choices ? [] : spec.choices,
        fromNode: spec.from_node_choices,
        optionsNote: spec.from_node_choices ? NOT_LOADED : null,
        default: spec.from_node_choices ? '' : (spec.choices[0] ?? ''),
      }
  }
}

function advancedOf(spec: InputSpec): boolean {
  return 'advanced' in spec ? spec.advanced : false
}

/** Known names in presentation order, then everything else alphabetically. */
function ordered(controls: Control[]): Control[] {
  return controls.toSorted((a, b) => {
    const ra = PRESENTATION_ORDER.indexOf(a.name)
    const rb = PRESENTATION_ORDER.indexOf(b.name)
    if (ra !== -1 && rb !== -1) return ra - rb
    if (ra !== -1) return -1
    if (rb !== -1) return 1
    return a.name.localeCompare(b.name)
  })
}

/**
 * The distinct group labels among `controls`, in the order they appear.
 *
 * Lives here rather than in the panel because it decides **what is rendered
 * and in what order**, which is derivation however few lines it takes. The
 * component that first held it had no test that could reach it -- vitest runs
 * in `node` with no DOM -- and the phase's own rule is that a decision derived
 * on the way to the screen is a decision nothing in the gate can see.
 */
export function groupsOf(controls: Control[]): string[] {
  const seen = new Set<string>()
  const groups: string[] = []
  for (const entry of controls) {
    if (entry.group !== null && !seen.has(entry.group)) {
      seen.add(entry.group)
      groups.push(entry.group)
    }
  }
  return groups
}

/** Shown before anything has asked the node registry. */
const NOT_LOADED = 'Options come from your ComfyUI. Start it to choose a value.'

/**
 * Shown when the options arrived from comfy-cli's cache instead of ComfyUI.
 *
 * The transport error that came with it is deliberately **not** in this
 * sentence. Splicing it in produced, on a real screen: "...may be out of date.
 * served from cache (http://127.0.0.1:8188): cannot reach
 * http://127.0.0.1:8188/object_info: [WinError 10061] No connection could be
 * made because the target machine actively refused it Start ComfyUI and retry
 * to refresh them." Lowercase, unterminated, the URL twice, a Windows error
 * number, and the actual instruction stranded past all of it. Which endpoint
 * failed is what the ComfyUI status pill is for.
 */
const FROM_CACHE =
  "These options came from ComfyUI's cache and may be out of date. Start ComfyUI, then Retry."

/** One enum's live options, mirroring Rust `EnumOptions`. */
export type EnumOptions =
  | { state: 'loaded'; choices: string[]; cached: boolean }
  | { state: 'undeclared' }
  | { state: 'unavailable'; detail: string }

/**
 * Fill in the live options a `from_node_choices` enum was waiting for.
 *
 * Returns a new model; nothing is mutated. Controls with no entry are left
 * exactly as they were, so a partial answer fills what it can.
 *
 * **A cached answer is not a good answer.** `nodes(action="get")` succeeds with
 * ComfyUI down -- comfy-cli serves its own `object_info` cache and flags it. The
 * backend classifies that into one `cached` flag rather than passing the raw
 * signals up: a live read carries **neither** the `stale` key nor the warning,
 * and reading that absence correctly took observing both shapes rather than
 * assuming one (MCP-SURFACE 19.1). For key signatures a stale list is nearly
 * harmless; the same path feeds the LoRA picker in T-309, where it is a picker
 * missing the LoRA the user trained an hour ago (19.3).
 */
export function withChoices(
  model: PanelModel,
  options: Record<string, EnumOptions>,
): PanelModel {
  const apply = (entry: Control): Control => {
    if (entry.kind !== 'enum' || !entry.fromNode) return entry
    const answer = options[entry.name]
    if (answer === undefined) return entry

    switch (answer.state) {
      case 'loaded':
        return {
          ...entry,
          choices: answer.choices,
          optionsNote: answer.cached ? FROM_CACHE : null,
        }
      case 'undeclared':
        return {
          ...entry,
          optionsNote:
            'This model profile does not say which ComfyUI node supplies these options, so they cannot be loaded.',
        }
      case 'unavailable':
        return {
          ...entry,
          optionsNote: `${answer.detail} Start ComfyUI and retry.`,
        }
    }
  }

  return {
    basic: model.basic.map(apply),
    advanced: model.advanced.map(apply),
    omitted: model.omitted,
  }
}

/** The value every control starts at, keyed by input name. */
export function defaults(model: PanelModel): Record<string, ControlValue> {
  const values: Record<string, ControlValue> = {}
  for (const entry of [...model.basic, ...model.advanced]) {
    values[entry.name] = entry.default
  }
  return values
}

/**
 * Why this seed cannot be used, or `null` when it can.
 *
 * Rejecting rather than clamping is the point. A clamped seed still generates,
 * still writes a sidecar, and the sidecar is wrong -- which is the failure
 * `InputValue::Seed` was introduced to prevent, reappearing one layer above it.
 */
export function seedError(raw: string): string | null {
  const trimmed = raw.trim()
  if (trimmed === '') return 'Enter a seed.'
  if (!/^\d+$/.test(trimmed)) return 'A seed is a whole number, digits only.'
  if (Number(trimmed) > MAX_SAFE_SEED) {
    return `This app accepts seeds up to ${MAX_SAFE_SEED}. Larger seeds cannot be stored exactly here and would be recorded wrongly.`
  }
  return null
}

/** One tagged value, mirroring Rust `create_core::generation::InputValue`. */
export type InputValue =
  | { type: 'text'; value: string }
  | { type: 'int'; value: number }
  | { type: 'float'; value: number }
  | { type: 'seed'; value: number }
  | { type: 'enum'; value: string }
  | { type: 'bool'; value: boolean }

/**
 * Build the `inputs` map of a `GenerationSpec` from the panel's values.
 *
 * The tag comes from the **control's declared kind**, never from what the value
 * looks like at runtime. `InputValue` is adjacently tagged for exactly this
 * reason: untagged, a JSON `3` deserialises as `Int`, `Float` or `Seed`, and a
 * seed demoted to an `Int` makes a track unreproducible (generation.rs). Typing
 * off `typeof value` here would hand Rust the guess its own encoding refuses to
 * make.
 *
 * A control the panel never rendered is skipped -- `resolve_slots` rejects an
 * input the profile does not declare.
 *
 * **An empty text box is sent as empty, not skipped**, and that is the whole
 * point of the rule. `resolve_slots` writes only what the spec sets and
 * `fetch_template` carries the template's own defaults, so a skipped `tags`
 * left ACE-Step's demo prompt -- "Late Night Trap, 95 BPM, Heavy 808 Bass..." --
 * running underneath an empty box, with nothing on screen saying so
 * (MCP-SURFACE 20.2). The comment that used to sit here worried about a form
 * quietly overriding the workflow; the observed failure was the exact inverse.
 *
 * Enums and numbers still skip when unset: a `from_node_choices` enum whose
 * options have not loaded holds `''`, and sending that is an
 * `unknown_enum_value` rejection rather than an empty field.
 */
export function specInputs(
  model: PanelModel,
  values: Record<string, ControlValue>,
): Record<string, InputValue> {
  const inputs: Record<string, InputValue> = {}

  for (const entry of [...model.basic, ...model.advanced]) {
    const value = values[entry.name]
    if (value === undefined) continue
    const textual = entry.kind === 'text' || entry.kind === 'lyrics'
    if (value === '' && !textual) continue

    switch (entry.kind) {
      case 'text':
      case 'lyrics':
        inputs[entry.name] = { type: 'text', value: String(value) }
        break
      case 'enum':
        inputs[entry.name] = { type: 'enum', value: String(value) }
        break
      case 'int':
        inputs[entry.name] = { type: 'int', value: Number(value) }
        break
      case 'float':
        inputs[entry.name] = { type: 'float', value: Number(value) }
        break
      case 'seed':
        inputs[entry.name] = { type: 'seed', value: Number(value) }
        break
    }
  }
  return inputs
}
