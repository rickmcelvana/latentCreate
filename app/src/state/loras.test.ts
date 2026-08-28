import { describe, expect, it } from 'vitest'
import fixture from '../../../testdata/mcp/lora_catalog.ace-step.json'
import type { CatalogState, Excluded, LoraGroup, LoraPanel } from '../bridge/loras'
import {
  ADD_PLACEHOLDER,
  CANNOT_READ,
  EMPTY_STACK,
  FROM_CACHE,
  LOOSE_GROUP,
  add,
  addable,
  catalogNote,
  excludedNote,
  entryFor,
  fullNote,
  missingFrom,
  move,
  pickerGroups,
  removeAt,
  setStrengthAt,
  specLoras,
  stackRows,
  supersededCount,
  toggleAt,
  type PickerEntry,
  type StackRow,
} from './loras'

/**
 * The real install's catalog, generated from `create_core::loras::catalog` over
 * the verbatim 53-entry `lora_name` capture. A Rust test re-derives this file
 * and fails if the two drift, because nothing else generates one language's
 * fixture from the other's output.
 */
const groups = fixture.groups as unknown as LoraGroup[]
const excluded = fixture.excluded as unknown as Excluded[]

function loaded(cached = false): CatalogState {
  return { state: 'loaded', groups, excluded, cached }
}

/** The ACE-Step profile's own LoRA block: 0..2 step 0.05, four slots. */
function panel(overrides: Partial<LoraPanel> = {}): LoraPanel {
  return {
    strength: { min: 0, max: 2, default: 1, step: 0.05 },
    max_stack: 4,
    catalog: loaded(),
    ...overrides,
  }
}

function firstEntries(count: number): PickerEntry[] {
  return pickerGroups(loaded(), [], false)
    .flatMap((group) => group.entries)
    .slice(0, count)
}

function stackOf(...entries: PickerEntry[]): StackRow[] {
  return entries.reduce((stack, entry) => add(stack, entry, panel()), [] as StackRow[])
}

describe('pickerGroups', () => {
  /** Protects: the headline. 53 raw choices reach the picker as 12 offers. */
  it('test_the_real_list_reaches_the_picker_as_twelve_offers', () => {
    const picker = pickerGroups(loaded(), [], false)

    expect(picker).toHaveLength(6)
    expect(picker.flatMap((group) => group.entries)).toHaveLength(12)
  })

  /**
   * Protects: files loose in the `loras` root get a heading.
   *
   * `LoraGroup.name` is empty for them and `create-core` deliberately left the
   * word to this layer, so an empty `<optgroup>` label is exactly what happens
   * if nobody chooses one.
   */
  it('test_loose_files_are_given_a_heading', () => {
    const picker = pickerGroups(loaded(), [], false)
    const loose = picker.find((group) => group.label === LOOSE_GROUP)

    expect(loose?.entries).toHaveLength(2)
    expect(picker.some((group) => group.label === '')).toBe(false)
  })

  /**
   * Protects: 20 training checkpoints stay out of the picker until asked for.
   *
   * They are two thirds of the raw list. Showing them by default is the state
   * T-307 exists to prevent.
   */
  it('test_training_checkpoints_are_hidden_until_disclosed', () => {
    expect(supersededCount(loaded())).toBe(20)
    expect(pickerGroups(loaded(), [], false).flatMap((g) => g.entries)).toHaveLength(12)

    const disclosed = pickerGroups(loaded(), [], true).flatMap((g) => g.entries)
    expect(disclosed).toHaveLength(32)
    expect(disclosed.filter((entry) => entry.superseded)).toHaveLength(20)
  })

  /**
   * Protects: an entry already stacked is not offered a second time.
   *
   * Two loader nodes for one file is a strength the user could have set once,
   * applied twice, with nothing on screen saying so -- and it spends one of the
   * four slots doing it.
   *
   * Note what this test has to do to be worth anything: it stacks an entry and
   * then looks for **that same** entry. A test that stacked one and counted the
   * rest would pass with the filter deleted.
   */
  it('test_an_entry_already_stacked_is_not_offered_again', () => {
    const [first] = firstEntries(1)
    const stack = stackOf(first)

    const offered = pickerGroups(loaded(), stack, false).flatMap((group) => group.entries)

    expect(offered.some((entry) => entry.path === first.path)).toBe(false)
    expect(offered).toHaveLength(11)
  })

  /** Protects: a group emptied by the stack does not leave a bare heading. */
  it('test_a_group_with_nothing_left_to_offer_disappears', () => {
    const ambient = pickerGroups(loaded(), [], false).find(
      (group) => group.entries.length === 1,
    )
    expect(ambient).toBeDefined()

    const stack = stackOf(ambient!.entries[0])
    const labels = pickerGroups(loaded(), stack, false).map((group) => group.label)

    expect(labels).not.toContain(ambient!.label)
  })

  /** Protects: an unreadable catalog offers nothing rather than throwing. */
  it('test_an_unavailable_catalog_offers_nothing', () => {
    const down: CatalogState = { state: 'unavailable', detail: 'ComfyUI is not connected.' }

    expect(pickerGroups(down, [], true)).toEqual([])
    expect(supersededCount(down)).toBe(0)
    expect(excludedNote(down)).toBeNull()
  })
})

