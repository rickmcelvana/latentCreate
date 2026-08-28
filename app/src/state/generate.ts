import type { GenerationSpec, LyricRef, Submission } from '../bridge/generate'
import type { LyricDoc } from '../bridge/lyricdoc'
import { specLoras, type StackRow } from './loras'
import { approvedText } from './lyrics'
import { seedError, specInputs, type ControlValue, type PanelModel } from './params'

/**
 * Assembling one generation: what is sent, and why Generate is sometimes off.
 *
 * This is the first module in the phase whose defects are **wrong tracks**
 * rather than wrong screens. Everything it decides is therefore a pure function
 * with a test, and the button below it decides nothing.
 */

/** Shown once a job is queued. */
export const QUEUED = 'Queued. Watch it in the queue below.'

/** The button that fills the lyrics field from the approved version. */
export const USE_APPROVED = 'Use it'

/** The submit button, idle and in flight. */
export const GENERATE = 'Generate'
export const QUEUEING = 'Queueing…'

/**
 * The lyrics control this profile declares, or `null` when it has none.
 *
 * By **kind**, not by the name `"lyrics"`. A profile names its own inputs, and
 * a custom-imported workflow (ARCHITECTURE 5b) may well call this something
 * else; matching on a hardcoded key would silently stop attaching lyric
 * provenance for exactly those users, with nothing on screen to show for it.
 */
function lyricsControl(model: PanelModel): string | null {
  const control = [...model.basic, ...model.advanced].find((c) => c.kind === 'lyrics')
  return control?.name ?? null
}

/** The seed control this profile declares, or `null` when it has none. */
function seedControl(model: PanelModel): string | null {
  const control = [...model.basic, ...model.advanced].find((c) => c.kind === 'seed')
  return control?.name ?? null
}

/**
 * Which lyric document and version these words came from, if it can be proved.
 *
 * `GenerationSpec` carries the lyric **text** in `inputs` and a `LyricRef`
 * beside it, and nothing downstream reconciles the two -- so a ref naming v2
 * next to v3's words is a provenance record that is wrong in the one way
 * provenance must never be wrong. T-311's acceptance bar is that a run
 * reproduces *from the sidecar alone*, which that would quietly break.
 *
 * So the ref is attached only when the text being submitted **is** the approved
 * version's text, byte for byte. Someone who pastes the approved lyric and then
 * changes a word has a different lyric, and no ref describes it.
 */
export function lyricRefFor(
  doc: LyricDoc | null,
  model: PanelModel,
  values: Record<string, ControlValue>,
): LyricRef | null {
  if (doc === null || doc.approved === null) return null

  const approved = approvedText(doc)
  if (approved === null) return null

  const name = lyricsControl(model)
  if (name === null) return null

  const submitted = values[name]
  if (typeof submitted !== 'string' || submitted !== approved) return null

  return { doc_id: doc.id, version: doc.approved }
}

/**
 * The offer to fill the lyrics field from the approved version, or `null`.
 *
 * Absent when nothing is approved, when this model takes no lyrics, and when
 * the field already holds exactly that text -- offering to do something already
 * done reads as though the app has not noticed.
 */
export function approvedOffer(
  doc: LyricDoc | null,
  model: PanelModel | null,
  values: Record<string, ControlValue>,
): string | null {
  if (doc === null || doc.approved === null || model === null) return null

  const approved = approvedText(doc)
  const name = lyricsControl(model)
  if (approved === null || name === null) return null
  if (values[name] === approved) return null

  return `The Lyrics Studio has v${doc.approved} approved.`
}

/** The approved text, ready to drop into the lyrics field. */
export function approvedFill(
  doc: LyricDoc | null,
  model: PanelModel | null,
): { name: string; text: string } | null {
  if (doc === null || model === null) return null
  const approved = approvedText(doc)
  const name = lyricsControl(model)
  if (approved === null || name === null) return null
  return { name, text: approved }
}

/**
 * Why Generate is off, as sentences the user reads. Empty means go.
 *
 * **ComfyUI being disconnected is deliberately not here.** `generate_audio`
 * calls `ensure_connected`, which starts comfy-mcp itself, so refusing on a
 * disconnected state would leave the button dead on every cold start -- and if
 * ComfyUI itself is down, the command's own error is what says so, accurately,
 * rather than this guessing ahead of it.
 */
export function blockers(
  profileId: string | null,
  model: PanelModel | null,
  values: Record<string, ControlValue>,
): string[] {
  if (model === null) {
    return [
      profileId === null
        ? 'Pick a model profile above.'
        : `No profile answers to ${profileId}. Pick a model profile above.`,
    ]
  }

  const reasons: string[] = []
  const seed = seedControl(model)
  if (seed !== null) {
    // The third layer of one rule: the panel refuses the value, the text input
    // keeps the DOM from rounding it, and this refuses to submit it. Only this
    // layer decides what actually reaches Rust.
    const problem = seedError(String(values[seed] ?? ''))
    if (problem !== null) reasons.push(problem)
  }
  return reasons
}

/** Everything one generation needs, assembled from the two panels. */
export function specFor(
  profileId: string,
  model: PanelModel,
  values: Record<string, ControlValue>,
  stack: StackRow[],
  doc: LyricDoc | null,
): GenerationSpec {
  return {
    profile_id: profileId,
    inputs: specInputs(model, values),
    loras: specLoras(stack),
    lyrics: lyricRefFor(doc, model, values),
  }
}

/**
 * What to say about a job that was just queued.
 *
 * The LoRA and format lines are the only confirmation anyone gets that the two
 * graph edits took effect -- and they are exactly the two things
 * `validate_workflow` cannot vouch for: a LoRA chain feeding nothing validates
 * clean (MCP-SURFACE 17.1), and the MiniMax template ships the modern save node
 * already set to MP3 (16.3). The owner swaps that node by hand in every
 * workflow, so the app doing it silently is worth one line.
 *
 * The format line does **not** claim "lossless". `output_format` is whatever
 * the edit wrote, and a sentence that would become a lie if a profile ever
 * opted into something else is not worth the extra word.
 */
export function submissionNotes(submission: Submission): string[] {
  const notes = [QUEUED]

  const loras = submission.lora_nodes.length
  if (loras > 0) {
    notes.push(`${loras} ${loras === 1 ? 'LoRA' : 'LoRAs'} applied.`)
  }
  if (submission.output_format !== null) {
    notes.push(`Saving ${submission.output_format}.`)
  }

  const unchecked = submission.unchecked_slots
  if (unchecked.length > 0) {
    notes.push(
      `${unchecked.length} ${unchecked.length === 1 ? 'setting' : 'settings'} could not be checked against this workflow: ${unchecked.join(', ')}. They may not reach the model.`,
    )
  }
  return notes
}

/**
 * The notes for a submission, but only while they still describe what is on
 * screen.
 *
 * A submission's notes outlive the settings that produced them, and the most
 * misleading case is concrete: generate with two LoRAs on ACE-Step, switch to
 * MiniMax Music 3, and `2 LoRAs applied.` sits under a model that has no LoRA
 * support at all and no panel to show for it. The queue below still carries the
 * job, so nothing is lost by dropping the line.
 *
 * Keyed on the profile rather than cleared on mount: a view re-mounts on every
 * tab switch, and clearing there would wipe the notes for anyone who generated,
 * looked at their lyrics, and came back.
 */
export function notesFor(
  last: Submission | null,
  lastProfileId: string | null,
  profileId: string | null,
): string[] {
  if (last === null || lastProfileId === null || lastProfileId !== profileId) return []
  return submissionNotes(last)
}
