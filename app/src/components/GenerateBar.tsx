import {
  BATCH_CHOICES,
  GENERATE,
  USE_APPROVED,
  approvedOffer,
  blockers,
  canBatch,
  effectiveCount,
  notesFor,
  queueingLabel,
} from '../state/generate'
import { useGenerateStore } from '../state/generatePanel'
import { useLyricsStore } from '../state/lyrics'
import { useParamPanelStore } from '../state/paramPanel'

export function GenerateBar() {
  const busy = useGenerateStore((s) => s.busy)
  const error = useGenerateStore((s) => s.error)
  const last = useGenerateStore((s) => s.last)
  const lastProfileId = useGenerateStore((s) => s.lastProfileId)
  const count = useGenerateStore((s) => s.count)
  const queued = useGenerateStore((s) => s.queued)
  const titleOverride = useGenerateStore((s) => s.title)
  const submit = useGenerateStore((s) => s.submit)
  const setCount = useGenerateStore((s) => s.setCount)
  const setTitle = useGenerateStore((s) => s.setTitle)
  const useApprovedLyric = useGenerateStore((s) => s.useApprovedLyric)

  const profileId = useParamPanelStore((s) => s.profileId)
  const model = useParamPanelStore((s) => s.model)
  const values = useParamPanelStore((s) => s.values)
  const doc = useLyricsStore((s) => s.doc)

  const offer = approvedOffer(doc, model, values)
  const reasons = blockers(profileId, model, values)
  const notes = notesFor(last, lastProfileId, profileId, queued)

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

      <label className="generate-title">
        <span className="generate-title-label">Title</span>
        <input
          type="text"
          className="generate-title-input"
          placeholder="Untitled — names the track and its exported file"
          value={titleOverride ?? doc?.title ?? ''}
          disabled={busy}
          onChange={(e) => setTitle(e.target.value)}
        />
      </label>

      <div className="generate-actions">
        {canBatch(model) ? (
          <label className="generate-count">
            Variations
            <select
              value={count}
              disabled={busy}
              onChange={(e) => setCount(Number(e.target.value))}
            >
              {BATCH_CHOICES.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
        ) : null}

        <button
          type="button"
          className="generate-button"
          disabled={reasons.length > 0 || busy}
          onClick={() => void submit()}
        >
          {busy ? queueingLabel(queued, effectiveCount(model, count)) : GENERATE}
        </button>
      </div>

      {error !== null ? <p className="generate-error">{error}</p> : null}

      {notes.map((note) => (
        <p className="generate-note" key={note}>
          {note}
        </p>
      ))}
    </section>
  )
}
