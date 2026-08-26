import { useMemo, useState } from 'react'
import {
  hasChanges,
  originalSpans,
  revisedSpans,
  wordDiff,
  type DiffSpan,
} from './wordDiff'

/**
 * The consent gate for any prompt this app proposes rewriting.
 *
 * Deliberately knows nothing about lyrics: it takes two strings and three
 * callbacks, so Phase 3's audio tags reuse it as-is (ARCHITECTURE 6 names both
 * uses). The caller owns the texts -- this component never applies anything,
 * which is the whole point of it existing.
 *
 * Accept / Edit / Revert are the three actions. Edit turns the proposal into a
 * textarea, because a rewrite the user is 90 percent happy with should not have
 * to be rejected wholesale.
 */
export interface PromptDiffProps {
  /** The text as it stands today. Never edited here. */
  original: string
  /** The proposed replacement, as the caller currently holds it. */
  revised: string
  /** Called as the user edits the proposal. */
  onRevisedChange: (text: string) => void
  /** Called when the user accepts the proposal as it now reads. */
  onAccept: () => void
  /** Called when the user throws the proposal away. */
  onRevert: () => void
  /** Heading for the left pane. Defaults to "Original". */
  originalLabel?: string
  /** Heading for the right pane. Defaults to "Optimized". */
  revisedLabel?: string
  /** An extra advisory, e.g. that the rewrite was cut off. */
  note?: string | null
}

export function PromptDiff({
  original,
  revised,
  onRevisedChange,
  onAccept,
  onRevert,
  originalLabel = 'Original',
  revisedLabel = 'Optimized',
  note = null,
}: PromptDiffProps) {
  const [editing, setEditing] = useState(false)
  const spans = useMemo(() => wordDiff(original, revised), [original, revised])
  const changed = hasChanges(spans)
  const acceptable = revised.trim() !== ''

  return (
    <section className="panel prompt-diff">
      <header className="prompt-diff-head">
        <h2 className="prompt-diff-title">Review the rewritten prompt</h2>
        <button
          type="button"
          className="setup-button"
          onClick={() => setEditing((was) => !was)}
        >
          {editing ? 'Done editing' : 'Edit'}
        </button>
      </header>

      <p className="prompt-diff-lede">
        Nothing is sent until you accept. Accept uses the text on the right; Revert keeps your
        own.
      </p>

      <div className="prompt-diff-panes">
        <div className="prompt-diff-pane">
          <span className="prompt-diff-pane-label">{originalLabel}</span>
          <pre className="prompt-diff-text">
            <Spans spans={originalSpans(spans)} />
          </pre>
        </div>

        <div className="prompt-diff-pane">
          <span className="prompt-diff-pane-label">{revisedLabel}</span>
          {editing ? (
            <textarea
              className="prompt-diff-edit"
              value={revised}
              onChange={(event) => onRevisedChange(event.target.value)}
            />
          ) : (
            <pre className="prompt-diff-text">
              <Spans spans={revisedSpans(spans)} />
            </pre>
          )}
        </div>
      </div>

      {!changed ? (
        <p className="prompt-diff-note">The model returned your prompt unchanged.</p>
      ) : null}
      {note !== null ? <p className="prompt-diff-note">{note}</p> : null}

      <div className="prompt-diff-actions">
        <button
          type="button"
          className="setup-button setup-button-primary"
          onClick={onAccept}
          disabled={!acceptable}
        >
          Accept
        </button>
        <button type="button" className="setup-button" onClick={onRevert}>
          Revert
        </button>
      </div>
    </section>
  )
}

/** One pane's text, with the changed runs marked. */
function Spans({ spans }: { spans: DiffSpan[] }) {
  return (
    <>
      {spans.map((span, index) =>
        span.kind === 'same' ? (
          <span key={index}>{span.text}</span>
        ) : (
          <mark key={index} className={`prompt-diff-${span.kind}`}>
            {span.text}
          </mark>
        ),
      )}
    </>
  )
}
