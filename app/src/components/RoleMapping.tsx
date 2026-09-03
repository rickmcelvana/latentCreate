import { useEffect } from 'react'
import { useModelsStore } from '../state/models'
import { canSave, roleRows, saveNotes, useImportStore } from '../state/import'

/**
 * The import flow once it has started: busy, failed, saved, or the role-mapping
 * screen. Rendered by `ImportWorkflow` for a file the user picked and by the
 * catalog step for a gallery row being brought in -- one store, two entry
 * points, one screen.
 *
 * Renders `state/import.ts` and derives nothing: `roleRows`, `canSave` and
 * `saveNotes` exist for exactly this reason.
 */
export function RoleMapping({ savedLabel }: { savedLabel?: string }) {
  const phase = useImportStore((s) => s.phase)
  const report = useImportStore((s) => s.report)
  const selected = useImportStore((s) => s.selected)
  const name = useImportStore((s) => s.name)
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

  if (phase.kind === 'idle') return null

  if (phase.kind === 'importing' || phase.kind === 'saving') {
    return (
      <div className="import-workflow">
        <p className="import-busy">
          {phase.kind === 'importing' ? 'Reading the workflow…' : 'Saving the profile…'}
        </p>
      </div>
    )
  }

  const defaultSavedLabel = 'It is in the list above.'
  const resetLabel = savedLabel === undefined ? 'Import another' : 'Done'

  if (phase.kind === 'failed') {
    return (
      <div className="import-workflow">
        <p className="import-error">{phase.message}</p>
        {/* "Back", not the saved branch's label: nothing was imported, so
            "Import another" would describe something that did not happen. */}
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
          Saved as <code>{phase.profileId}</code>. {savedLabel ?? defaultSavedLabel}
        </p>
        <button type="button" className="import-button" onClick={reset}>
          {resetLabel}
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
