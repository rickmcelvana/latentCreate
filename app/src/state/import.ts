import { create } from 'zustand'
import {
  importWorkflow,
  saveImportedProfile,
  type Candidate,
  type ImportReport,
  type Role,
  type RoleMapping,
  type RoleSuggestion,
} from '../bridge/import'

/**
 * Every role a mapping screen shows, in reading order.
 *
 * Mirrors `create_core::roles::Role::ALL`. Listed here rather than derived from
 * the suggestions, because a role the app found **nothing** for is still a row:
 * hiding it would leave a person unable to see what was not matched.
 */
export const ROLES: Role[] = [
  'tags',
  'lyrics',
  'negative',
  'duration_seconds',
  'seed',
  'steps',
  'cfg',
]

/** The label for each role. Same words `emit::label_for` uses, so the mapping
 * screen and the panel it generates agree. */
const LABELS: Record<Role, string> = {
  tags: 'Style tags',
  lyrics: 'Lyrics',
  negative: 'Negative prompt',
  duration_seconds: 'Duration (s)',
  seed: 'Seed',
  steps: 'Steps',
  cfg: 'CFG',
}

/** Which addresses are ticked, per role. */
export type Selection = Partial<Record<Role, string[]>>

/**
 * What the app pre-selects.
 *
 * **`possible` candidates are never ticked, and this is the whole point of the
 * confidence field.** A `possible` candidate was reached by following a link:
 * ACE-Step's seed is `109.value`, whose name and node class say nothing about
 * seeds -- it is right because of the graph's shape, which is a reason to put
 * it top of the list with its reason showing, not a reason to check it for
 * someone.
 *
 * If this function ticked everything, the failure would be silent and total:
 * the user saves, the profile is written, generation works, and the seed they
 * believe they chose is the one the app guessed. Nothing errors.
 *
 * A role whose only candidates are `possible` therefore starts **empty**, which
 * is the honest state -- we found something, and we are not claiming it.
 */
export function initialSelection(suggestions: RoleSuggestion[]): Selection {
  const selection: Selection = {}
  for (const suggestion of suggestions) {
    const strong = suggestion.candidates
      .filter((c) => c.confidence === 'strong')
      .map((c) => c.address)
    if (strong.length > 0) selection[suggestion.role] = strong
  }
  return selection
}

/** One row of the mapping screen, with every decision already made. */
export interface RoleRow {
  role: Role
  label: string
  /** Candidates in the order Rust ranked them, each with its checked state. */
  options: { candidate: Candidate; checked: boolean }[]
  /** True when this role will reach the emitted profile. */
  mapped: boolean
  /** Shown when nothing was found, so the row still says something. */
  emptyNote: string | null
}

/** Every role as a row, mapped or not. The view renders these and derives
 * nothing. */
export function roleRows(suggestions: RoleSuggestion[], selected: Selection): RoleRow[] {
  return ROLES.map((role) => {
    const candidates = suggestions.find((s) => s.role === role)?.candidates ?? []
    const chosen = selected[role] ?? []
    return {
      role,
      label: LABELS[role],
      options: candidates.map((candidate) => ({
        candidate,
        checked: chosen.includes(candidate.address),
      })),
      mapped: chosen.length > 0,
      emptyNote:
        candidates.length === 0
          ? 'No input in this workflow looks like this. Leave it unmapped.'
          : null,
    }
  })
}

/**
 * Tick or untick one address.
 *
 * A role holds a **set** of addresses rather than one, because ACE-Step's
 * duration legitimately needs two -- `94.duration` and `98.seconds` are both
 * real and both land (MCP-SURFACE 29.5).
 */
export function toggleAddress(selected: Selection, role: Role, address: string): Selection {
  const chosen = selected[role] ?? []
  const next = chosen.includes(address)
    ? chosen.filter((a) => a !== address)
    : [...chosen, address]
  const updated: Selection = { ...selected }
  if (next.length > 0) {
    updated[role] = next
  } else {
    delete updated[role]
  }
  return updated
}

/**
 * Whether Save should be offered.
 *
 * A name **and** at least one mapped role. A profile with no inputs is a picker
 * entry that can do nothing at all, and an unnamed one is a row nobody can
 * identify later.
 *
 * **Warnings are deliberately not consulted.** A graph that demonstrably
 * produces audio carries three of them (MCP-SURFACE 29.3); this is the far side
 * of the wire from T-313b's Rust-side rule, and the place it would most
 * plausibly be re-imposed by accident.
 */
export function canSave(name: string, selected: Selection): boolean {
  return name.trim() !== '' && Object.keys(selected).length > 0
}

/**
 * Advisory lines shown above Save. **Never** disable it.
 *
 * An imported profile with no seed input has no seed control, and T-312's
 * "queue N variations by seed" then queues N runs varying nothing. It does not
 * error, and on ACE-Step the tracks differ anyway because the model is not
 * reproducible run-to-run (MCP-SURFACE 17.3) -- so nothing on screen would ever
 * reveal it.
 *
 * That is the default path, not an edge case: on an ACE-Step-shaped graph the
 * seed is always the `possible` hop, which is never pre-ticked. The answer is
 * to say what the choice costs, **not** to tick it for someone -- that would
 * re-introduce the silent-guess failure the pre-tick rule exists to prevent.
 */
export function saveNotes(selected: Selection): string[] {
  const notes: string[] = []
  if ((selected.seed ?? []).length === 0) {
    notes.push(
      'No seed mapped, so Variations will queue identical settings. Tick a seed row to change that.',
    )
  }
  return notes
}

/** The mappings, in the shape the command takes. */
export function mappingsOf(selected: Selection): RoleMapping[] {
  return ROLES.filter((role) => (selected[role] ?? []).length > 0).map((role) => ({
    role,
    addresses: selected[role] ?? [],
  }))
}

/** Where the import flow has got to. */
export type ImportPhase =
  | { kind: 'idle' }
  | { kind: 'importing' }
  | { kind: 'mapping' }
  | { kind: 'saving' }
  | { kind: 'saved'; profileId: string }
  | { kind: 'failed'; message: string }

interface ImportState {
  phase: ImportPhase
  report: ImportReport | null
  selected: Selection
  name: string
  begin: (source: string) => Promise<void>
  toggle: (role: Role, address: string) => void
  setName: (name: string) => void
  save: () => Promise<void>
  reset: () => void
}

export const useImportStore = create<ImportState>((set, get) => ({
  phase: { kind: 'idle' },
  report: null,
  selected: {},
  name: '',

  begin: async (source: string) => {
    set({ phase: { kind: 'importing' } })
    try {
      const report = await importWorkflow(source)
      set({
        phase: { kind: 'mapping' },
        report,
        selected: initialSelection(report.suggestions),
        // A name they can recognise later, and one they can overwrite.
        name: report.workflow_id,
      })
    } catch (e) {
      set({ phase: { kind: 'failed', message: String(e) } })
    }
  },

  toggle: (role: Role, address: string) => {
    set((state) => ({ selected: toggleAddress(state.selected, role, address) }))
  },

  setName: (name: string) => set({ name }),

  save: async () => {
    const { report, name, selected } = get()
    if (report === null || !canSave(name, selected)) return
    set({ phase: { kind: 'saving' } })
    try {
      const saved = await saveImportedProfile(report.workflow_id, name.trim(), mappingsOf(selected))
      set({ phase: { kind: 'saved', profileId: saved.profile_id } })
    } catch (e) {
      set({ phase: { kind: 'failed', message: String(e) } })
    }
  },

  reset: () => set({ phase: { kind: 'idle' }, report: null, selected: {}, name: '' }),
}))
