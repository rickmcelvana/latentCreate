import { useEffect, useState } from 'react'
import { useConfigStore } from '../state/config'
import {
  effectiveProjectSlug,
  projectRow,
  useProjectsStore,
  type ProjectRow,
} from '../state/projects'
import {
  EMPTY_LIBRARY,
  provenanceView,
  useLibraryStore,
  type TrackRow,
} from '../state/library'
import { Player } from '../components/Player'
import { usePlayerStore } from '../state/player'
import { AlbumPanel } from '../components/AlbumPanel'
import { useAlbumsStore } from '../state/albums'
import {
  failureFor,
  isSending,
  SEND_TARGET_NAMES,
  SEND_TARGETS,
  useSendToStore,
} from '../state/sendto'
import {
  errorFor,
  filenameSafe,
  isRow,
  useTrackActionsStore,
} from '../state/trackActions'

export function Library() {
  const tracks = useLibraryStore((state) => state.tracks)
  const warnings = useLibraryStore((state) => state.warnings)
  const error = useLibraryStore((state) => state.error)
  const load = useLibraryStore((state) => state.load)
  const startListening = useLibraryStore((state) => state.startListening)

  const config = useConfigStore((state) => state.config)
  const projects = useProjectsStore((state) => state.projects)
  const projectError = useProjectsStore((state) => state.error)
  const projectWarnings = useProjectsStore((state) => state.warnings)
  const loadProjects = useProjectsStore((state) => state.load)
  const selectProject = useProjectsStore((state) => state.select)
  const albumsLoad = useAlbumsStore((state) => state.load)

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    void startListening()
  }, [startListening])

  useEffect(() => {
    void loadProjects()
  }, [loadProjects])

  const selected = effectiveProjectSlug(config, projects)
  const rows = projects.map(projectRow)

  useEffect(() => {
    void albumsLoad()
  }, [albumsLoad, selected])

  return (
    <>
      <h1 className="view-title">Library</h1>
      <p className="view-subtitle">
        Everything you have made, with the recipe that made it.
      </p>

      <section className="panel project-picker">
        <h2 className="project-picker-title">Project</h2>

        {projectError !== null ? <p className="library-error">{projectError}</p> : null}
        {projectWarnings !== null ? <p className="library-warning">{projectWarnings}</p> : null}

        <ul className="project-list">
          {rows.map((row) => (
            <ProjectRow
              key={row.slug}
              row={row}
              selected={row.slug === selected}
              onSelect={() => void selectProject(row.slug)}
            />
          ))}
        </ul>

        <ProjectCreate />
      </section>

      {error !== null ? (
        <p className="library-error">
          {error}
          <button
            type="button"
            className="library-retry"
            onClick={() => void load()}
          >
            Retry
          </button>
        </p>
      ) : null}

      {warnings !== null ? <p className="library-warning">{warnings}</p> : null}

      {tracks.length === 0 ? (
        <p className="library-empty">{EMPTY_LIBRARY}</p>
      ) : (
        <ul className="track-list">
          {tracks.map((row) => (
            <TrackCard key={row.id} row={row} />
          ))}
        </ul>
      )}

      <AlbumPanel />

      <Player />
    </>
  )
}

