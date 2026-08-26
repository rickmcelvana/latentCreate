import { useEffect, useRef } from 'react'
import type { ComfyStatus } from '../bridge/comfy'
import { useComfyStore, formatVram, pillFor } from '../state/comfy'
import type { ProfileStatus } from '../bridge/models'
import { curatedFirst, formatBytes, installView, rowFor, useModelsStore } from '../state/models'
import { canTest, modelView, testSummary, useLlmStore } from '../state/llm'
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

      <ModelsStep />
      <LlmStep />
    </>
  )
}

/** Where the lyric LLM lives. Ollama's default, and the commonest case. */
const DEFAULT_BASE_URL = 'http://127.0.0.1:11434/v1'

/**
 * Setup wizard, lyric-LLM step.
 *
 * Probes once on mount and otherwise only when the user asks. The probe is
 * also the step's only keychain read -- `has_key` rides on the status, so
 * nothing here calls `has_secret`, whose answer requires reading the secret
 * and on macOS can raise a prompt (T-004).
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
  const configStatus = useConfigStore((state) => state.status)
  const configuredModel = useConfigStore((state) => state.config?.llm?.model ?? null)
  const probed = useRef(false)

  useEffect(() => {
    // Probe once, and not before config has been read. Passing null while it
    // loads would throw away the model the user already configured and
    // preselect a suggestion over it, which the backend's `preselect` exists
    // to prevent. Re-probing whenever config changes is the other wrong
    // answer: `probe` resets `model` from `preselect`, so it would stomp the
    // selection the user just made and saved.
    if (probed.current || configStatus === 'idle' || configStatus === 'loading') return
    probed.current = true
    void probe(DEFAULT_BASE_URL, configuredModel)
  }, [probe, configStatus, configuredModel])

  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">Lyrics model</h2>
        <button
          type="button"
          className="setup-button"
          onClick={() => void probe(DEFAULT_BASE_URL, model)}
          disabled={busy}
        >
          {busy ? 'Checking...' : 'Retry'}
        </button>
      </header>

      {status !== null && status.state === 'not_configured' ? (
        <p className="setup-next-step">Set an endpoint to write lyrics with a model.</p>
      ) : null}

      {status !== null && status.state === 'unreachable' ? (
        <>
          <p className="setup-next-step">{status.detail}</p>
          {status.hint !== null ? <p className="setup-next-step">{status.hint}</p> : null}
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
                      onChange={() => void choose(DEFAULT_BASE_URL, view.id)}
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

          {status.missing_suggestions.map((suggestion) => (
            <div key={suggestion.label} className="llm-suggestion">
              <p className="setup-next-step">
                {suggestion.label} is suggested for lyrics
                {suggestion.why === null ? '' : ` -- ${suggestion.why}`}
                {suggestion.vram_hint === null ? '' : ` Needs ${suggestion.vram_hint}.`}
              </p>
              {/* The command is shown, never run: this app does not pull an
                  LLM onto the user's disk (docs/MODELS.md). */}
              {suggestion.pull_command !== null ? (
                <code className="setup-command">{suggestion.pull_command}</code>
              ) : null}
            </div>
          ))}

          <div className="setup-actions">
            <button
              type="button"
              className="setup-button setup-button-primary"
              onClick={() => void test(DEFAULT_BASE_URL)}
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

/**
 * Setup wizard, models step.
 *
 * Readiness is decided by comparing each profile's declared files against what
 * ComfyUI reports it has -- never by `local_check.runnable`, which answers a
 * different question and calls a working MiniMax install unrunnable over a
 * filename the profile already corrects.
 */
function ModelsStep() {
  const view = useModelsStore((state) => state.view)
  const busy = useModelsStore((state) => state.busy)
  const refresh = useModelsStore((state) => state.refresh)

  useEffect(() => {
    void refresh()
  }, [refresh])

  const profiles = view === null ? [] : curatedFirst(view.profiles)

  return (
    <section className="panel setup-step">
      <header className="setup-step-head">
        <h2 className="setup-step-title">Models</h2>
        <button type="button" className="setup-button" onClick={() => void refresh()} disabled={busy}>
          {busy ? 'Checking...' : 'Retry'}
        </button>
      </header>

      {view !== null && !view.inventory_available ? (
        <p className="setup-next-step">
          Cannot see which models are installed. {view.inventory_detail ?? 'Start ComfyUI above.'}
        </p>
      ) : null}

      {profiles.map((profile) => (
        <ModelRow key={profile.id} profile={profile} />
      ))}
    </section>
  )
}

/** One model, its licence, and whether it can be used. */
function ModelRow({ profile }: { profile: ProfileStatus }) {
  const install = useModelsStore((state) => state.install)
  const installing = useModelsStore((state) => state.installing)
  const progress = useModelsStore((state) => state.progress)

  const row = rowFor(profile.readiness)
  const active = installing === profile.id
  const live = active ? installView(progress) : null

  return (
    <article className="model-row">
      <header className="model-row-head">
        <h3 className="model-row-title">{profile.display_name}</h3>
        <span className={`status-pill status-pill-${row.tone}`}>{row.label}</span>
      </header>

      {/* Shown for every model, installed or not: some weights are open with
          conditions the user takes on by generating with them (CONVENTIONS). */}
      <p className="model-row-license">
        <span className="model-row-license-name">{profile.license}</span>
        {profile.license_notes !== null ? ` -- ${profile.license_notes}` : null}
      </p>

      {row.nextStep !== null && !active ? <p className="setup-next-step">{row.nextStep}</p> : null}

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
    </article>
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
