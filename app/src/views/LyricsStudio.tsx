import { useEffect, useState } from 'react'
import { useConfigStore } from '../state/config'
import {
  approvedLabel,
  approvedText,
  generationPhase,
  structureOptions,
  thinkingTail,
  useLyricsStore,
  type GenerationPhase,
} from '../state/lyrics'
import { getProfileGuide, DEFAULT_PROFILE_ID, type ProfileGuide } from '../bridge/profiles'
import { isTauri, type PointOfView } from '../bridge/lyrics'
import { PromptDiff } from '../components/PromptDiff'
import { lintSeverity, type LintFinding, type LyricSource, type LyricVersion } from '../bridge/lyricdoc'

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
  const optimize = useLyricsStore((state) => state.optimize)
  const optimizing = useLyricsStore((state) => state.optimizing)
  // One rewrite in play at a time: with a proposal on screen or a prompt
  // already accepted, the way to another rewrite is through Revert, so the
  // text being replaced is always the one the user can see.
  const reviewing = useLyricsStore((state) => state.optimization !== null)
  const accepted = useLyricsStore((state) => state.promptOverride !== null)
  const configured = useConfigStore((state) => state.config?.default_profile_id ?? null)
  const profileId = configured ?? DEFAULT_PROFILE_ID
  const [guide, setGuide] = useState<ProfileGuide | null>(null)

  useEffect(() => {
    void startListening()
  }, [startListening])

  useEffect(() => {
    void useLyricsStore.getState().loadDoc()
  }, [])

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
          <button
            type="button"
            className="setup-button"
            onClick={() => void optimize(profileId)}
            disabled={generating || optimizing || reviewing || accepted}
          >
            {optimizing ? 'Optimizing...' : 'Optimize prompt'}
          </button>
        </div>
      </form>

      <PromptReview />
      <LyricEditor profileId={profileId} />
    </>
  )
}

/**
 * The optimizer's two visible states: a rewrite awaiting review, and a prompt
 * already accepted.
 *
 * The accepted state is a banner rather than nothing, because an accepted
 * override changes what Generate sends and an invisible one would make the form
 * a liar. Editing any brief field clears it (see the store's `setBrief`).
 */
