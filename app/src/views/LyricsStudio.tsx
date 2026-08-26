import { useEffect, useState } from 'react'
import { useConfigStore } from '../state/config'
import {
  generationPhase,
  structureOptions,
  thinkingTail,
  useLyricsStore,
  type GenerationPhase,
} from '../state/lyrics'
import { getProfileGuide, DEFAULT_PROFILE_ID, type ProfileGuide } from '../bridge/profiles'
import { isTauri, type PointOfView } from '../bridge/lyrics'

const POINTS_OF_VIEW: PointOfView[] = ['first_person', 'second_person', 'third_person']

const STATUS_LABELS: Record<GenerationPhase, string> = {
  idle: 'Idle',
  starting: 'Starting...',
  thinking: 'Thinking...',
  writing: 'Writing...',
  failed: 'Failed',
}

/**
 * LyricsStudio: the brief form.
 *
 * Every field binds to the store's `brief`, so the form is a thin projection of
 * the store and the store is what generate sends. The style-tags prefill comes
 * from the selected profile's own worked example, not a constant -- the two
 * shipped profiles disagree about what a style tag even is.
 */
export function LyricsStudio() {
  const startListening = useLyricsStore((state) => state.startListening)
  const brief = useLyricsStore((state) => state.brief)
  const setBrief = useLyricsStore((state) => state.setBrief)
  const prefillFrom = useLyricsStore((state) => state.prefillFrom)
  const generate = useLyricsStore((state) => state.generate)
  const generating = useLyricsStore((state) => state.generating)
  const configured = useConfigStore((state) => state.config?.default_profile_id ?? null)
  const profileId = configured ?? DEFAULT_PROFILE_ID
  const [guide, setGuide] = useState<ProfileGuide | null>(null)

  useEffect(() => {
    void startListening()
  }, [startListening])

  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    void getProfileGuide(profileId).then((result) => {
      if (cancelled) return
      setGuide(result)
      prefillFrom(result)
    })
    return () => {
      cancelled = true
    }
  }, [profileId, prefillFrom])

  return (
    <>
      <h1 className="view-title">Lyrics</h1>
      <p className="view-subtitle">
        {guide !== null
          ? `Writing for ${guide.display_name}. Describe the song, then generate.`
          : 'Describe the song; your local model writes the words.'}
      </p>

      <form
        className="panel lyrics-form"
        onSubmit={(event) => {
          event.preventDefault()
          void generate(profileId)
        }}
      >
        <label className="lyrics-field">
          <span className="lyrics-label">Theme</span>
          <textarea
            className="lyrics-input lyrics-textarea"
            value={brief.theme}
            onChange={(event) => setBrief({ theme: event.target.value })}
          />
        </label>

        <label className="lyrics-field">
          <span className="lyrics-label">Style tags</span>
          <input
            className="lyrics-input"
            value={brief.style_tags}
            onChange={(event) => setBrief({ style_tags: event.target.value })}
          />
          <span className="lyrics-hint">{guide?.tag_style ?? 'Comma-separated short tags.'}</span>
        </label>

        <div className="lyrics-row">
          <label className="lyrics-field">
            <span className="lyrics-label">Mood</span>
            <input
              className="lyrics-input"
              value={brief.mood}
              onChange={(event) => setBrief({ mood: event.target.value })}
            />
          </label>

          <label className="lyrics-field">
            <span className="lyrics-label">Structure</span>
            <select
              className="lyrics-input lyrics-select"
              value={brief.structure}
              onChange={(event) => setBrief({ structure: event.target.value })}
            >
              {structureOptions(brief.structure).map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="lyrics-row">
          <label className="lyrics-field">
            <span className="lyrics-label">Language</span>
            <input
              className="lyrics-input"
              value={brief.language}
              onChange={(event) => setBrief({ language: event.target.value })}
            />
          </label>

          <label className="lyrics-field">
            <span className="lyrics-label">Point of view</span>
            <select
              className="lyrics-input lyrics-select"
              value={brief.point_of_view}
              onChange={(event) => setBrief({ point_of_view: event.target.value as PointOfView })}
            >
              {POINTS_OF_VIEW.map((option) => (
                <option key={option} value={option}>
                  {option.replace('_', ' ')}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="lyrics-row">
          <label className="lyrics-field">
            <span className="lyrics-label">Era / references</span>
            <input
              className="lyrics-input"
              value={brief.era_refs ?? ''}
              onChange={(event) =>
                setBrief({ era_refs: event.target.value === '' ? null : event.target.value })
              }
            />
          </label>

          <label className="lyrics-field">
            <span className="lyrics-label">Length (seconds)</span>
            <input
              className="lyrics-input"
              type="number"
              min={1}
              value={brief.target_duration_s}
              onChange={(event) =>
                setBrief({ target_duration_s: Math.max(1, event.target.valueAsNumber || 1) })
              }
            />
          </label>
        </div>

        <label className="lyrics-check">
          <input
            type="checkbox"
            checked={brief.explicit_allowed}
            onChange={(event) => setBrief({ explicit_allowed: event.target.checked })}
          />
          Allow explicit language
        </label>

        <div className="lyrics-actions">
          <button type="submit" className="setup-button setup-button-primary" disabled={generating}>
            {generating ? 'Generating...' : 'Generate'}
          </button>
        </div>
      </form>

      <LyricOutput />
    </>
  )
}

/**
 * The generation itself: the streaming draft, the thinking status, and the
 * terminal banner.
 *
 * A generation that streams nothing visible for tens of seconds is
 * indistinguishable from a hang, so the reasoning is shown as status -- the
 * content the model is already sending is the fix, not a spinner
 * (LLM-SURFACE 12).
 */
function LyricOutput() {
  const draft = useLyricsStore((state) => state.draft)
  const thinking = useLyricsStore((state) => state.thinking)
  const truncated = useLyricsStore((state) => state.truncated)
  const generating = useLyricsStore((state) => state.generating)
  const error = useLyricsStore((state) => state.error)
  const cancel = useLyricsStore((state) => state.cancel)

  const phase = generationPhase({ draft, thinking, truncated, generating, error })
  if (phase === 'idle') return null

  return (
    <section className="panel lyrics-output">
      <header className="lyrics-output-head">
        <span className={`lyrics-status lyrics-status-${phase}`}>{STATUS_LABELS[phase]}</span>
        {generating ? (
          <button type="button" className="job-cancel" onClick={() => void cancel()}>
            Cancel
          </button>
        ) : null}
      </header>

      {phase === 'thinking' ? <p className="lyrics-thinking">{thinkingTail(thinking)}</p> : null}

      {phase === 'failed' ? <p className="lyrics-error">{error}</p> : null}

      {draft !== '' ? <pre className="lyrics-draft">{draft}</pre> : null}

      {truncated && !generating ? (
        <p className="lyrics-truncation">
          The model ran out of room and stopped early. Try a longer length, then generate again.
        </p>
      ) : null}
    </section>
  )
}
