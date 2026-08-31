import { convertFileSrc, invoke } from '@tauri-apps/api/core'

/**
 * Resolve a track id to a URL the webview can play.
 *
 * The backend returns an absolute path (T-402a's `track_audio_path`, which
 * validates the id and the stored file); `convertFileSrc` turns that into an
 * `asset://localhost/...` (or `http://asset.localhost/...`) URL the asset
 * protocol serves. Both halves stay here: the store and components import this
 * wrapper, never `@tauri-apps/*`.
 */
export async function trackAudioUrl(id: string): Promise<string> {
  const absolute = await invoke<string>('track_audio_path', { id })
  return convertFileSrc(absolute)
}