describe('the notes', () => {
  /**
   * Protects: the 21 dropped files are accounted for.
   *
   * Entries vanishing with no account of why is how a user concludes the app
   * cannot see their LoRAs -- and this particular exclusion is the one that
   * matters most, because a `training_state.pt` is a legitimate member of the
   * node's enum: it validates clean and then applies nothing at all.
   */
  it('test_the_excluded_files_are_counted_and_explained', () => {
    const note = excludedNote(loaded())

    expect(note).toContain('21')
    expect(note).toContain('not adapters')
  })

  /**
   * Protects: a cached list says what is **missing**, not what is suspect.
   *
   * The first version of this sentence had it backwards, warning that a cached
   * list offers files the user has deleted. Measured: a path the live server
   * does not know is rejected by `validate_workflow` as `unknown_enum_value`
   * before any GPU time (MCP-SURFACE 19.3). What a cached list actually costs
   * is the LoRA finished an hour ago being absent, so that is what it says.
   */
  it('test_the_cache_note_names_what_is_missing', () => {
    const note = catalogNote(loaded(true))

    expect(note).toBe(FROM_CACHE)
    expect(note).toContain('will not be here')
    expect(note).not.toMatch(/delet/i)
  })

  /** Protects: a healthy list is not warned about. */
  it('test_a_live_list_carries_no_note', () => {
    expect(catalogNote(loaded(false))).toBeNull()
  })

  /** Protects: an unreadable list says so, and says what to do. */
  it('test_an_unavailable_catalog_says_what_to_do', () => {
    const note = catalogNote({ state: 'unavailable', detail: 'ComfyUI is not connected.' })

    expect(note).toBe(CANNOT_READ)
    expect(note).toContain('Retry')
  })

  /**
   * Protects: the "full" wording follows the profile's cap.
   *
   * ACE-Step's four slots make `All 4 slots are full` and a hardcoded `4`
   * indistinguishable, which is the same vacuity the strength default had -- so
   * a one-slot profile is here to make it a rule.
   */
  it('test_the_full_note_counts_the_profiles_slots', () => {
    expect(fullNote(panel())).toBe('All 4 slots are full')
    expect(fullNote(panel({ max_stack: 1 }))).toBe('All 1 slot is full')
  })

  /** Protects: an empty stack does not read as something gone wrong. */
  it('test_the_empty_stack_reads_as_normal', () => {
    expect(EMPTY_STACK).not.toMatch(/error|missing|fail|must|should/i)
    expect(ADD_PLACEHOLDER).toContain('Add')
  })

  /**
   * Protects: the note is a sentence, not a transport error inside one.
   *
   * The param panel shipped exactly that defect and a person had to read it off
   * the screen to find it: comfy-cli's raw warning spliced mid-sentence, URL
   * twice, `[WinError 10061]`, and the one instruction that mattered stranded
   * past all of it. `detail` is deliberately not rendered here.
   */
  it('test_the_unavailable_note_does_not_splice_the_transport_error', () => {
    const note = catalogNote({
      state: 'unavailable',
      detail:
        'served from cache (http://127.0.0.1:8188): cannot reach http://127.0.0.1:8188/object_info: [WinError 10061]',
    })

    expect(note).not.toContain('http')
    expect(note).not.toContain('WinError')
    expect(note?.match(/\./g)).toHaveLength(2)
  })
})

