import { useProjectsStore } from '../state/projects'
import type { ProjectRow as Row } from '../state/projects'

/** One project in the Setup picker: pick it, see when it was made, delete it. */
export function ProjectPickerRow({
  row,
  selected,
  onSelect,
}: {
  row: Row
  selected: boolean
  onSelect: () => void
}) {
  return (
    <li className={`picker-row ${selected ? 'picker-row-selected' : ''}`}>
      <label className="picker-row-pick">
        <input
          type="radio"
          name="project"
          checked={selected}
          onChange={onSelect}
        />
        <span className="picker-row-name">{row.name}</span>
      </label>

      <div className="picker-row-meta">
        <span className="picker-row-tag">{row.created}</span>
        <ProjectDelete slug={row.slug} name={row.name} />
      </div>
    </li>
  )
}

/** Inline delete: a button that becomes a "Delete 'name'? … Delete / Cancel". */
function ProjectDelete({ slug, name }: { slug: string; name: string }) {
  const confirming = useProjectsStore((state) => state.confirmingDelete === slug)
  const askDelete = useProjectsStore((state) => state.askDelete)
  const cancelDelete = useProjectsStore((state) => state.cancelDelete)
  const deleteProject = useProjectsStore((state) => state.deleteProject)

  if (!confirming) {
    return (
      <button
        type="button"
        className="project-row-delete"
        onClick={() => askDelete(slug)}
      >
        Delete
      </button>
    )
  }
  return (
    <div className="project-delete-confirm">
      <span className="project-delete-prompt">
        Delete “{name}”? This trashes the whole project — every track, lyric and
        album in it — to the Recycle Bin.
      </span>
      <button
        type="button"
        className="project-delete-yes"
        onClick={() => void deleteProject(slug)}
      >
        Delete
      </button>
      <button
        type="button"
        className="project-delete-cancel"
        onClick={() => cancelDelete()}
      >
        Cancel
      </button>
    </div>
  )
}
