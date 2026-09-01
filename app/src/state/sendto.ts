import { create } from 'zustand'
import { sendTo, type SendTarget } from '../bridge/sendto'

/** The order the two destinations are offered in. */
export const SEND_TARGETS: readonly SendTarget[] = ['mixing', 'mastering']

/** What each destination is called on screen. */
export const SEND_TARGET_NAMES: Record<SendTarget, string> = {
  mixing: 'Mixing',
  mastering: 'Mastering',
}

/** The last send failure, remembered with the track it belongs to. */
export interface SendFailure {
  trackId: string
  message: string
}

/**
 * The message to show under one track's row, or `null`.
 *
 * A failure belongs to the row that produced it. Showing the last error under
 * every row is the absent-versus-empty confusion this repo has paid for four
 * times, landing in the one place a user is about to click something that
 * touches their files.
 */
export function failureFor(failure: SendFailure | null, trackId: string): string | null {
  if (failure === null) return null
  return failure.trackId === trackId ? failure.message : null
}

/** True only for the row whose send is in flight. */
export function isSending(sending: string | null, trackId: string): boolean {
  return sending === trackId
}

interface SendToState {
  /** The track currently being sent, or `null`. */
  sending: string | null
  failure: SendFailure | null
  send: (trackId: string, target: SendTarget) => Promise<void>
}

export const useSendToStore = create<SendToState>((set) => ({
  sending: null,
  failure: null,

  send: async (trackId, target) => {
    set({ sending: trackId, failure: null })
    try {
      await sendTo(trackId, target)
      set({ sending: null })
    } catch (err: unknown) {
      // Tauri rejects a `Result<(), String>` with the bare string, not an
      // `Error`; `state/player.ts` narrows the same way for the same reason.
      const message = err instanceof Error ? err.message : String(err)
      set({ sending: null, failure: { trackId, message } })
    }
  },
}))
