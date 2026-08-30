import { useEffect } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { useModelsStore } from '../state/models'
import { canSave, roleRows, saveNotes, useImportStore } from '../state/import'

/**
 * Import a ComfyUI workflow and map its inputs to the app's semantic roles.
 *
 * Renders `state/import.ts` and derives nothing: `roleRows` and `canSave`
 * already exist for exactly this reason. Every Phase 2 milestone defect was
 * correct logic derived inline in a view, invisible to the whole gate.
 */
export function ImportWorkflow() {
  const phase = useImportStore((s) => s.phase)
  const report = useImportStore((s) => s.report)
  const selected = useImportStore((s) => s.selected)
  const name = useImportStore((s) => s.name)
  const begin = useImportStore((s) => s.begin)
  const toggle = useImportStore((s) => s.toggle)
  const setName = useImportStore((s) => s.setName)
  const save = useImportStore((s) => s.save)
  const reset = useImportStore((s) => s.reset)
  const refreshModels = useModelsStore((s) => s.refresh)

  // A saved profile has to appear in the picker above without a reload. This
  // is wiring, not a derivation -- the store decided it was saved.
  useEffect(() => {
    if (phase.kind === 'saved') void refreshModels()
  }, [phase, refreshModels])

  async function pick() {
    const chosen = await open({
      multiple: false,
      filters: [{ name: 'ComfyUI workflow', extensions: ['json'] }],
    })
    // **Cancelling is not an error.** `open` returns null, and reporting that
    // as a failure is the same mistake as rendering a cancelled job as failed
    // (MCP-SURFACE 21) -- it reports the user's own decision back to them as a
    // fault.
    if (typeof chosen !== 'string') return
    await begin(chosen)
  }

  if (phase.kind === 'idle') {
    return (
      <div className="import-workflow">
        <button type="button" className="import-button" onClick={() => void pick()}>
          Import a workflow…
        </button>
        {/* The cost of the copy-not-reference decision, stated where someone
            can learn it before it surprises them. */}
        <p className="import-note">
          Use ComfyUI&rsquo;s <strong>File &gt; Save (As)</strong> export. latentCreate keeps its own
          copy, so later edits in ComfyUI will not follow — re-import to pick them up.
        </p>
      </div>
    )
  }

  if (phase.kind === 'importing' || phase.kind === 'saving') {
    return (
      <div className="import-workflow">
        <p className="import-busy">
          {phase.kind === 'importing' ? 'Reading the workflow…' : 'Saving the profile…'}
        </p>
      </div>
    )
  }

  if (phase.kind === 'failed') {
    return (
      <div className="import-workflow">
        <p className="import-error">{phase.message}</p>
        <button type="button" className="import-button" onClick={reset}>
          Back
        </button>
      </div>
    )
  }

  if (phase.kind === 'saved') {
    return (
      <div className="import-workflow">
        <p className="import-saved">
          Saved as <code>{phase.profileId}</code>. It is in the list above.
        </p>
        <button type="button" className="import-button" onClick={reset}>
          Import another
        </button>
      </div>
    )
  }

  const rows = roleRows(report?.suggestions ?? [], selected)

  return (
    <div className="import-workflow">
      <label className="import-field">
        <span>Name</span>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="My workflow"
        />
      </label>

      {(report?.warnings ?? []).map((warning) => (
        <p className="import-warning" key={warning}>
          {warning}
        </p>
      ))}

      <ul className="import-roles">
        {rows.map((row) => (
          <li className="import-role" key={row.role}>
            <span className="import-role-label">{row.label}</span>
            {row.emptyNote !== null ? (
              <p className="import-role-empty">{row.emptyNote}</p>
            ) : (
              <ul className="import-candidates">
                {row.options.map(({ candidate, checked }) => (
                  <li key={candidate.address}>
                    <label className="import-candidate">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggle(row.role, candidate.address)}
                      />
                      <code>{candidate.address}</code>
                      {/* T-313c writes this so a person can check a guess
                          about their own graph. Without it the screen asks
                          for trust it has not earned. */}
                      <span className="import-candidate-reason">{candidate.reason}</span>
                    </label>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>

      {saveNotes(selected).map((note) => (
        <p className="import-warning" key={note}>
          {note}
        </p>
      ))}

      <div className="import-actions">
        <button
          type="button"
          className="import-button"
          disabled={!canSave(name, selected)}
          onClick={() => void save()}
        >
          Save as a profile
        </button>
        <button type="button" className="import-button import-button-quiet" onClick={reset}>
          Cancel
        </button>
      </div>
    </div>
  )
}
