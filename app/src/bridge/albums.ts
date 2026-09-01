import { invoke } from '@tauri-apps/api/core'
import type { AlbumList } from './projects'

/** List every album in the selected project. */
export async function listAlbums(): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('albums_list')
}

/** Create an album. Returns the project's refreshed album list. */
export async function createAlbum(name: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_create', { name })
}

/** Rename an album. Returns the refreshed album list. */
export async function renameAlbum(from: string, to: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_rename', { from, to })
}

/** Delete an album. Returns the refreshed album list. */
export async function deleteAlbum(name: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_delete', { name })
}

/** Add a track to an album. Returns the refreshed album list. */
export async function addAlbumTrack(album: string, trackId: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_add_track', { album, trackId })
}

/** Remove a track from an album. Returns the refreshed album list. */
export async function removeAlbumTrack(album: string, trackId: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_remove_track', { album, trackId })
}

/** Set an album's full track order. Returns the refreshed album list. */
export async function reorderAlbum(album: string, trackIds: string[]): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_reorder', { album, trackIds })
}
