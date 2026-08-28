import type { CatalogState, LoraEntry, LoraPanel } from '../bridge/loras'

/**
 * The LoRA stack panel's derivations: what the picker offers, what the stack
 * holds, and what reaches Rust as `GenerationSpec.loras`.
 *
 * Pure functions rather than JSX, for the reason `params.ts` gives at length:
 * vitest runs in `node` with no DOM, so a rule that lives in a component is a
 * rule no test can reach. That matters more here than it did for the param
 * panel, because most of these rules are about *order* and *absence* -- neither
 * of which shows up in a screenshot.
 */

/** One entry the user has stacked, in the order it will be applied. */
export interface StackRow {
  /** The choice verbatim, exactly as ComfyUI lists it. */
  path: string
  /**
   * Carried on the row rather than looked up.
   *
   * A row has to keep rendering after a Retry whose catalog no longer lists it
   * -- see [`missingFrom`]. Resolving the label through the catalog would blank
   * exactly the row the user needs to see in order to remove it.
   */
  label: string
  strength: number
  /** Bypassed rows stay in the list, and stay in the spec. See [`specLoras`]. */
  enabled: boolean
}

/** One offer in the picker. */
export interface PickerEntry {
  path: string
  label: string
  /** A superseded training step, shown only when the user asks for them. */
  superseded: boolean
  epoch: number | null
}

/** The picker's options under one heading. */
export interface PickerGroup {
  label: string
  entries: PickerEntry[]
}

/**
 * Heading for adapters sitting loose in the `loras` root.
 *
 * `LoraGroup.name` is empty for these and `create-core` deliberately left the
 * wording to the panel. On the reference install this holds the two
 * `minimax_h3_fl2v_turbo_*` video LoRAs, which cannot be filtered out by
 * filename (MCP-SURFACE 4) and are therefore still offered.
 */
export const LOOSE_GROUP = 'Loose files'

/**
 * Shown when the list came from comfy-cli's cache rather than from ComfyUI.
 *
 * What a cached list costs is narrower than it looks, and the first version of
 * this sentence had it backwards. A cached list is a **short** list: a LoRA
 * added since ComfyUI last ran is simply absent. It does not offer ghosts --
 * a path the live server does not know is rejected by `validate_workflow` as
 * `unknown_enum_value` before any GPU time (MCP-SURFACE 19.3). So the sentence
 * names what is missing instead of cautioning about what is shown.
 */
export const FROM_CACHE =
  "This list came from ComfyUI's cache. A LoRA added since ComfyUI last ran will not be here. Start ComfyUI, then Retry."

/** Shown when the loader node could not be read at all. */
export const CANNOT_READ =
  'Your installed LoRAs could not be read because ComfyUI is not running. Start it, then Retry.'

/**
 * What the picker offers, grouped, with everything already stacked removed.
 *
 * **A path already in the stack is not offered again.** Two loader nodes for
 * one file is a strength the user could have set once, applied twice, with
 * nothing on screen saying so -- and it spends one of the profile's `max_stack`
 * slots doing it.
 *
 * Superseded training steps are behind `showSuperseded` because one run
 * contributes 20 of them on the reference install, which is the single biggest
 * reason the raw list is unusable (T-307).
 */
export function pickerGroups(
  catalog: CatalogState,
  stack: StackRow[],
  showSuperseded: boolean,
): PickerGroup[] {
  if (catalog.state !== 'loaded') return []
  const taken = new Set(stack.map((row) => row.path))

  return catalog.groups
    .map((group) => ({
      label: group.name === '' ? LOOSE_GROUP : group.name,
      entries: [
        ...group.primary.map((entry) => offer(entry, false)),
        ...(showSuperseded ? group.superseded.map((entry) => offer(entry, true)) : []),
      ].filter((entry) => !taken.has(entry.path)),
    }))
    .filter((group) => group.entries.length > 0)
}

function offer(entry: LoraEntry, superseded: boolean): PickerEntry {
  return { path: entry.path, label: entry.label, superseded, epoch: entry.epoch }
}

/** How many training checkpoints are hidden behind the disclosure. */
export function supersededCount(catalog: CatalogState): number {
  if (catalog.state !== 'loaded') return 0
  return catalog.groups.reduce((total, group) => total + group.superseded.length, 0)
}

/**
 * Why the picker is shorter than the folder, or `null` when it is not.
 *
 * Reported rather than dropped: 21 of 53 entries vanishing with no account of
 * why is how a user concludes the app cannot see their LoRAs. It is also the
 * exclusion that matters most -- a `training_state.pt` is a legitimate member
 * of the node's enum, so it validates clean and then applies nothing at all
 * (MCP-SURFACE 17.6, 19.3).
 */
export function excludedNote(catalog: CatalogState): string | null {
  if (catalog.state !== 'loaded' || catalog.excluded.length === 0) return null
  const n = catalog.excluded.length
  return `${n} ${n === 1 ? 'file' : 'files'} in your loras folder ${
    n === 1 ? 'is not an adapter' : 'are not adapters'
  } and ${n === 1 ? 'is' : 'are'} not offered.`
}

/** Why this list should not be fully trusted, or `null` when it can be. */
export function catalogNote(catalog: CatalogState): string | null {
  if (catalog.state === 'unavailable') return CANNOT_READ
  return catalog.cached ? FROM_CACHE : null
}

/** Whether another entry may be stacked. */
export function addable(panel: LoraPanel, stack: StackRow[]): boolean {
  return stack.length < panel.max_stack
}

