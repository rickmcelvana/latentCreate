import { beforeEach, describe, expect, it, vi } from 'vitest'
import fixture from '../../../testdata/mcp/lora_catalog.ace-step.json'
import type { Excluded, LoraGroup, LoraPanel } from '../bridge/loras'
import { pickerGroups } from './loras'
import { useLoraPanelStore } from './loraPanel'

const groups = fixture.groups as unknown as LoraGroup[]
const excluded = fixture.excluded as unknown as Excluded[]

const acePanel: LoraPanel = {
  strength: { min: 0, max: 2, default: 1, step: 0.05 },
  max_stack: 4,
  catalog: { state: 'loaded', groups, excluded, cached: false },
}

let mockIsTauri = true
let mockPanel: LoraPanel | null = acePanel
let calls = 0

vi.mock('../bridge/comfy', () => ({ isTauri: () => mockIsTauri }))
vi.mock('../bridge/loras', () => ({
  getLoraPanel: () => {
    calls += 1
    return Promise.resolve(mockPanel)
  },
}))

/** The first N offers the real catalog makes. */
function offers(count: number) {
  return pickerGroups(acePanel.catalog, [], false)
    .flatMap((group) => group.entries)
    .slice(0, count)
}

beforeEach(() => {
  mockIsTauri = true
  mockPanel = acePanel
  calls = 0
  useLoraPanelStore.setState({
    profileId: null,
    panel: null,
    stack: [],
    showSuperseded: false,
    busy: false,
  })
})

describe('useLoraPanelStore', () => {
  it('test_load_brings_back_the_profiles_panel', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    const state = useLoraPanelStore.getState()

    expect(state.profileId).toBe('ace-step-1.5-turbo')
    expect(state.panel?.max_stack).toBe(4)
    expect(state.stack).toEqual([])
  })

  /**
   * Protects: a model with no LoRA support loads to nothing, and says nothing.
   *
   * MiniMax Music 3 declares no `loras` block. `null` is *render nothing* --
   * distinct from an unreadable catalog, which is a visible panel with a
   * sentence in it. Collapsing the two is how a user with ComfyUI switched off
   * concludes their model cannot take LoRAs.
   */
  it('test_a_model_without_lora_support_has_no_panel', async () => {
    mockPanel = null
    await useLoraPanelStore.getState().load('minimax-music-3')

    expect(useLoraPanelStore.getState().panel).toBeNull()
    expect(useLoraPanelStore.getState().profileId).toBe('minimax-music-3')
  })

  /**
   * Protects: switching views does not throw away the stack someone built.
   *
   * A view re-mounts on every tab switch, so `load` runs again with the same
   * id. Re-fetching there would clear the stack, and the user would come back
   * from the Lyrics tab to an empty panel.
   */
  it('test_reloading_the_same_profile_keeps_the_stack', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    useLoraPanelStore.getState().addPath(offers(1)[0].path)
    expect(useLoraPanelStore.getState().stack).toHaveLength(1)

    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')

    expect(useLoraPanelStore.getState().stack).toHaveLength(1)
    expect(calls).toBe(1)
  })

  /**
   * Protects: switching profiles clears the stack.
   *
   * A LoRA chosen for ACE-Step is meaningless on another model, and MiniMax
   * declares no `loras` block at all -- so a carried-over stack would sit
   * attached to a panel that is never rendered, and reappear on the way back.
   */
  it('test_switching_profiles_clears_the_stack', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    useLoraPanelStore.getState().addPath(offers(1)[0].path)

    mockPanel = null
    await useLoraPanelStore.getState().load('minimax-music-3')

    expect(useLoraPanelStore.getState().stack).toEqual([])
    expect(useLoraPanelStore.getState().panel).toBeNull()
  })

  /**
   * Protects: Retry keeps the stack.
   *
   * ComfyUI is very often started after the app, so `refresh` is the way out of
   * an unreadable catalog. Clearing the stack there would punish the user for
   * fixing the problem, and a row the new list no longer offers is reported by
   * `missingFrom` rather than silently dropped.
   */
  it('test_refresh_rereads_the_list_and_keeps_the_stack', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    useLoraPanelStore.getState().addPath(offers(1)[0].path)

    await useLoraPanelStore.getState().refresh()

    expect(calls).toBe(2)
    expect(useLoraPanelStore.getState().stack).toHaveLength(1)
  })

  /** Protects: the store's edits go through the pure rules, cap included. */
  it('test_the_store_will_not_stack_past_the_profiles_cap', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    for (const entry of offers(6)) useLoraPanelStore.getState().addPath(entry.path)

    expect(useLoraPanelStore.getState().stack).toHaveLength(4)
  })

  it('test_the_store_moves_toggles_and_removes_rows', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    for (const entry of offers(3)) useLoraPanelStore.getState().addPath(entry.path)
    const paths = useLoraPanelStore.getState().stack.map((row) => row.path)

    useLoraPanelStore.getState().moveRow(2, 0)
    expect(useLoraPanelStore.getState().stack.map((row) => row.path)).toEqual([
      paths[2],
      paths[0],
      paths[1],
    ])

    useLoraPanelStore.getState().toggleRow(0)
    expect(useLoraPanelStore.getState().stack[0].enabled).toBe(false)

    useLoraPanelStore.getState().setStrength(0, 9)
    expect(useLoraPanelStore.getState().stack[0].strength).toBe(2)

    useLoraPanelStore.getState().removeRow(0)
    expect(useLoraPanelStore.getState().stack).toHaveLength(2)
  })

  /**
   * Protects: a path the catalog does not offer never becomes a row.
   *
   * The picker cannot produce one, but the store takes a bare string, so this
   * is the layer where a stale or hand-edited value would arrive. Inventing a
   * row for it would put a LoRA in the stack -- and in the provenance sidecar
   * -- that the installed list does not have.
   */
  it('test_an_unknown_path_is_not_stacked', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    useLoraPanelStore.getState().addPath('nothing/like/this.safetensors')

    expect(useLoraPanelStore.getState().stack).toEqual([])
  })

  it('test_the_checkpoint_disclosure_toggles', async () => {
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')
    expect(useLoraPanelStore.getState().showSuperseded).toBe(false)

    useLoraPanelStore.getState().toggleSuperseded()
    expect(useLoraPanelStore.getState().showSuperseded).toBe(true)
  })

  /** Protects: nothing is invoked outside Tauri -- the browser dev server. */
  it('test_nothing_is_read_outside_tauri', async () => {
    mockIsTauri = false
    await useLoraPanelStore.getState().load('ace-step-1.5-turbo')

    expect(calls).toBe(0)
    expect(useLoraPanelStore.getState().panel).toBeNull()
  })
})