function ProjectRow({
  row,
  selected,
  onSelect,
}: {
  row: ProjectRow
  selected: boolean
  onSelect: () => void
}) {
  return (
    <li className={`project-row ${selected ? 'project-row-selected' : ''}`}>
      <label className="project-row-pick">
        <input
          type="radio"
          name="project"
          checked={selected}
          onChange={onSelect}
        />
        <span className="project-row-name">{row.name}</span>
      </label>

      <div className="project-row-meta">
        <span className="project-row-created">{row.created}</span>
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

function TrackCard({ row }: { row: TrackRow }) {
  const play = usePlayerStore((state) => state.play)
  const send = useSendToStore((state) => state.send)
  const sending = useSendToStore((state) => state.sending)
  const sendFailure = useSendToStore((state) => state.failure)

  const reloadLibrary = useLibraryStore((state) => state.load)
  const actions = useTrackActionsStore()

  const sendError = failureFor(sendFailure, row.id)
  const actionError = errorFor(actions.error, row.id)
  const busy = isSending(sending, row.id) || isRow(actions.busy, row.id)
  const confirming = isRow(actions.confirming, row.id)
  const renaming = isRow(actions.renaming, row.id)

  return (
    <li className="panel track-row">
      <div className="track-head">
        <span className="track-name">{row.name}</span>
        <div className="track-head-actions">
          <button
            type="button"
            className="track-play"
            onClick={() => void play(row.id, row.name)}
          >
            Play
          </button>
          <span className="track-send-label">Send to</span>
          {SEND_TARGETS.map((target) => (
            <button
              key={target}
              type="button"
              className="track-send"
              disabled={busy}
              onClick={() => void send(row.id, target)}
            >
              {SEND_TARGET_NAMES[target]}
            </button>
          ))}
          <span className="track-duration">{row.duration}</span>
        </div>
      </div>

      <dl className="track-recipe">
        <div className="track-fact">
          <dt>Model</dt>
          <dd>{row.model}</dd>
        </div>
        <div className="track-fact">
          <dt>Licence</dt>
          <dd>{row.license}</dd>
        </div>
        <div className="track-fact">
          <dt>Created</dt>
          <dd>{row.created}</dd>
        </div>
        <div className="track-fact">
          <dt>Seed</dt>
          <dd>{row.seed}</dd>
        </div>
        {row.loras !== '' ? (
          <div className="track-fact">
            <dt>LoRAs</dt>
            <dd>{row.loras}</dd>
          </div>
        ) : null}
        {row.promptId !== null ? (
          <div className="track-fact">
            <dt>Run</dt>
            <dd>{row.promptId}</dd>
          </div>
        ) : null}
      </dl>

      <TrackDetails trackId={row.id} />

      <p className="track-file">{row.file}</p>

      {renaming ? (
        <RenameRow row={row} onDone={() => void reloadLibrary()} />
      ) : confirming ? (
        <ConfirmDeleteRow row={row} onDone={() => void reloadLibrary()} />
      ) : (
        <div className="track-actions">
          <button
            type="button"
            className="track-action"
            disabled={busy}
            onClick={() => actions.startRename(row.id)}
          >
            Rename
          </button>
          <button
            type="button"
            className="track-action"
            disabled={busy}
            onClick={() => {
              const ext = row.file.split('.').pop() ?? 'flac'
              // Sanitise the title before the dialog (T-409 trap 4); the id is
              // always filename-safe, so it is the fallback for an all-illegal
              // title.
              const stem = filenameSafe(row.name) || row.id
              void actions.runExport(row.id, `${stem}.${ext}`)
            }}
          >
            Export
          </button>
          <button
            type="button"
            className="track-action"
            disabled={busy}
            onClick={() => void actions.reveal(row.id)}
          >
            Reveal
          </button>
          <button
            type="button"
            className="track-action"
            disabled={busy}
            onClick={() => actions.askDelete(row.id)}
          >
            Delete
          </button>
        </div>
      )}

      {actionError !== null ? <p className="track-action-error">{actionError}</p> : null}
      {sendError !== null ? <p className="track-send-error">{sendError}</p> : null}
    </li>
  )
}

/**
 * The full recipe behind a track, revealed on demand (T-406): every input, the
 * resolved slots ComfyUI received, the lyric ref, and the server that ran it.
 * Open/closed is local state -- a read-only disclosure needs no cross-row
 * exclusivity, unlike the delete/rename confirms the store holds.
 */
function TrackDetails({ trackId }: { trackId: string }) {
  const track = useLibraryStore((state) => state.byId[trackId])
  const [open, setOpen] = useState(false)

  if (track === undefined) return null
  const sections = provenanceView(track)

  return (
    <div className="track-details">
      <button
        type="button"
        className="track-details-toggle"
        aria-expanded={open}
        onClick={() => setOpen((was) => !was)}
      >
        {open ? 'Hide details' : 'Details'}
      </button>

      {open ? (
        <div className="track-details-body">
          {sections.map((section) => (
            <dl className="track-details-section" key={section.title}>
              <dt className="track-details-title">{section.title}</dt>
              {section.facts.map((fact) => (
                <div className="track-details-fact" key={`${section.title}:${fact.label}`}>
                  <span className="track-details-label">{fact.label}</span>
                  <span className="track-details-value">{fact.value}</span>
                </div>
              ))}
            </dl>
          ))}
        </div>
      ) : null}
    </div>
  )
}

function RenameRow({
  row,
  onDone,
}: {
  row: TrackRow
  onDone: () => void
}) {
  const [value, setValue] = useState(row.name)
  const submitRename = useTrackActionsStore((state) => state.submitRename)
  const cancelRename = useTrackActionsStore((state) => state.cancelRename)

  return (
    <form
      className="track-action-rename"
      onSubmit={(event) => {
        event.preventDefault()
        void submitRename(row.id, value).then((ok) => {
          if (ok) onDone()
        })
      }}
    >
      <input
        type="text"
        value={value}
        onChange={(event) => setValue(event.target.value)}
      />
      <button type="submit" className="track-action">
        Save
      </button>
      <button
        type="button"
        className="track-action"
        onClick={() => cancelRename()}
      >
        Cancel
      </button>
    </form>
  )
}

function ConfirmDeleteRow({
  row,
  onDone,
}: {
  row: TrackRow
  onDone: () => void
}) {
  const confirmDelete = useTrackActionsStore((state) => state.confirmDelete)
  const cancelDelete = useTrackActionsStore((state) => state.cancelDelete)

  return (
    <div className="track-action-confirm">
      <span>Move to Trash?</span>
      <button
        type="button"
        className="track-action"
        onClick={() => {
          void confirmDelete(row.id).then((ok) => {
            if (ok) onDone()
          })
        }}
      >
        Trash it
      </button>
      <button
        type="button"
        className="track-action"
        onClick={() => cancelDelete()}
      >
        Cancel
      </button>
    </div>
  )
}
