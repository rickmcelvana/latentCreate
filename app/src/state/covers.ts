import type { AlbumList } from '../bridge/projects'
import type { ArtRow } from './art'
import type { TrackRow } from './library'

/** What a row shows where its cover goes. */
export type CoverView =
  | { state: 'none' }
  /**
   * The row names an artwork the gallery does not have. Rendered as missing,
   * never as an error -- the T-403 rule, and a state T-506e-a can really leave
   * behind: clearing covers on delete is N atomic writes, not one transaction,
   * so a crash part-way leaves a track naming a deleted artwork.
   */
  | { state: 'missing'; id: string }
  | { state: 'shown'; id: string; name: string; url: string | null }

export function coverView(coverId: string | null, art: ArtRow[]): CoverView {
  if (coverId === null) return { state: 'none' }
  const row = art.find((a) => a.id === coverId)
  if (row === undefined) return { state: 'missing', id: coverId }
  return { state: 'shown', id: coverId, name: row.name, url: row.url }
}

/** How many tracks and albums use this artwork as their cover. */
export function coverUsage(
  artId: string,
  tracks: TrackRow[],
  albums: AlbumList[],
): { tracks: number; albums: number } {
  return {
    tracks: tracks.filter((track) => track.cover === artId).length,
    albums: albums.filter((album) => album.cover === artId).length,
  }
}

/**
 * The delete confirm, as one sentence plus the specifics when they are known.
 *
 * The rule is stated unconditionally -- the image and its record go to the
 * Recycle Bin, and anything using it as a cover loses it -- because it is true
 * whether or not the library has loaded. The counts are appended only when
 * there are any, so a view that has not loaded the tracks understates nothing;
 * it simply says less.
 */
export function deleteArtPrompt(name: string, usage: { tracks: number; albums: number }): string {
  let prompt =
    `Delete “${name}”? The image and its record go to the Recycle Bin, and anything using it as a cover loses it.`
  const parts: string[] = []
  if (usage.tracks > 0) {
    parts.push(`${usage.tracks} track${usage.tracks === 1 ? '' : 's'}`)
  }
  if (usage.albums > 0) {
    parts.push(`${usage.albums} album${usage.albums === 1 ? '' : 's'}`)
  }
  if (parts.length > 0) {
    prompt += ` It is the cover for ${parts.join(' and ')}.`
  }
  return prompt
}

/**
 * The options a cover picker offers: "No cover" first, then every artwork in
 * gallery order. `id: null` is the clear.
 */
export function coverChoices(art: ArtRow[]): { id: string | null; label: string }[] {
  return [{ id: null, label: 'No cover' }, ...art.map((row) => ({ id: row.id, label: row.name }))]
}
