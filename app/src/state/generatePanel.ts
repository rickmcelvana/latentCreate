import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import { generateAudio, type Submission } from '../bridge/generate'
import { approvedFill, blockers, seedControl, specsFor } from './generate'
import { useJobsStore } from './jobs'
import { useLoraPanelStore } from './loraPanel'
import { useLyricsStore } from './lyrics'
import { freshSeed, useParamPanelStore } from './paramPanel'

/**
 * Pressing Generate: assemble, submit, and hand the job to the queue.
 *
 * Reads the two panels rather than holding a copy of them -- the settings and
 * the LoRA stack each already have an owner, and a third store keeping its own
 * version is how the thing submitted stops matching the thing on screen.
 */
interface GenerateState {
  /** A submit is in flight. Two clicks would queue two jobs. */
  busy: boolean
  /** The command's own error, verbatim and unspliced. */
  error: string | null
  /** What was queued last, or `null` before anything was. */
  last: Submission | null
  /** The profile `last` was generated for -- see `notesFor`. */
  lastProfileId: string | null
  /** How many variations this click will queue. Not a profile input. */
  count: number
  /** How many of the current batch ComfyUI has accepted so far. */
  queued: number
  /**
   * The Audio Studio title override, or `null` to follow the selected document.
   * `null` means "the user has not touched the field", so it shows and uses the
   * selected doc's own title; any string (including `''`, meaning untitled) is a
   * deliberate override that persists across a document switch.
   */
  title: string | null
  submit: () => Promise<void>
  setCount: (n: number) => void
  setTitle: (title: string) => void
  useApprovedLyric: () => void
}

export const useGenerateStore = create<GenerateState>((set, get) => ({
  busy: false,
  error: null,
  last: null,
  lastProfileId: null,
  count: 1,
  queued: 0,
  title: null,

  /**
   * Queue one or more generations.
   *
   * The `blockers` check is repeated here even though the button is disabled
   * on it. That is not belt-and-braces: the button is a view, and this is the
   * layer that decides what reaches Rust -- the same reason `add` refuses a
   * duplicate the picker never offers.
   *
   * **`register` is the load-bearing line.** `generate_audio` starts the job
   * pump itself, and without this the queue panel never hears about the job it
   * is running (jobs.ts). In a batch it happens once per accepted job, inside the
   * loop, because each job has its own prompt id and the queue ignores events
   * for ids it does not know.
   *
   * Sequential `await`, never `Promise.all`: one stdio transport, and
   * `register` stamps `submittedAt = Date.now()` which `queueRows` sorts by.
   * Parallel submits would interleave and list the queue in an order ComfyUI
   * will not run it in.
   */
  submit: async () => {
    if (!isTauri() || get().busy) return

    const { profileId, model, values, seedPinned } = useParamPanelStore.getState()
    if (blockers(profileId, model, values).length > 0) return
    if (profileId === null || model === null) return

    const stack = useLoraPanelStore.getState().stack
    const doc = useLyricsStore.getState().doc
    const { count } = get()
    // The override when the user typed one, else the selected document's title.
    // `specFor` normalises it (empty -> untitled).
    const title = get().title ?? doc?.title ?? null

    set({ busy: true, error: null, queued: 0 })
    try {
      const specs = specsFor(profileId, model, values, stack, doc, title, count, freshSeed, seedPinned)
      // Keep the screen truthful: the seed that ran is the seed shown. A fresh
      // Generate re-rolls an unpinned seed, and the field must not keep showing
      // the value that was replaced (MCP-SURFACE 20.2's "whatever runs is on
      // screen").
      const name = seedControl(model)
      if (name !== null && specs.length > 0) {
        const first = specs[0].inputs[name]
        if (first !== undefined && first.type === 'seed') {
          useParamPanelStore.getState().setSeed(first.value)
        }
      }
      for (const spec of specs) {
        const submission = await generateAudio(spec)
        useJobsStore.getState().register(submission.prompt_id, profileId)
        set({ last: submission, lastProfileId: profileId, queued: get().queued + 1 })
      }
    } catch (e) {
      // Verbatim. The param panel once shipped a note with comfy-cli's raw
      // transport error spliced into the middle of a sentence, and it took
      // somebody reading the screen to find it while every test passed.
      // Do not clear `last`: a partial batch really is running on the GPU.
      set({ error: String(e) })
    } finally {
      set({ busy: false })
    }
  },

  setCount: (n) => set({ count: n }),

  // Any keystroke is an override, `''` included -- an empty field means the user
  // wants this generation untitled, not "fall back to the document".
  setTitle: (title) => set({ title }),

  /** Fill the lyrics field from the approved version. */
  useApprovedLyric: () => {
    const { model, setValue } = useParamPanelStore.getState()
    const fill = approvedFill(useLyricsStore.getState().doc, model)
    if (fill === null) return
    setValue(fill.name, fill.text)
  },
}))