describe('the stack', () => {
  /**
   * Protects: a new row takes the **profile's** default strength.
   *
   * `LoraLoaderModelOnly.strength_model` runs -100..100 in steps of 0.01 (read
   * live). Only about 0..2 is musically useful, so the profile narrows it, and
   * the panel follows the profile.
   */
  it('test_a_new_row_starts_at_the_profiles_default', () => {
    const [entry] = firstEntries(1)
    const stack = add([], entry, panel())

    expect(stack).toHaveLength(1)
    expect(stack[0].strength).toBe(1)
    expect(stack[0].enabled).toBe(true)
    expect(stack[0].path).toBe(entry.path)

    // ACE-Step's default happens to be 1, so the assertion above passes just as
    // well against a hardcoded 1. A profile that says something else is what
    // makes it a rule -- the same trap T-307 hit, where the captured list
    // already satisfied the ordering it was meant to prove.
    const quiet = panel({ strength: { min: 0, max: 3, default: 0.65, step: 0.05 } })
    expect(add([], entry, quiet)[0].strength).toBe(0.65)
  })

  /**
   * Protects: strength is held inside the profile's range, not the node's.
   *
   * Checked against two different ranges, because clamping to a hardcoded
   * `0..2` passes every assertion the ACE-Step profile can make.
   */
  it('test_strength_is_held_inside_the_profiles_range', () => {
    const stack = stackOf(...firstEntries(1))
    const range = panel().strength

    expect(setStrengthAt(stack, 0, 50, range)[0].strength).toBe(2)
    expect(setStrengthAt(stack, 0, -3, range)[0].strength).toBe(0)
    expect(setStrengthAt(stack, 0, 0.85, range)[0].strength).toBe(0.85)

    const wider = { min: 0.5, max: 3, default: 1, step: 0.1 }
    expect(setStrengthAt(stack, 0, 50, wider)[0].strength).toBe(3)
    expect(setStrengthAt(stack, 0, 0, wider)[0].strength).toBe(0.5)
  })

  /**
   * Protects: the cap is the profile's `max_stack`, not a constant.
   *
   * `splice_loras` returns `GraphError::TooManyLoras` above it, so a panel that
   * allowed one more would turn a Rust guard into a failed job -- the fourth
   * time this phase that a guard in one layer did not bind the layer above it.
   * A second panel declaring two slots is what makes the number a rule rather
   * than a coincidence.
   */
  it('test_the_cap_comes_from_the_profile', () => {
    const four = panel()
    const full = firstEntries(4).reduce((stack, e) => add(stack, e, four), [] as StackRow[])

    expect(full).toHaveLength(4)
    expect(addable(four, full)).toBe(false)
    expect(add(full, firstEntries(5)[4], four)).toBe(full)

    const two = panel({ max_stack: 2 })
    const capped = firstEntries(4).reduce((stack, e) => add(stack, e, two), [] as StackRow[])
    expect(capped).toHaveLength(2)
    expect(addable(two, capped)).toBe(false)
  })

  /** Protects: `add` refuses a duplicate even when the picker offered one. */
  it('test_add_refuses_a_path_already_in_the_stack', () => {
    const [entry] = firstEntries(1)
    const stack = add([], entry, panel())

    expect(add(stack, entry, panel())).toBe(stack)
  })

  /** Protects: bypassing a row keeps it in place rather than removing it. */
  it('test_bypassing_a_row_leaves_it_in_the_stack', () => {
    const stack = stackOf(...firstEntries(2))
    const after = toggleAt(stack, 0)

    expect(after).toHaveLength(2)
    expect(after[0].enabled).toBe(false)
    expect(after[1].enabled).toBe(true)
    expect(toggleAt(after, 0)[0].enabled).toBe(true)
  })

  it('test_removing_a_row_leaves_the_others_in_order', () => {
    const stack = stackOf(...firstEntries(3))
    const after = removeAt(stack, 1)

    expect(after.map((row) => row.path)).toEqual([stack[0].path, stack[2].path])
  })

  /**
   * Protects: reordering actually reorders.
   *
   * `splice_loras` chains the loaders in list order, so each LoRA is applied to
   * the model the one before it produced -- moving a row changes the audio.
   */
  it('test_moving_a_row_changes_the_order', () => {
    const stack = stackOf(...firstEntries(3))
    const paths = stack.map((row) => row.path)

    expect(move(stack, 2, 0).map((row) => row.path)).toEqual([paths[2], paths[0], paths[1]])
    expect(move(stack, 0, 2).map((row) => row.path)).toEqual([paths[1], paths[2], paths[0]])
  })

  /** Protects: a move that cannot happen leaves the stack alone. */
  it('test_a_move_out_of_bounds_is_a_no_op', () => {
    const stack = stackOf(...firstEntries(2))

    expect(move(stack, 0, 0)).toBe(stack)
    expect(move(stack, 0, 5)).toBe(stack)
    expect(move(stack, -1, 0)).toBe(stack)
  })
})

