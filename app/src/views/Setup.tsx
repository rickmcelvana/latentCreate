import { useEffect, useRef, useState } from 'react'
import type { ComfyStatus } from '../bridge/comfy'
import { useComfyStore, formatVram, pillFor } from '../state/comfy'
import type { ProfileStatus } from '../bridge/models'
import { curatedFirst, formatBytes, installView, useModelsStore } from '../state/models'
import { ImportWorkflow } from '../components/ImportWorkflow'
import { ProfilePickerRow } from '../components/ProfilePickerRow'
import { ProjectPickerRow } from '../components/ProjectPickerRow'
import {
  effectiveImageProfileId,
  effectiveProfileId,
  profileRow,
} from '../state/profiles'
import { effectiveProjectSlug, projectRow, useProjectsStore } from '../state/projects'
import {
  canTest,
  DEFAULT_BASE_URL,
  effectiveBaseUrl,
  keyField,
  modelView,
  testSummary,
  useLlmStore,
} from '../state/llm'
import { useConfigStore } from '../state/config'

/**
 * Setup wizard, ComfyUI step.
 *
 * Checks once on mount and otherwise only when the user asks. Nothing here
 * polls: a wizard that re-probes on a timer spawns `comfy-mcp` processes
 * behind the user's back.
 */
export function Setup() {
  const status = useComfyStore((state) => state.status)
  const busy = useComfyStore((state) => state.busy)
  const refresh = useComfyStore((state) => state.refresh)
  const launch = useComfyStore((state) => state.launch)

  useEffect(() => {
    void refresh()
  }, [refresh])

  const pill = pillFor(status)

  return (
    <>
      <h1 className="view-title">Setup</h1>
      <p className="view-subtitle">
        Connect ComfyUI and, optionally, a model for writing lyrics.
      </p>

      <section className="panel setup-step">
        <header className="setup-step-head">
          <h2 className="setup-step-title">ComfyUI</h2>
          <span className={`status-pill status-pill-${pill.tone}`}>{pill.label}</span>
        </header>

        {pill.nextStep !== null ? <p className="setup-next-step">{pill.nextStep}</p> : null}

        {status !== null && status.state === 'not_installed' ? (
          <code className="setup-command">{status.install_command}</code>
        ) : null}

        {status !== null && status.state === 'ready' ? (
          <ComfyFacts status={status} />
        ) : null}

        <div className="setup-actions">
          <button type="button" className="setup-button" onClick={() => void refresh()} disabled={busy}>
            {busy ? 'Checking...' : 'Retry'}
          </button>
          {status !== null && status.state === 'server_down' ? (
            <button
              type="button"
              className="setup-button setup-button-primary"
              onClick={() => void launch()}
              disabled={busy}
            >
              Start ComfyUI
            </button>
          ) : null}
        </div>
      </section>

      <ModelsStep kind="music" />
      <ModelsStep kind="image" />
      <ImportStep />
      <LlmStep />
      <ProjectsStep />
      <ReleaseStep />
    </>
  )
}

/**
 * Setup wizard, lyric-LLM step.
 *
 * Probes once on mount and otherwise only when the user asks. The probe is
 * also the step's only keychain read -- `has_key` rides on the status, so
 * nothing here calls `has_secret`, whose answer requires reading the secret
 * and on macOS can raise a prompt (T-004).
 *
 * The endpoint and API key are editable here. The key is write-only: it is
 * stored through the config store and never read back into the input.
 */
