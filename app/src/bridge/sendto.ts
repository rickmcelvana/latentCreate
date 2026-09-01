import { invoke } from '@tauri-apps/api/core'

/** Mirrors Rust `sendto::SendTarget`. These are the wire words, not labels. */
export type SendTarget = 'mixing' | 'mastering'

/**
 * Open the sibling app and reveal this track's audio file for drag-in.
 *
 * Rejects with the backend's own sentence -- the copy lives in Rust
 * (`src-tauri/src/sendto.rs`) so every surface says the same thing.
 */
export async function sendTo(id: string, target: SendTarget): Promise<void> {
  await invoke('send_to', { id, target })
}
