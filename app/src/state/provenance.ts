import type { Provenance } from '../bridge/library'
import type { InputValue } from './params'

/**
 * What a row calls the model that produced it.
 *
 * Named `modelLabel` rather than `modelName` because `state/queue.ts` has a
 * `modelName` with the same fallback chain and a different input -- a queued
 * job has no provenance yet, only a profile id. Two names, so a reader can tell
 * which side of the run they are on.
 */
export function modelLabel(p: Provenance): string {
  const display = p.profile_display_name.trim()
  if (display !== '') return display
  const id = p.profile_id.trim()
  if (id !== '') return id
  return 'Unknown model'
}

/** The seed as text, or `--`. `inputs.seed` is the tagged value, not a number. */
export function seedText(p: Provenance): string {
  const value = p.spec.inputs.seed
  if (value && value.type === 'seed') return String(value.value)
  return '--'
}

/** The date half of the RFC 3339 stamp. Never parsed into a `Date`. */
export function createdDate(p: Provenance): string {
  return p.created_at.split('T')[0]
}

/** One labelled fact in the provenance inspector. */
export interface ProvenanceFact {
  label: string
  value: string
}

/** One titled group of facts in the inspector; omitted entirely when empty. */
export interface ProvenanceSection {
  title: string
  facts: ProvenanceFact[]
}

/**
 * One tagged value as a string. **`v.value`, never `String(v)`** -- an
 * `InputValue` is `{ type, value }`, so stringifying the wrapper prints
 * `[object Object]`. A seed reads as its number, text/enum/int/float as theirs.
 */
function formatValue(v: InputValue): string {
  return String(v.value)
}

/**
 * The full sidecar as inspector sections (T-406): every semantic input, the
 * lyric ref, the resolved slots ComfyUI received, and the server that ran it.
 * The Library card already shows the summary (model/licence/seed/LoRAs/run);
 * this is the rest. A section with nothing in it is omitted, so an older sidecar
 * with no `prompt_id`, no resolved slots and no comfy info still renders cleanly.
 */
export function provenanceView(p: Provenance): ProvenanceSection[] {
  const sections: ProvenanceSection[] = []

  const inputs = Object.entries(p.spec.inputs).map(([label, v]) => ({ label, value: formatValue(v) }))
  if (inputs.length > 0) sections.push({ title: 'Inputs', facts: inputs })

  if (p.spec.lyrics !== null) {
    const ref = p.spec.lyrics
    sections.push({
      title: 'Lyrics',
      facts: [{ label: 'Document', value: `${ref.doc_id}, v${ref.version}` }],
    })
  }

  const slots = Object.entries(p.resolved_slots).map(([label, v]) => ({ label, value: formatValue(v) }))
  if (slots.length > 0) sections.push({ title: 'Resolved slots', facts: slots })

  const server: ProvenanceFact[] = []
  if (p.comfy?.comfyui_version) server.push({ label: 'ComfyUI', value: p.comfy.comfyui_version })
  if (p.comfy?.comfy_cli_version) server.push({ label: 'comfy-cli', value: p.comfy.comfy_cli_version })
  if (p.comfy?.url) server.push({ label: 'Endpoint', value: p.comfy.url })
  if (p.template !== null) server.push({ label: 'Template', value: p.template })
  if (server.length > 0) sections.push({ title: 'Server', facts: server })

  return sections
}
