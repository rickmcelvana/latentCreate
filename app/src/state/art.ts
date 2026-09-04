import { create } from 'zustand'
import {
  artImageUrl,
  listArt,
  subscribeArt,
  type ArtSet,
  type ArtWarning,
  type Artwork,
} from '../bridge/art'
import { isTauri } from '../bridge/jobs'
import { createdDate, modelLabel, seedText } from './provenance'

/** Shown in place of the grid when nothing has been generated yet. */
export const EMPTY_ART =
  'Cover art you generate will appear here, with the recipe that made it.'

/** One tile of the gallery, with every decision already made. */
export interface ArtRow {
  id: string
  /** The user's title, else the id -- never empty. */
  name: string
  model: string
  license: string
  /** `768 x 768`, or `--` when the header could not be read. */
  size: string
  created: string
  seed: string
  promptId: string | null
  file: string
  /**
   * The asset URL, or `null` when it could not be resolved. `null` is a tile
   * that says so; it is not a reason to drop the artwork from the gallery.
   */
  url: string | null
}

function artName(art: Artwork): string {
  const title = art.title?.trim()
  return title !== undefined && title !== '' ? title : art.id
}

function formatSize(width: number | null, height: number | null): string {
  if (width !== null && height !== null) return `${width} x ${height}`
  return '--'
}

/** Map an `ArtSet` and the resolved URLs to the tiles Cover Art will render. */
export function artRows(set: ArtSet, urls: Record<string, string>): ArtRow[] {
  return set.art.map((art) => ({
    id: art.id,
    name: artName(art),
    model: modelLabel(art.provenance),
    license: art.provenance.model_license,
    size: formatSize(art.width, art.height),
    created: createdDate(art.provenance),
    seed: seedText(art.provenance),
    promptId: art.provenance.prompt_id,
    file: art.file,
    url: urls[art.id] ?? null,
  }))
}

/** A single sentence describing warnings, or `null`. Never a modal. */
export function artWarningLine(warnings: ArtWarning[]): string | null {
  if (warnings.length === 0) return null
  const count = warnings.length
  const noun = count === 1 ? 'artwork sidecar' : 'artwork sidecars'
  return `${count} ${noun} could not be read; check the files in your project's art folder.`
}

interface ArtState {
  art: ArtRow[]
  /** The raw artworks by id, for the provenance inspector and T-506e's attach. */
  byId: Record<string, Artwork>
  warnings: string | null
  loading: boolean
  error: string | null
  listening: boolean
  load: () => Promise<void>
  startListening: () => Promise<void>
}

export const useArtStore = create<ArtState>((set, get) => ({
  art: [],
  byId: {},
  warnings: null,
  loading: false,
  error: null,
  listening: false,

  load: async () => {
    set({ loading: true, error: null })
    try {
      const artSet = await listArt()
      const urlMap: Record<string, string> = {}
      await Promise.all(
        artSet.art.map(async (art) => {
          try {
            urlMap[art.id] = await artImageUrl(art.id)
          } catch {
            // One failure leaves this tile's URL null; the rest still resolve.
          }
        }),
      )
      set({
        art: artRows(artSet, urlMap),
        byId: Object.fromEntries(artSet.art.map((art) => [art.id, art])),
        warnings: artWarningLine(artSet.warnings),
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
    await subscribeArt(() => {
      void get().load()
    })
  },
}))
