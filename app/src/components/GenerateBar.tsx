import {
  GENERATE,
  QUEUEING,
  USE_APPROVED,
  approvedOffer,
  blockers,
  notesFor,
} from '../state/generate'
import { useGenerateStore } from '../state/generatePanel'
import { useLyricsStore } from '../state/lyrics'
import { useParamPanelStore } from '../state/paramPanel'

export function GenerateBar() {
  const busy = useGenerateStore((s) => s.busy)
  const error = useGenerateStore((s) => s.error)
  const last = useGenerateStore((s) => s.last)
  const lastProfileId = useGenerateStore((s) => s.lastProfileId)
  const submit = useGenerateStore((s) => s.submit)
  const useApprovedLyric = useGenerateStore((s) => s.useApprovedLyric)

  const profileId = useParamPanelStore((s) => s.profileId)
  const model = useParamPanelStore((s) => s.model)
  const values = useParamPanelStore((s) => s.values)
  const doc = useLyricsStore((s) => s.doc)

  const offer = approvedOffer(doc, model, values)
  const reasons = blockers(profileId, model, values)
  const notes = notesFor(last, lastProfileId, profileId)

  return (
    <section className="panel generate-bar">
      {offer !== null ? (
        <p className="generate-lyric-offer">
          {offer}{' '}
          <button
            type="button"
            className="generate-use-lyric"
            onClick={useApprovedLyric}
          >
            {USE_APPROVED}
          </button>
        </p>
      ) : null}

      {reasons.map((reason) => (
        <p className="generate-blocked" key={reason}>
          {reason}
        </p>
      ))}

      <button
        type="button"
        className="generate-button"
        disabled={reasons.length > 0 || busy}
        onClick={() => void submit()}
      >
        {busy ? QUEUEING : GENERATE}
      </button>

      {error !== null ? <p className="generate-error">{error}</p> : null}

      {notes.map((note) => (
        <p className="generate-note" key={note}>
          {note}
        </p>
      ))}
    </section>
  )
}
