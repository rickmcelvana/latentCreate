import { useEffect } from 'react'
import { EMPTY_LIBRARY, useLibraryStore, type TrackRow } from '../state/library'

export function Library() {
  const tracks = useLibraryStore((state) => state.tracks)
  const warnings = useLibraryStore((state) => state.warnings)
  const error = useLibraryStore((state) => state.error)
  const load = useLibraryStore((state) => state.load)
  const startListening = useLibraryStore((state) => state.startListening)

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    void startListening()
  }, [startListening])

  return (
    <>
      <h1 className="view-title">Library</h1>
      <p className="view-subtitle">
        Everything you have made, with the recipe that made it.
      </p>

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
    </>
  )
}

function TrackCard({ row }: { row: TrackRow }) {
  return (
    <li className="panel track-row">
      <div className="track-head">
        <span className="track-name">{row.name}</span>
        <span className="track-duration">{row.duration}</span>
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