describe('missingFrom', () => {
  /**
   * Protects: a row the catalog no longer offers is named, not blanked.
   *
   * Reachable by deleting a LoRA and pressing Retry. The pipeline would reject
   * the same path as `unknown_enum_value`, but only after Generate -- saying it
   * in the panel is strictly earlier feedback, and it is the reason `StackRow`
   * carries its own label instead of resolving one through the catalog.
   */
  it('test_a_row_the_catalog_no_longer_lists_is_named', () => {
    const stack: StackRow[] = [
      { path: 'gone/adapter_model.safetensors', label: 'gone', strength: 1, enabled: true },
      ...stackOf(...firstEntries(1)),
    ]

    const missing = missingFrom(stack, loaded())

    expect(missing).toHaveLength(1)
    expect(missing[0].label).toBe('gone')
  })

  /**
   * Protects: an unreadable catalog does not report the whole stack missing.
   *
   * With ComfyUI down, "not installed" is not something this can know, and
   * flagging every row would be a false alarm -- the kind that gets a check
   * ignored on the day it is right.
   */
  it('test_an_unreadable_catalog_reports_nothing_missing', () => {
    const stack = stackOf(...firstEntries(2))
    const down: CatalogState = { state: 'unavailable', detail: 'ComfyUI is not connected.' }

    expect(missingFrom(stack, down)).toEqual([])
  })
})

describe('specLoras', () => {
  /**
   * Protects: a bypassed row still reaches the spec.
   *
   * `GenerationSpec.loras` records the stack the user built and Rust's
   * `active_loras()` is what filters it before the splice. Dropping disabled
   * rows here would make a bypass indistinguishable from a delete in the
   * provenance sidecar -- the record of how a track was actually made.
   */
  it('test_a_bypassed_row_is_still_in_the_spec', () => {
    const stack = toggleAt(stackOf(...firstEntries(2)), 0)
    const spec = specLoras(stack)

    expect(spec).toHaveLength(2)
    expect(spec[0].enabled).toBe(false)
    expect(spec[1].enabled).toBe(true)
  })

  /** Protects: the spec carries the apply order the user arranged. */
  it('test_the_spec_follows_the_stack_order', () => {
    const stack = stackOf(...firstEntries(3))
    const moved = move(stack, 2, 0)

    expect(specLoras(moved).map((ref) => ref.file)).toEqual([
      stack[2].path,
      stack[0].path,
      stack[1].path,
    ])
  })

  /**
   * Protects: the path crosses to Rust **verbatim**.
   *
   * Every path on the reference install contains backslashes, and the value is
   * the identity the loader node is given. Normalising the separators produces
   * a string ComfyUI rejects as `unknown_enum_value`.
   */
  it('test_the_path_is_passed_through_untouched', () => {
    const stack = stackOf(...firstEntries(1))
    const spec = specLoras(stack)

    expect(spec[0].file).toBe(stack[0].path)
    expect(spec[0].file).toContain('\\')
  })
})

