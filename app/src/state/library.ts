import { create } from 'zustand'
import {
  listTracks,
  subscribeTracks,
  type Track,
  type TrackSet,
  type TrackWarning,
} from '../bridge/library'
import { isTauri } from '../bridge/jobs'
import type { InputValue } from './params'

/** Shown in place of the list when nothing has been generated yet. */
export const EMPTY_LIBRARY =
  'Tracks you generate will appear here, with the recipe that made them.'

/** One row of the Library, with every decision already made. */
export interface TrackRow {
  id: string
  /** The user's title, else the id -- never empty. */
  name: string
  model: string
  license: string
  duration: string
  created: string
  loras: string
  seed: string
  /** `null` when the track predates T-311d, which is not an error. */
  promptId: string | null
  file: string
}

/**
 * What a row calls the model that produced it.
 *
 * Deliberate twin of `state/queue.ts` `modelName`: the input is different
 * (a track, not a job), but the absent-versus-empty rule and the fallback
 * chain are the same.
 */
function trackModel(track: Track): string {
  const display = track.provenance.profile_display_name.trim()
  if (display !== '') return display
  const id = track.provenance.profile_id.trim()
  if (id !== '') return id
  return 'Unknown model'
}

function trackName(track: Track): string {
  const title = track.title?.trim()
  return title !== undefined && title !== '' ? title : track.id
}

function formatDuration(seconds: number | null): string {
  if (seconds === null) return '--'
  const total = Math.floor(seconds)
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}

function loraStack(track: Track): string {
  return track.provenance.spec.loras
    .filter((lora) => lora.enabled)
    .map((lora) => {
      const segment = lora.file.split(/[\\/]/).pop() ?? lora.file
      const stem = segment.replace(/\.safetensors$/i, '')
      return `${stem} x${lora.strength.toFixed(1)}`
    })
    .join(', ')
}

function seedValue(track: Track): string {
  const value = track.provenance.spec.inputs.seed
  if (value && value.type === 'seed') return String(value.value)
  return '--'
}

function createdDate(track: Track): string {
  return track.provenance.created_at.split('T')[0]
}

/** Map a `TrackSet` to the rows the Library will render. */
export function trackRows(set: TrackSet): TrackRow[] {
  return set.tracks.map((track) => ({
    id: track.id,
    name: trackName(track),
    model: trackModel(track),
    license: track.provenance.model_license,
    duration: formatDuration(track.duration_s),
    created: createdDate(track),
    loras: loraStack(track),
    seed: seedValue(track),
    promptId: track.provenance.prompt_id,
    file: track.file,
  }))
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
export function provenanceView(track: Track): ProvenanceSection[] {
  const p = track.provenance
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

/**
 * A single sentence describing warnings, or `null` when there are none.
 *
 * Never a modal: the sidecars are files the user can inspect.
 */
export function warningLine(warnings: TrackWarning[]): string | null {
  if (warnings.length === 0) return null
  const count = warnings.length
  const noun = count === 1 ? 'sidecar' : 'sidecars'
  return `${count} track ${noun} could not be read; check the files in your project's tracks folder.`
}

interface LibraryState {
  tracks: TrackRow[]
  /**
   * The raw tracks by id, kept so the provenance inspector and "re-use these
   * settings" (T-406) can read the full sidecar the flattened `TrackRow` drops.
   */
  byId: Record<string, Track>
  warnings: string | null
  loading: boolean
  error: string | null
  listening: boolean
  load: () => Promise<void>
  startListening: () => Promise<void>
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  tracks: [],
  byId: {},
  warnings: null,
  loading: false,
  error: null,
  listening: false,

  load: async () => {
    set({ loading: true, error: null })
    try {
      const trackSet = await listTracks()
      set({
        tracks: trackRows(trackSet),
        byId: Object.fromEntries(trackSet.tracks.map((track) => [track.id, track])),
        warnings: warningLine(trackSet.warnings),
        loading: false,
      })
    } catch (err) {
      set({
        error: err instanceof Error ? err.message : String(err),
        loading: false,
      })
    }
  },

  startListening: async () => {
    if (get().listening) return
    if (!isTauri()) return
    set({ listening: true })
    await subscribeTracks(() => {
      void get().load()
    })
  },
}))
