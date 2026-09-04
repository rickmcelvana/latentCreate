import { create } from 'zustand'
import {
  listTracks,
  subscribeTracks,
  type Track,
  type TrackSet,
  type TrackWarning,
} from '../bridge/library'
import { isTauri } from '../bridge/jobs'
import {
  createdDate,
  modelLabel,
  seedText,
  provenanceView,
  type ProvenanceFact,
  type ProvenanceSection,
} from './provenance'

export type { ProvenanceFact, ProvenanceSection }
export { provenanceView }

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

/** Map a `TrackSet` to the rows the Library will render. */
export function trackRows(set: TrackSet): TrackRow[] {
  return set.tracks.map((track) => ({
    id: track.id,
    name: trackName(track),
    model: modelLabel(track.provenance),
    license: track.provenance.model_license,
    duration: formatDuration(track.duration_s),
    created: createdDate(track.provenance),
    loras: loraStack(track),
    seed: seedText(track.provenance),
    promptId: track.provenance.prompt_id,
    file: track.file,
  }))
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
