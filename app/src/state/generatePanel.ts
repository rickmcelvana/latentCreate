import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import { generateAudio, type Submission } from '../bridge/generate'
import { approvedFill, blockers, specFor } from './generate'
import { useJobsStore } from './jobs'
import { useLoraPanelStore } from './loraPanel'
import { useLyricsStore } from './lyrics'
import { useParamPanelStore } from './paramPanel'

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
  submit: () => Promise<void>
  useApprovedLyric: () => void
}

export const useGenerateStore = create<GenerateState>((set, get) => ({
  busy: false,
  error: null,
  last: null,
  lastProfileId: null,

  /**
   * Queue one generation.
   *
   * The `blockers` check is repeated here even though the button is disabled
   * on it. That is not belt-and-braces: the button is a view, and this is the
   * layer that decides what reaches Rust -- the same reason `add` refuses a
   * duplicate the picker never offers.
   *
   * **`register` is the load-bearing line.** `generate_audio` starts the job
   * pump itself, and without this the queue panel never hears about the job it
   * is running (jobs.ts).
   */
  submit: async () => {
    if (!isTauri() || get().busy) return

    const { profileId, model, values } = useParamPanelStore.getState()
    if (blockers(profileId, model, values).length > 0) return
    if (profileId === null || model === null) return

    const stack = useLoraPanelStore.getState().stack
    const doc = useLyricsStore.getState().doc

    set({ busy: true, error: null })
    try {
      const submission = await generateAudio(specFor(profileId, model, values, stack, doc))
      useJobsStore.getState().register(submission.prompt_id, profileId)
      set({ last: submission, lastProfileId: profileId })
    } catch (e) {
      // Verbatim. The param panel once shipped a note with comfy-cli's raw
      // transport error spliced into the middle of a sentence, and it took
      // somebody reading the screen to find it while every test passed.
      set({ error: String(e), last: null, lastProfileId: null })
    } finally {
      set({ busy: false })
    }
  },

  /** Fill the lyrics field from the approved version. */
  useApprovedLyric: () => {
    const { model, setValue } = useParamPanelStore.getState()
    const fill = approvedFill(useLyricsStore.getState().doc, model)
    if (fill === null) return
    setValue(fill.name, fill.text)
  },
}))
