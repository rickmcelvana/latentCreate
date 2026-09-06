import { USE_APPROVED, approvedOffer } from '../state/generate'
import { useGenerateStore } from '../state/generatePanel'
import { useLyricsStore } from '../state/lyrics'
import { useParamPanelStore } from '../state/paramPanel'

/**
 * "The Lyrics Studio has vN approved. Use it" -- rendered beside the **Lyrics**
 * field's own label, not down at the Generate button.
 *
 * It offers to fill one specific field, so it belongs at that field. Sitting
 * above Generate it read as a note about generating, and a user looking at an
 * empty Lyrics box had no reason to scroll past it to find the fix.
 *
 * Renders nothing when there is nothing to offer -- no approved version, a
 * model that takes no lyrics, or a field that already holds exactly that text.
 */
export function ApprovedLyricOffer() {
  const model = useParamPanelStore((s) => s.model)
  const values = useParamPanelStore((s) => s.values)
  const doc = useLyricsStore((s) => s.doc)
  const useApprovedLyric = useGenerateStore((s) => s.useApprovedLyric)

  const offer = approvedOffer(doc, model, values)
  if (offer === null) return null

  return (
    <span className="lyric-offer">
      {offer}{' '}
      <button type="button" className="lyric-offer-use" onClick={useApprovedLyric}>
        {USE_APPROVED}
      </button>
    </span>
  )
}