describe('entryFor', () => {
  /**
   * Protects: a path from a `<select>` resolves to the entry behind it.
   *
   * The picker hands back a string; the stack needs a label. Resolving that in
   * the component would be a lookup no test can reach, which is how the last
   * three panels acquired their untestable lines.
   */
  it('test_a_path_resolves_to_its_offer', () => {
    const [first] = firstEntries(1)
    const found = entryFor(loaded(), first.path)

    expect(found?.label).toBe(first.label)
    expect(found?.superseded).toBe(false)
  })

  /** Protects: a superseded checkpoint resolves, and knows that it is one. */
  it('test_a_checkpoint_resolves_and_is_marked_superseded', () => {
    const checkpoint = pickerGroups(loaded(), [], true)
      .flatMap((group) => group.entries)
      .find((entry) => entry.superseded)
    expect(checkpoint).toBeDefined()

    const found = entryFor(loaded(), checkpoint!.path)

    expect(found?.superseded).toBe(true)
    expect(found?.epoch).toBe(checkpoint!.epoch)
  })

  /**
   * Protects: an unknown path resolves to nothing rather than to a made-up row.
   *
   * A row invented here would reach `GenerationSpec.loras` and the provenance
   * sidecar naming a LoRA the install does not have.
   */
  it('test_an_unknown_path_resolves_to_nothing', () => {
    expect(entryFor(loaded(), 'nothing/like/this.safetensors')).toBeNull()
    expect(entryFor({ state: 'unavailable', detail: 'down' }, firstEntries(1)[0].path)).toBeNull()
  })
})

describe('stackRows', () => {
  /**
   * Protects: the view gets its decisions made for it.
   *
   * `missing`, the sentence, and whether each move button does anything are all
   * one line apiece in a component -- and unreachable from any test the moment
   * they live there, because vitest runs in `node` with no DOM.
   */
  it('test_the_rows_carry_their_own_move_affordances', () => {
    const rows = stackRows(stackOf(...firstEntries(3)), loaded())

    expect(rows.map((view) => view.canMoveUp)).toEqual([false, true, true])
    expect(rows.map((view) => view.canMoveDown)).toEqual([true, true, false])
    expect(rows.map((view) => view.index)).toEqual([0, 1, 2])
  })

  /** Protects: a single row can move nowhere, rather than up onto itself. */
  it('test_a_lone_row_can_move_neither_way', () => {
    const [only] = stackRows(stackOf(...firstEntries(1)), loaded())

    expect(only.canMoveUp).toBe(false)
    expect(only.canMoveDown).toBe(false)
  })

  /** Protects: a row the catalog no longer offers carries its own sentence. */
  it('test_a_missing_row_carries_its_note', () => {
    const stack: StackRow[] = [
      ...stackOf(...firstEntries(1)),
      { path: 'gone/adapter_model.safetensors', label: 'gone', strength: 1, enabled: true },
    ]

    const [present, absent] = stackRows(stack, loaded())

    expect(present.missing).toBe(false)
    expect(present.note).toBeNull()
    expect(absent.missing).toBe(true)
    expect(absent.note).toContain('gone')
    expect(absent.note).toContain('no longer in your loras folder')
  })

  /** Protects: ComfyUI being down does not mark the whole stack missing. */
  it('test_an_unreadable_catalog_marks_no_row_missing', () => {
    const rows = stackRows(stackOf(...firstEntries(2)), {
      state: 'unavailable',
      detail: 'down',
    })

    expect(rows.every((view) => !view.missing)).toBe(true)
    expect(rows.every((view) => view.note === null)).toBe(true)
  })
})