function LlmStep() {
  const status = useLlmStore((state) => state.status)
  const busy = useLlmStore((state) => state.busy)
  const testing = useLlmStore((state) => state.testing)
  const result = useLlmStore((state) => state.result)
  const model = useLlmStore((state) => state.model)
  const probe = useLlmStore((state) => state.probe)
  const choose = useLlmStore((state) => state.choose)
  const test = useLlmStore((state) => state.test)
  const saveEndpoint = useLlmStore((state) => state.saveEndpoint)
  const config = useConfigStore((state) => state.config)
  const configStatus = useConfigStore((state) => state.status)
  const configuredModel = useConfigStore((state) => state.config?.llm?.model ?? null)
  const storeSecret = useConfigStore((state) => state.storeSecret)
  const removeSecret = useConfigStore((state) => state.removeSecret)
  const effective = effectiveBaseUrl(config)
  const [draftUrl, setDraftUrl] = useState(effective)
  const [draftKey, setDraftKey] = useState('')
  const probed = useRef(false)

  useEffect(() => {
    setDraftUrl(effective)
  }, [effective])

  useEffect(() => {
    // Probe once, and not before config has been read. Passing null while it
    // loads would throw away the model the user already configured, which is
    // the whole of what the backend's `preselect` now protects. Re-probing
    // whenever config changes is the other wrong answer: `probe` resets
    // `model` from `preselect`, so it would stomp the selection the user just
    // made and saved.
    if (probed.current || configStatus === 'idle' || configStatus === 'loading') return
    probed.current = true
    void probe(effective, configuredModel)
  }, [probe, configStatus, configuredModel, effective])

  // Blank means "use the default", not "no endpoint": the prefill is the app's
  // baseline and there is no useful state where the step points at nothing.
  // Storing null is what lets `effectiveBaseUrl` resolve it back to the
  // default, so the field and the probe never disagree about the address.
  const applyEndpoint = async () => {
    const trimmed = draftUrl.trim()
    const next = trimmed === '' ? DEFAULT_BASE_URL : trimmed
    await saveEndpoint(trimmed === '' ? null : trimmed)
    void probe(next, configuredModel)
  }

  const storeKey = async () => {
    await storeSecret('llm_api_key', draftKey)
    setDraftKey('')
    void probe(effective, model)
  }

  const removeKey = async () => {
    await removeSecret('llm_api_key')
    void probe(effective, model)
  }

  const keyView = keyField(status)

  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">Lyrics model</h2>
        <button
          type="button"
          className="setup-button"
          onClick={() => void probe(effective, model)}
          disabled={busy}
        >
          {busy ? 'Checking...' : 'Retry'}
        </button>
      </header>

      <div className="llm-endpoint-row">
        <input
          type="text"
          className="lyrics-input llm-endpoint"
          value={draftUrl}
          onChange={(e) => setDraftUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              void applyEndpoint()
            }
          }}
          placeholder="OpenAI-compatible endpoint URL"
          aria-label="Endpoint address"
        />
        <button
          type="button"
          className="setup-button setup-button-primary"
          onClick={() => void applyEndpoint()}
          disabled={busy}
        >
          Connect
        </button>
      </div>

      <div className="llm-key-row">
        {keyView === 'stored' ? (
          <>
            <span className="status-pill status-pill-ok">API key stored</span>
            <button type="button" className="setup-button" onClick={() => void removeKey()}>
              Remove
            </button>
          </>
        ) : (
          <>
            <input
              type="password"
              className="lyrics-input llm-key"
              value={draftKey}
              onChange={(e) => setDraftKey(e.target.value)}
              placeholder="API key (optional)"
              aria-label="API key"
            />
            <button
              type="button"
              className="setup-button"
              onClick={() => void storeKey()}
              disabled={draftKey === ''}
            >
              Store key
            </button>
          </>
        )}
      </div>

      {/* Rendered for type-completeness only. The step always resolves to a
          non-empty endpoint -- a blank field means the default -- so nothing
          in this wizard can currently produce `not_configured`. Kept because
          the backend variant exists; do not put guidance a user needs here. */}
      {status !== null && status.state === 'not_configured' ? (
        <p className="setup-next-step">No endpoint set.</p>
      ) : null}

      {status !== null && status.state === 'unreachable' ? (
        <>
          <p className="setup-next-step">{status.detail}</p>
          {status.hint !== null ? <p className="setup-next-step">{status.hint}</p> : null}
          <p className="setup-next-step">
            Lyrics are written by a model you provide. latentCreate works with any OpenAI-compatible
            endpoint -- a local server, or a hosted API with a key.
          </p>
          <p className="setup-next-step">
            Tried <code className="setup-command">{effective}</code>.
          </p>
        </>
      ) : null}

      {status !== null && status.state === 'ready' ? (
        <>
          {/* Said once, plainly: without Ollama's native API neither the
              capability nor the privacy question can be answered at all. */}
          {!status.enriched ? (
            <p className="setup-next-step">
              This endpoint does not report model capabilities, so it cannot be checked whether a
              model runs locally or can write lyrics at all.
            </p>
          ) : null}

          <ul className="llm-models">
            {status.models.map((row) => {
              const view = modelView(row)
              return (
                <li key={view.id} className="llm-model">
                  <label className="llm-model-pick">
                    <input
                      type="radio"
                      name="lyric-model"
                      value={view.id}
                      checked={model === view.id}
                      disabled={!view.selectable}
                      onChange={() => void choose(effective, view.id)}
                    />
                    <code>{view.id}</code>
                  </label>
                  {view.chips.length > 0 ? (
                    <span className="llm-chips">
                      {view.chips.map((chip) => (
                        <span key={chip} className="llm-chip">
                          {chip}
                        </span>
                      ))}
                    </span>
                  ) : null}
                  {view.disclosure !== null ? (
                    <p className="llm-disclosure">{view.disclosure}</p>
                  ) : null}
                </li>
              )
            })}
          </ul>

          <div className="setup-actions">
            <button
              type="button"
              className="setup-button setup-button-primary"
              onClick={() => void test(effective)}
              disabled={!canTest(status, model) || testing}
            >
              {testing ? 'Testing...' : 'Test call'}
            </button>
          </div>

          {result !== null ? <p className="setup-next-step">{testSummary(result)}</p> : null}
        </>
      ) : null}
    </section>
  )
}

