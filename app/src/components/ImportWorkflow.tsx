import { open } from '@tauri-apps/plugin-dialog'
import { RoleMapping } from './RoleMapping'
import { useImportStore } from '../state/import'

/**
 * Import a ComfyUI workflow and map its inputs to the app's semantic roles.
 *
 * Renders `state/import.ts` and derives nothing: `roleRows` and `canSave`
 * already exist for exactly this reason. Every Phase 2 milestone defect was
 * correct logic derived inline in a view, invisible to the whole gate.
 */
export function ImportWorkflow() {
  const phase = useImportStore((s) => s.phase)
  const adopting = useImportStore((s) => s.adopting)
  const begin = useImportStore((s) => s.begin)

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

  if (adopting !== null) {
    return (
      <div className="import-workflow">
        <button type="button" className="import-button" disabled>
          Import a workflow…
        </button>
        <p className="import-note">
          Bringing in a model on the Setup screen. Finish there first.
        </p>
      </div>
    )
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

  return <RoleMapping />
}
