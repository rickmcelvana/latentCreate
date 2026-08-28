import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import { getLoraPanel, type LoraPanel } from '../bridge/loras'
import {
  add,
  entryFor,
  move,
  removeAt,
  setStrengthAt,
  toggleAt,
  type StackRow,
} from './loras'

/**
 * The LoRA stack panel's state: which profile is loaded, what the user has
 * stacked, and whether the training checkpoints are disclosed.
 *
 * Everything derived lives in `loras.ts`. This holds only what a person has
 * actually done -- the same split `params.ts` / `paramPanel.ts` uses, for the
 * same reason.
 */
interface LoraPanelState {
  /** The profile this stack belongs to; `null` before the first load. */
  profileId: string | null
  /** `null` when this model has no LoRA support at all -- render nothing. */
  panel: LoraPanel | null
  stack: StackRow[]
  showSuperseded: boolean
  busy: boolean
  load: (profileId: string) => Promise<void>
  refresh: () => Promise<void>
  addPath: (path: string) => void
  removeRow: (index: number) => void
  toggleRow: (index: number) => void
  setStrength: (index: number, strength: number) => void
  moveRow: (from: number, to: number) => void
  toggleSuperseded: () => void
}

export const useLoraPanelStore = create<LoraPanelState>((set, get) => ({
  profileId: null,
  panel: null,
  stack: [],
  showSuperseded: false,
  busy: false,

  /**
   * Load one profile's LoRA support and installed list.
   *
   * **Reloading the same profile is a no-op, and switching profiles clears the
   * stack.** Both halves are load-bearing. A view re-mounts on every tab
   * switch, so re-running this would throw away a stack someone built; and a
   * LoRA chosen for ACE-Step is meaningless on another model -- MiniMax
   * Music 3 declares no `loras` block at all, so carrying the stack across
   * would leave rows attached to a panel that is not rendered, and they would
   * reappear on the way back.
   */
  load: async (profileId: string) => {
    if (!isTauri()) return
    if (get().profileId === profileId) return

    set({ profileId, stack: [], busy: true })
    try {
      set({ panel: await getLoraPanel(profileId) })
    } finally {
      set({ busy: false })
    }
  },

  /**
   * Re-read the installed list, keeping the stack.
   *
   * Separate from `load` and safe to call again, because ComfyUI is very often
   * started *after* the app. A failure leaves the panel as it was: the note
   * already on screen says the list could not be read, which is more useful
   * than replacing a working panel with an error.
   *
   * The stack survives on purpose. A row whose path the new catalog no longer
   * offers is reported by `missingFrom` rather than removed -- silently
   * dropping it would make the LoRA the user picked disappear with no account
   * of why.
   */
  refresh: async () => {
    const { profileId } = get()
    if (!isTauri() || profileId === null) return
    set({ busy: true })
    try {
      set({ panel: await getLoraPanel(profileId) })
    } catch {
      // Left as it was, note intact.
    } finally {
      set({ busy: false })
    }
  },

  /**
   * Stack the entry a picker handed back, by path.
   *
   * By path rather than by entry because a `<select>` yields a string, and
   * resolving it against the catalog in the component would be a lookup no test
   * can reach. An unknown path is ignored -- it cannot arrive from the picker,
   * and inventing a row for it would put a LoRA in the stack that the installed
   * list does not have.
   */
  addPath: (path: string) => {
    const { panel, stack } = get()
    if (panel === null) return
    const entry = entryFor(panel.catalog, path)
    if (entry === null) return
    set({ stack: add(stack, entry, panel) })
  },

  removeRow: (index: number) => set({ stack: removeAt(get().stack, index) }),

  toggleRow: (index: number) => set({ stack: toggleAt(get().stack, index) }),

  setStrength: (index: number, strength: number) => {
    const { panel, stack } = get()
    if (panel === null) return
    set({ stack: setStrengthAt(stack, index, strength, panel.strength) })
  },

  moveRow: (from: number, to: number) => set({ stack: move(get().stack, from, to) }),

  toggleSuperseded: () => set({ showSuperseded: !get().showSuperseded }),
}))
