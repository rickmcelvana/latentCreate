import type { GenerationSpec, LoraRef, LyricRef, Submission } from '../bridge/generate'
import type { LyricDoc } from '../bridge/lyricdoc'
import { specLoras, type StackRow } from './loras'
import { approvedText } from './lyrics'
import {
  seedError,
  specInputs,
  type ControlValue,
  type InputValue,
  type PanelModel,
} from './params'

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

/** How many variations one click can queue. */
export const MAX_BATCH = 8

/** The choices the Variations select offers. */
export const BATCH_CHOICES = [1, 2, 4, 8] as const

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
export function seedControl(model: PanelModel): string | null {
  const control = [...model.basic, ...model.advanced].find((c) => c.kind === 'seed')
  return control?.name ?? null
}

/**
 * Whether this model can be batched at all: only a seed makes two jobs differ.
 *
 * A model with no seed control would queue N identical specs, and their
 * sidecars could not be told apart -- so the UI does not offer the control.
 */
export function canBatch(model: PanelModel | null): boolean {
  if (model === null) return false
  return seedControl(model) !== null
}

/**
 * How many jobs a click will actually queue.
 *
 * One rule, one owner. The count survives a profile switch, so a 4 chosen on
 * ACE-Step is still 4 when the panel shows a model with no seed -- where the
 * control is not even on screen and only one job can be queued. Without this
 * the button would count `1 of 4` through a batch of one.
 */
export function effectiveCount(model: PanelModel | null, count: number): number {
  if (!canBatch(model)) return 1
  return Math.min(Math.max(Math.trunc(count), 1), MAX_BATCH)
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

/**
 * The song title for a spec: trimmed, and empty or whitespace-only becomes
 * `null` (an untitled track, which the Library renders as the id). The Audio
 * Studio title is free text, so it is normalised here, at the one place the
 * spec is built, rather than trusting the field.
 */
export function cleanTitle(title: string | null): string | null {
  const trimmed = (title ?? '').trim()
  return trimmed === '' ? null : trimmed
}

/** Everything one generation needs, assembled from the two panels. */
export function specFor(
  profileId: string,
  model: PanelModel,
  values: Record<string, ControlValue>,
  stack: StackRow[],
  doc: LyricDoc | null,
  title: string | null,
): GenerationSpec {
  return {
    profile_id: profileId,
    inputs: specInputs(model, values),
    loras: specLoras(stack),
    lyrics: lyricRefFor(doc, model, values),
    // The title the user named at generation (T-409). The caller resolves it --
    // the Audio Studio override when the user typed one, else the selected
    // document's own title -- and this normalises it (empty -> untitled).
    title: cleanTitle(title),
  }
}

/**
 * The specs for one click: the first exactly as `specFor` builds it, the rest
 * identical but for a fresh seed.
 *
 * `pinned` is whether the user deliberately chose the seed (typed it, or hit
 * Reroll). When it is false the seed is the panel's auto-rolled default, and a
 * fresh Generate must re-roll it -- otherwise two clicks with nothing changed
 * submit the same seed, ComfyUI answers `execution_cached` in 0 s, and the app
 * files a byte-identical duplicate track (MCP-SURFACE 30.6, T-316). When it is
 * true the first spec keeps the seed on screen, and only the variations after
 * it get fresh ones.
 *
 * `nextSeed` is a parameter with no default. `freshSeed` lives in `paramPanel.ts`,
 * which imports zustand, and this module is pure -- importing a store here to get
 * a random number would pull the store graph into the one file that has none.
 */
export function specsFor(
  profileId: string,
  model: PanelModel,
  values: Record<string, ControlValue>,
  stack: StackRow[],
  doc: LyricDoc | null,
  title: string | null,
  count: number,
  nextSeed: () => number,
  pinned: boolean,
): GenerationSpec[] {
  const name = seedControl(model)
  const total = effectiveCount(model, count)
  // One title across the batch: the seed varies per variation (below), the
  // title does not -- five variations are five takes of the same song.
  if (name === null) return [specFor(profileId, model, values, stack, doc, title)]

  const firstValues = pinned ? values : { ...values, [name]: nextSeed() }
  const specs = [specFor(profileId, model, firstValues, stack, doc, title)]
  for (let i = 1; i < total; i++) {
    specs.push(specFor(profileId, model, { ...values, [name]: nextSeed() }, stack, doc, title))
  }
  return specs
}

/**
 * A spec's tagged inputs back to raw panel values -- the inverse of `specInputs`
 * (T-406 "re-use these settings"). Each `InputValue.value` is already a
 * `ControlValue` (a string or a number), so this just unwraps the tag.
 */
export function controlValues(inputs: Record<string, InputValue>): Record<string, ControlValue> {
  const values: Record<string, ControlValue> = {}
  for (const [name, input] of Object.entries(inputs)) {
    // A `ControlValue` is a string or a number; the panel has no boolean control
    // (`specInputs` emits none), so a bool input has no field to land in.
    if (typeof input.value === 'boolean') continue
    values[name] = input.value
  }
  return values
}

/**
 * A spec's LoRA refs back to stack rows -- the inverse of `specLoras`. The label
 * is derived from the file (the stem), the same way the Library summary renders
 * it; a re-used LoRA the current catalog no longer offers stays in the stack and
 * is reported by `missingFrom`, exactly as any stale row is.
 */
export function stackFromLoras(loras: LoraRef[]): StackRow[] {
  return loras.map((lora) => ({
    path: lora.file,
    label: (lora.file.split(/[\\/]/).pop() ?? lora.file).replace(/\.safetensors$/i, ''),
    strength: lora.strength,
    enabled: lora.enabled,
  }))
}

/** The button's label while a batch is in flight. */
export function queueingLabel(queued: number, total: number): string {
  if (total <= 1) return QUEUEING
  return `Queueing ${queued + 1} of ${total}…`
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
export function submissionNotes(submission: Submission, queued: number = 1): string[] {
  const notes = [
    queued <= 1 ? QUEUED : `Queued ${queued}. Watch them in the queue below.`,
  ]

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
 *
 * **`queued === 0` clears them**, and that guard is what makes T-312's "do not
 * clear `last` on a failure" safe. A partial batch must keep its notes -- two
 * jobs really are on the GPU -- but a click where *nothing* was accepted must
 * not inherit the last successful submission's, or killing ComfyUI and pressing
 * Generate again shows the transport error and `Queued.` in the same breath.
 */
export function notesFor(
  last: Submission | null,
  lastProfileId: string | null,
  profileId: string | null,
  queued: number = 1,
): string[] {
  if (last === null || lastProfileId === null || lastProfileId !== profileId) return []
  if (queued === 0) return []
  return submissionNotes(last, queued)
}
