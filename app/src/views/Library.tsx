import { useEffect, useState } from 'react'
import { useConfigStore } from '../state/config'
import {
  effectiveProjectSlug,
  projectRow,
  useProjectsStore,
  type ProjectRow,
} from '../state/projects'
import { EMPTY_LIBRARY, useLibraryStore, type TrackRow } from '../state/library'
import { Player } from '../components/Player'
import { usePlayerStore } from '../state/player'
import { AlbumPanel } from '../components/AlbumPanel'
import { useAlbumsStore } from '../state/albums'

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
      </div>
    </li>
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

      <p className="track-file">{row.file}</p>
    </li>
  )
}
