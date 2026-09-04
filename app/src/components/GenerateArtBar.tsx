import {
  BATCH_CHOICES,
  GENERATE,
  blockers,
  canBatch,
  effectiveCount,
  notesFor,
  queueingLabel,
} from '../state/generate'
import { useGenerateArtStore } from '../state/artGenerate'
import { useArtPanelStore } from '../state/paramPanel'

export function GenerateArtBar() {
  const busy = useGenerateArtStore((s) => s.busy)
  const error = useGenerateArtStore((s) => s.error)
  const last = useGenerateArtStore((s) => s.last)
  const lastProfileId = useGenerateArtStore((s) => s.lastProfileId)
  const count = useGenerateArtStore((s) => s.count)
  const queued = useGenerateArtStore((s) => s.queued)
  const title = useGenerateArtStore((s) => s.title)
  const submit = useGenerateArtStore((s) => s.submit)
  const setCount = useGenerateArtStore((s) => s.setCount)
  const setTitle = useGenerateArtStore((s) => s.setTitle)

  const profileId = useArtPanelStore((s) => s.profileId)
  const model = useArtPanelStore((s) => s.model)
  const values = useArtPanelStore((s) => s.values)

  const reasons = blockers(profileId, model, values)
  const notes = notesFor(last, lastProfileId, profileId, queued)

  return (
    <section className="panel generate-bar">
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
          placeholder="Untitled — names the artwork"
          value={title ?? ''}
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