/** The copy that differs between the two model steps. Kept as data so the two
 *  cards are one component: a music card and an image card that drifted apart
 *  is exactly what this session's rework existed to undo. */
const MODEL_STEPS = {
  music: {
    title: 'Music models',
    blurb: 'Install a model, and pick the one the Audio studio starts with.',
    group: 'setup-music',
  },
  image: {
    title: 'Image models',
    blurb: 'Install a model, and pick the one Cover Art starts with.',
    group: 'setup-image',
  },
} as const

/**
 * Setup wizard, models step -- one card per kind.
 *
 * Readiness is decided by comparing each profile's declared files against what
 * ComfyUI reports it has -- never by `local_check.runnable`, which answers a
 * different question and calls a working MiniMax install unrunnable over a
 * filename the profile already corrects.
 *
 * The step also owns the **default** for its studio: the quick-swap menus on
 * Audio and Cover Art offer installed models only, so what is picked here is
 * what a screen with nothing installed still reads.
 */
function ModelsStep({ kind }: { kind: ProfileStatus['kind'] }) {
  const view = useModelsStore((state) => state.view)
  const busy = useModelsStore((state) => state.busy)
  const refresh = useModelsStore((state) => state.refresh)
  const config = useConfigStore((state) => state.config)
  const save = useConfigStore((state) => state.save)

  useEffect(() => {
    void refresh()
  }, [refresh])

  const step = MODEL_STEPS[kind]
  const profiles = view === null ? [] : curatedFirst(view.profiles).filter((p) => p.kind === kind)
  const chosen = kind === 'music' ? effectiveProfileId(config) : effectiveImageProfileId(config)

  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">{step.title}</h2>
        <button type="button" className="setup-button" onClick={() => void refresh()} disabled={busy}>
          {busy ? 'Checking...' : 'Retry'}
        </button>
      </header>

      <p className="setup-next-step">{step.blurb}</p>

      {/* Said on both cards rather than once above them: the cards are
          separate now, and a warning that only appears on the first is a
          warning half the readers never see. */}
      {view !== null && !view.inventory_available ? (
        <p className="setup-next-step">
          Cannot see which models are installed. {view.inventory_detail ?? 'Start ComfyUI above.'}
        </p>
      ) : null}

      {profiles.length > 0 ? (
        <ul className="picker-list">
          {profiles.map((p) => (
            <ModelRow
              key={p.id}
              profile={p}
              group={step.group}
              selected={p.id === chosen}
              onSelect={() =>
                void save(
                  kind === 'music'
                    ? { default_profile_id: p.id }
                    : { default_image_profile_id: p.id },
                )
              }
            />
          ))}
        </ul>
      ) : null}
    </section>
  )
}