function PromptReview() {
  const optimization = useLyricsStore((state) => state.optimization)
  const proposed = useLyricsStore((state) => state.proposed)
  const promptOverride = useLyricsStore((state) => state.promptOverride)
  const setProposed = useLyricsStore((state) => state.setProposed)
  const acceptOptimized = useLyricsStore((state) => state.acceptOptimized)
  const revertOptimized = useLyricsStore((state) => state.revertOptimized)

  if (optimization !== null) {
    return (
      <PromptDiff
        original={optimization.original}
        revised={proposed}
        onRevisedChange={setProposed}
        onAccept={acceptOptimized}
        onRevert={revertOptimized}
        note={
          optimization.truncated
            ? 'The rewrite was cut off before it finished. Check the end of it, or revert.'
            : null
        }
      />
    )
  }

  if (promptOverride === null) return null

  return (
    <p className="lyrics-optimized">
      Generate will send your accepted prompt.{' '}
      <button type="button" className="lyrics-link" onClick={revertOptimized}>
        Use the brief instead
      </button>
    </p>
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
function LyricEditor({ profileId }: { profileId: string }) {
  const draft = useLyricsStore((state) => state.draft)
  const setDraft = useLyricsStore((state) => state.setDraft)
  const thinking = useLyricsStore((state) => state.thinking)
  const truncated = useLyricsStore((state) => state.truncated)
  const generating = useLyricsStore((state) => state.generating)
  const error = useLyricsStore((state) => state.error)
  const cancel = useLyricsStore((state) => state.cancel)
  const doc = useLyricsStore((state) => state.doc)
  const findings = useLyricsStore((state) => state.findings)
  const saveDraft = useLyricsStore((state) => state.saveDraft)
  const lint = useLyricsStore((state) => state.lint)
  const linted = useLyricsStore((state) => state.linted)

  const phase = generationPhase({ draft, thinking, truncated, generating, error })
  if (phase === 'idle' && doc === null) return null

  return (
    <section className="panel lyrics-output">
      <header className="lyrics-output-head">
        <span className={`lyrics-status lyrics-status-${phase}`}>{STATUS_LABELS[phase]}</span>
        {/* Approval is a property of the document, so it belongs in the panel
            header beside the generation phase -- and as the same status pill
            the rest of the app uses, not another line of small green prose. It
            was twice reported missing while rendering correctly further down. */}
        {approvedLabel(doc) !== null ? (
          <span className="status-pill status-pill-ok">{approvedLabel(doc)}</span>
        ) : null}
        {generating ? (
          <button type="button" className="job-cancel" onClick={() => void cancel()}>
            Cancel
          </button>
        ) : null}
      </header>

      {phase === 'thinking' ? <p className="lyrics-thinking">{thinkingTail(thinking)}</p> : null}

      {phase === 'failed' ? <p className="lyrics-error">{error}</p> : null}

      {doc !== null ? (
        <textarea
          className="lyrics-draft"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
        />
      ) : null}

      <div className="lyrics-actions">
        <button
          type="button"
          className="setup-button"
          onClick={() => void saveDraft()}
          disabled={generating || draft.trim() === ''}
        >
          Save
        </button>
        <button
          type="button"
          className="setup-button"
          onClick={() => void lint(profileId)}
          disabled={generating}
        >
          Check
        </button>
      </div>

      {/* The approval notice sits with the actions, not at the foot of the
          panel. It shipped below the version list, which on a lyric carrying
          eight lint advisories put it off the bottom of the screen -- the user
          saw the per-version badge and concluded the notice did not exist. */}
      {doc !== null && approvedText(doc) !== null ? (
        <p className="lyrics-approved">v{doc.approved} is approved and ready for audio.</p>
      ) : null}

      {truncated && !generating ? (
        <p className="lyrics-truncation">
          The model ran out of room and stopped early. Try a longer length, then generate again.
        </p>
      ) : null}

      {/* A check that finds nothing must say so. Rendering nothing leaves the
          user unable to tell a clean lyric from a button that did not fire --
          and with a well-behaved model, clean is the common case. */}
      {linted && findings.length === 0 ? (
        <p className="lyrics-clean">Checked: no structure problems found.</p>
      ) : null}

      {findings.length > 0 ? <Findings findings={findings} /> : null}

      {doc !== null && doc.versions.length > 0 ? <VersionList /> : null}
    </section>
  )
}

/** The lint findings, as advisories rather than blockers. */
function Findings({ findings }: { findings: LintFinding[] }) {
  return (
    <ul className="lyrics-findings">
      {findings.map((finding, index) => (
        <li
          key={`${finding.kind}-${index}`}
          className={`lyrics-finding lyrics-finding-${lintSeverity(finding)}`}
        >
          {findingText(finding)}
        </li>
      ))}
    </ul>
  )
}

/** The versions, with restore and approve. */
function VersionList() {
  const doc = useLyricsStore((state) => state.doc)
  const restore = useLyricsStore((state) => state.restore)
  const approve = useLyricsStore((state) => state.approve)

  return (
    <div className="lyrics-versions">
      <h2 className="lyrics-versions-title">Versions</h2>
      <ol className="lyrics-version-list">
        {doc?.versions.map((version) => (
          <VersionRow
            key={version.number}
            version={version}
            isApproved={doc.approved === version.number}
            onRestore={() => restore(version.number)}
            onApprove={() => void approve(version.number)}
          />
        ))}
      </ol>
    </div>
  )
}

function VersionRow({
  version,
  isApproved,
  onRestore,
  onApprove,
}: {
  version: LyricVersion
  isApproved: boolean
  onRestore: () => void
  onApprove: () => void
}) {
  return (
    <li className={`lyrics-version ${isApproved ? 'lyrics-version-approved' : ''}`}>
      <div className="lyrics-version-head">
        <span className="lyrics-version-number">v{version.number}</span>
        <span className="lyrics-version-source">{sourceLabel(version.source)}</span>
        {isApproved ? <span className="lyrics-version-approved-badge">approved</span> : null}
      </div>
      <p className="lyrics-version-preview">{preview(version.text)}</p>
      <div className="lyrics-version-actions">
        <button type="button" className="setup-button" onClick={onRestore}>
          Restore
        </button>
        {!isApproved ? (
          <button type="button" className="setup-button setup-button-primary" onClick={onApprove}>
            Approve
          </button>
        ) : null}
      </div>
    </li>
  )
}

function sourceLabel(source: LyricSource): string {
  switch (source.kind) {
    case 'human':
      return 'typed'
    case 'llm':
      return source.model === '' ? 'generated' : source.model
    case 'edited':
      return `edited from v${source.from_version}`
  }
}

function findingText(finding: LintFinding): string {
  switch (finding.kind) {
    case 'unknown_tag':
      return `Line ${finding.line}: "${finding.tag}" is not a structure tag.`
    case 'missing_section':
      return `Missing section [${finding.section}].`
    case 'out_of_order':
      return 'Sections are out of the requested order.'
    case 'extra_section':
      return `Line ${finding.line}: extra section "${finding.tag}".`
    case 'text_after_tag':
      return `Line ${finding.line}: text after a tag ("${finding.text}").`
    case 'no_structure_tags':
      return 'No structure tags found.'
  }
}

function preview(text: string): string {
  const first = text.split('\n').find((line) => line.trim() !== '') ?? ''
  return first.length > 60 ? `${first.slice(0, 60)}...` : first
}
