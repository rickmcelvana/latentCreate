import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import { generateImage, type Submission } from '../bridge/generate'
import { blockers, seedControl, specsFor } from './generate'
import { useJobsStore } from './jobs'
import { freshSeed, useArtPanelStore } from './paramPanel'

/**
 * Pressing Generate in Cover Art: assemble, submit, and hand the job to the queue.
 *
 * This deliberately does not share `state/generatePanel.ts`: that store reads a
 * LoRA panel, a lyric document and a nav store that Cover Art has none of, and
 * threading a "kind" through it would put four unused branches in the one place
 * a wrong branch means a wrong track.
 */
interface ArtGenerateState {
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
  /** The artwork title. Free text; `cleanTitle` normalises empty to untitled. */
  title: string | null
  submit: () => Promise<void>
  setCount: (n: number) => void
  setTitle: (title: string) => void
}

export const useGenerateArtStore = create<ArtGenerateState>((set, get) => ({
  busy: false,
  error: null,
  last: null,
  lastProfileId: null,
  count: 1,
  queued: 0,
  title: null,

  /**
   * Queue one or more cover-art generations.
   *
   * The `blockers` check is repeated here even though the button is disabled
   * on it. That is not belt-and-braces: the button is a view, and this is the
   * layer that decides what reaches Rust.
   *
   * Four differences from the Audio Studio's `submit`:
   * 1. It reads `useArtPanelStore`, the independent Cover Art panel.
   * 2. It passes `specsFor` an empty LoRA stack and no lyric document. An
   *    adopted image profile declares no `loras` block and no `lyrics_contract`,
   *    so both are genuinely absent rather than omitted; passing them empty
   *    reuses the one spec assembler instead of forking it.
   * 3. The title is the field alone. The Audio Studio falls back to the
   *    selected lyric document's title; Cover Art has no document, so the
   *    field goes straight to `specsFor`, which normalises it through
   *    `cleanTitle`.
   * 4. It calls `generateImage`.
   *
   * Everything else is carried over verbatim, because each line is a rule this
   * project paid for: `register` per accepted job, sequential `await`, writing
   * the first spec's seed back with `setSeed`, verbatim errors, and not
   * clearing `last` on failure.
   */
  submit: async () => {
    if (!isTauri() || get().busy) return

    const { profileId, model, values, seedPinned } = useArtPanelStore.getState()
    if (blockers(profileId, model, values).length > 0) return
    if (profileId === null || model === null) return

    const { count } = get()
    const title = get().title

    set({ busy: true, error: null, queued: 0 })
    try {
      const specs = specsFor(profileId, model, values, [], null, title, count, freshSeed, seedPinned)
      // Keep the screen truthful: the seed that ran is the seed shown.
      const name = seedControl(model)
      if (name !== null && specs.length > 0) {
        const first = specs[0].inputs[name]
        if (first !== undefined && first.type === 'seed') {
          useArtPanelStore.getState().setSeed(first.value)
        }
      }
      for (const spec of specs) {
        const submission = await generateImage(spec)
        useJobsStore.getState().register(submission.prompt_id, profileId)
        set({ last: submission, lastProfileId: profileId, queued: get().queued + 1 })
      }
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ busy: false })
    }
  },

  setCount: (n) => set({ count: n }),

  setTitle: (title) => set({ title }),
}))