/**
 * Setup wizard, import step.
 *
 * Its own card under the two model lists: importing a workflow is how a model
 * that is *not* on the curated list gets in (ARCHITECTURE 5b), which is a
 * different act from installing one that is -- and it was reading as a footer
 * to whichever list it sat inside.
 */
function ImportStep() {
  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">Your own workflow</h2>
      </header>

      <p className="setup-next-step">
        For anything not on the lists above: import a ComfyUI workflow and map its inputs.
      </p>

      <ImportWorkflow />
    </section>
  )
}

/** One model: pick it as the default, read its licence, and install it. */
function ModelRow({
  profile,
  group,
  selected,
  onSelect,
}: {
  profile: ProfileStatus
  group: string
  selected: boolean
  onSelect: () => void
}) {
  const install = useModelsStore((state) => state.install)
  const installing = useModelsStore((state) => state.installing)
  const progress = useModelsStore((state) => state.progress)

  const row = profileRow(profile)
  const active = installing === profile.id
  const live = active ? installView(progress) : null

  return (
    <ProfilePickerRow row={row} selected={selected} group={group} onSelect={onSelect}>
      {row.readiness.nextStep !== null && !active ? (
        <p className="setup-next-step">{row.readiness.nextStep}</p>
      ) : null}

      {live !== null ? (
        <p className="setup-next-step">
          Downloading {live.done} of {live.total} files
          {live.percent === null ? '' : ` -- ${live.percent}%`}
          {live.failed.length > 0 ? ` -- ${live.failed.length} failed` : ''}
        </p>
      ) : null}

      {profile.readiness.state === 'missing' ? (
        <ul className="model-files">
          {profile.readiness.files.map((file) => (
            <li key={`${file.folder}/${file.file}`}>
              <code>{file.file}</code>
              <span className="model-file-folder">
                {file.folder}
                {formatBytes(file.size_bytes) === null ? '' : ` -- ${formatBytes(file.size_bytes)}`}
              </span>
            </li>
          ))}
        </ul>
      ) : null}

      {profile.readiness.state === 'missing' && profile.readiness.installable ? (
        <div className="setup-actions">
          <button
            type="button"
            className="setup-button setup-button-primary"
            onClick={() => void install(profile.id)}
            disabled={installing !== null}
          >
            {active ? 'Downloading...' : 'Install'}
          </button>
        </div>
      ) : null}
    </ProfilePickerRow>
  )
}

/**
 * Setup wizard, release-details step.
 *
 * The artist is the **only** tag an exported track needs that the app cannot
 * work out for itself: title comes from the track, album and track number from
 * the album list it sits in, the year from its own creation stamp, and the
 * artwork from its cover. So this step is one field, and grows only if another
 * such fact turns up.
 *
 * Saved on blur and on Enter rather than on every keystroke -- `save` writes
 * `config.json` -- and read back from config on every render, so what is on
 * screen is what an export will use.
 */
