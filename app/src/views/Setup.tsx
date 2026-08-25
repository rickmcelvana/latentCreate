import { useEffect } from 'react'
import type { ComfyStatus } from '../bridge/comfy'
import { useComfyStore, formatVram, pillFor } from '../state/comfy'

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
    </>
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
