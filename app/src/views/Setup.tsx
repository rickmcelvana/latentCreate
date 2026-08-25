import { useEffect } from 'react'
import type { ComfyStatus } from '../bridge/comfy'
import { useComfyStore, formatVram, pillFor } from '../state/comfy'
import type { ProfileStatus } from '../bridge/models'
import { curatedFirst, formatBytes, installView, rowFor, useModelsStore } from '../state/models'

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
    </>
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