function ReleaseStep() {
  const configured = useConfigStore((state) => state.config?.export?.artist ?? null)
  const save = useConfigStore((state) => state.save)
  const [draft, setDraft] = useState(configured ?? '')

  // Follow config once it loads, and after any save. Keyed on the stored value,
  // so typing is never interrupted by a re-render.
  useEffect(() => {
    setDraft(configured ?? '')
  }, [configured])

  // Blank means "no artist", which is a real answer -- an empty ARTIST tag is
  // not written at all (create-core's `export::present`).
  const commit = () => {
    const trimmed = draft.trim()
    if (trimmed === (configured ?? '')) return
    void save({ export: { artist: trimmed === '' ? null : trimmed } })
  }

  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">Release details</h2>
      </header>

      <p className="setup-next-step">
        Written into the files you export, along with the title, album, track number, year and
        cover the app already knows.
      </p>

      <label className="release-field">
        <span className="release-field-label">Artist</span>
        <input
          type="text"
          className="release-field-input"
          value={draft}
          placeholder="The name your releases go out under"
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              commit()
            }
          }}
        />
      </label>
    </section>
  )
}

/**
 * Setup wizard, projects step.
 *
 * A project is the folder every track, lyric, album and cover files into, so
 * creating and deleting them belongs with the rest of setup. The Library keeps
 * a quick-swap menu for changing which one is open, and nothing else.
 */
function ProjectsStep() {
  const config = useConfigStore((state) => state.config)
  const projects = useProjectsStore((state) => state.projects)
  const error = useProjectsStore((state) => state.error)
  const warnings = useProjectsStore((state) => state.warnings)
  const load = useProjectsStore((state) => state.load)
  const select = useProjectsStore((state) => state.select)

  useEffect(() => {
    void load()
  }, [load])

  const selected = effectiveProjectSlug(config, projects)

  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">Projects</h2>
      </header>

      <p className="setup-next-step">
        Every track, lyric, album and cover files into the open project.
      </p>

      {error !== null ? <p className="library-error">{error}</p> : null}
      {warnings !== null ? <p className="library-warning">{warnings}</p> : null}

      <ul className="picker-list">
        {projects.map(projectRow).map((row) => (
          <ProjectPickerRow
            key={row.slug}
            row={row}
            selected={row.slug === selected}
            onSelect={() => void select(row.slug)}
          />
        ))}
      </ul>

      <ProjectCreate />
    </section>
  )
}

function ProjectCreate() {
  const [name, setName] = useState('')
  const create = useProjectsStore((state) => state.create)
  return (
    <form
      className="project-create"
      onSubmit={(event) => {
        event.preventDefault()
        void create(name).then((ok) => {
          if (ok) setName('')
        })
      }}
    >
      <input
        className="project-create-input"
        type="text"
        value={name}
        placeholder="New project name"
        onChange={(event) => setName(event.target.value)}
      />
      <button type="submit" className="project-create-button" disabled={name.trim() === ''}>
        Create
      </button>
    </form>
  )
}

/** The details worth showing once ComfyUI is up. */
function ComfyFacts({ status }: { status: Extract<ComfyStatus, { state: 'ready' }> }) {
  const vram = formatVram(status.vram_bytes)
  return (
    <dl className="setup-facts">
      {vram !== null ? (
        <div className="setup-fact">
          <dt>Hardware</dt>
          <dd>{vram}</dd>
        </div>
      ) : null}
      {status.workspace !== null ? (
        <div className="setup-fact">
          <dt>Workspace</dt>
          <dd>{status.workspace}</dd>
        </div>
      ) : null}
      {status.comfy_cli_version !== null ? (
        <div className="setup-fact">
          <dt>comfy-cli</dt>
          <dd>
            {status.comfy_cli_version}
            {status.update_available ? (
              <span className="setup-update">update available</span>
            ) : null}
          </dd>
        </div>
      ) : null}
    </dl>
  )
}