/**
 * Stack one entry at the end, at the profile's default strength.
 *
 * Refuses a duplicate and refuses to exceed `max_stack`, even though
 * [`pickerGroups`] and [`addable`] already prevent both from the UI. That is
 * not belt-and-braces for its own sake: `splice_loras` returns
 * `GraphError::TooManyLoras` above the cap, so a panel that let someone add one
 * more would turn a Rust guard into a failed job. This phase has now found the
 * same shape four times -- **a guard in one layer does not bind the layer above
 * it** -- so the layer above carries its own.
 */
export function add(stack: StackRow[], entry: PickerEntry, panel: LoraPanel): StackRow[] {
  if (stack.length >= panel.max_stack) return stack
  if (stack.some((row) => row.path === entry.path)) return stack

  return [
    ...stack,
    {
      path: entry.path,
      label: entry.label,
      strength: panel.strength.default,
      enabled: true,
    },
  ]
}

export function removeAt(stack: StackRow[], index: number): StackRow[] {
  return stack.filter((_, at) => at !== index)
}

/** Bypass or re-enable one row, leaving it in place. */
export function toggleAt(stack: StackRow[], index: number): StackRow[] {
  return stack.map((row, at) => (at === index ? { ...row, enabled: !row.enabled } : row))
}

/** Set one row's strength, held inside the profile's range. */
export function setStrengthAt(
  stack: StackRow[],
  index: number,
  strength: number,
  range: LoraPanel['strength'],
): StackRow[] {
  const held = Math.min(Math.max(strength, range.min), range.max)
  return stack.map((row, at) => (at === index ? { ...row, strength: held } : row))
}

/**
 * Move one row to another position.
 *
 * Order is not cosmetic: `splice_loras` chains the loader nodes in list order,
 * so each LoRA is applied to the model the one before it produced, and moving a
 * row changes the audio.
 */
export function move(stack: StackRow[], from: number, to: number): StackRow[] {
  if (from === to || from < 0 || to < 0 || from >= stack.length || to >= stack.length) {
    return stack
  }
  const next = [...stack]
  const [row] = next.splice(from, 1)
  next.splice(to, 0, row)
  return next
}

/**
 * Rows whose path the catalog no longer offers.
 *
 * Reachable by deleting a LoRA and pressing Retry. Naming it here is strictly
 * earlier feedback than the alternative: the pipeline's `validate_workflow`
 * step would reject the same path as `unknown_enum_value`, but only after the
 * user pressed Generate (MCP-SURFACE 19.3).
 *
 * An unreadable catalog reports nothing rather than everything -- with ComfyUI
 * down, "not installed" is not a thing this can know, and marking the whole
 * stack missing would be a false alarm that gets the check ignored.
 */
export function missingFrom(stack: StackRow[], catalog: CatalogState): StackRow[] {
  if (catalog.state !== 'loaded') return []
  const known = new Set(
    catalog.groups.flatMap((group) =>
      [...group.primary, ...group.superseded].map((entry) => entry.path),
    ),
  )
  return stack.filter((row) => !known.has(row.path))
}

/** How a missing row explains itself. Rendered through [`stackRows`]. */
function missingNote(row: StackRow): string {
  return `${row.label} is no longer in your loras folder.`
}

/** One offer, found by the path a `<select>` hands back. */
export function entryFor(catalog: CatalogState, path: string): PickerEntry | null {
  if (catalog.state !== 'loaded') return null
  for (const group of catalog.groups) {
    const primary = group.primary.find((entry) => entry.path === path)
    if (primary !== undefined) return offer(primary, false)
    const superseded = group.superseded.find((entry) => entry.path === path)
    if (superseded !== undefined) return offer(superseded, true)
  }
  return null
}

/** One stacked row, with everything the view would otherwise work out itself. */
export interface StackRowView {
  row: StackRow
  index: number
  /** This LoRA is no longer in the installed list. */
  missing: boolean
  /** The sentence saying so, or `null`. */
  note: string | null
  canMoveUp: boolean
  canMoveDown: boolean
}

/**
 * The stack as the panel renders it.
 *
 * Every one of these is a decision -- whether a row is still installed, what it
 * says about that, whether its move buttons do anything -- and each is one line
 * in a component and unreachable from a test the moment it lives there. The
 * repo has paid for that lesson at every panel so far, so the component below
 * this is a `map` over this list and nothing else.
 */
export function stackRows(stack: StackRow[], catalog: CatalogState): StackRowView[] {
  const missing = new Set(missingFrom(stack, catalog).map((row) => row.path))

  return stack.map((row, index) => ({
    row,
    index,
    missing: missing.has(row.path),
    note: missing.has(row.path) ? missingNote(row) : null,
    canMoveUp: index > 0,
    canMoveDown: index < stack.length - 1,
  }))
}

/** One LoRA in a spec, mirroring Rust `create_core::generation::LoraRef`. */
export interface LoraRef {
  file: string
  strength: number
  enabled: boolean
}

/**
 * The stack as `GenerationSpec.loras`.
 *
 * **Bypassed rows are included.** `GenerationSpec.loras` records the stack the
 * user built and `active_loras()` is what filters it before the splice --
 * generation.rs says so in its own doc, and the provenance sidecar records the
 * whole list. Dropping disabled rows here would make a bypass indistinguishable
 * from a delete in the record of how a track was made.
 *
 * Order is preserved, because it is the apply order.
 */
export function specLoras(stack: StackRow[]): LoraRef[] {
  return stack.map((row) => ({
    file: row.path,
    strength: row.strength,
    enabled: row.enabled,
  }))
}
