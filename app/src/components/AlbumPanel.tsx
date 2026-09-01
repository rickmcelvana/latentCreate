import { useState } from 'react'
import { albumRows, useAlbumsStore, type AlbumRow } from '../state/albums'
import { useLibraryStore } from '../state/library'

/**
 * The Library's album section: create albums, open one to see its tracks in
 * order, add/remove/reorder them, rename them. Every decision lives in
 * state/albums.ts; this component renders (T-403).
 */
export function AlbumPanel() {
  const albums = useAlbumsStore((state) => state.albums)
  const open = useAlbumsStore((state) => state.open)
  const error = useAlbumsStore((state) => state.error)
  const create = useAlbumsStore((state) => state.create)
  const openAlbum = useAlbumsStore((state) => state.openAlbum)
  const tracks = useLibraryStore((state) => state.tracks)

  const [name, setName] = useState('')
  const rows = albumRows(albums, tracks)

  return (
    <section className="panel album-panel">
      <h2 className="album-panel-title">Albums</h2>

      {error !== null ? <p className="library-error">{error}</p> : null}

      <form
        className="album-create"
        onSubmit={(event) => {
          event.preventDefault()
          void create(name).then((ok) => {
            if (ok) setName('')
          })
        }}
      >
        <input
          className="album-create-input"
          type="text"
          value={name}
          placeholder="New album name"
          onChange={(event) => setName(event.target.value)}
        />
        <button type="submit" className="album-create-button" disabled={name.trim() === ''}>
          Create
        </button>
      </form>

      {rows.length === 0 ? (
        <p className="album-empty">Albums you create will appear here.</p>
      ) : (
        <ul className="album-list">
          {rows.map((row) => (
            <li className="album-row" key={row.name}>
              <div className="album-row-head">
                <button
                  type="button"
                  className="album-row-toggle"
                  onClick={() => openAlbum(open === row.name ? null : row.name)}
                  aria-expanded={open === row.name}
                >
                  <span className="album-row-name">{row.name}</span>
                  <span className="album-row-count">
                    {row.entries.length} {row.entries.length === 1 ? 'track' : 'tracks'}
                  </span>
                </button>
                <div className="album-row-actions">
                  <AlbumRename name={row.name} />
                  <AlbumDelete name={row.name} />
                </div>
              </div>

              {open === row.name ? (
                <div className="album-row-body">
                  <AlbumTrackList row={row} />
                  <AlbumAddTrack row={row} />
                </div>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

/** The album's tracks in order, each with move-up/move-down/remove. */
function AlbumTrackList({ row }: { row: AlbumRow }) {
  const removeTrack = useAlbumsStore((state) => state.removeTrack)
  const move = useAlbumsStore((state) => state.move)

  return (
    <ol className="album-tracks">
      {row.entries.map((entry, index) => (
        <li className="album-track" key={entry.trackId}>
          <span
            className={
              entry.name === null ? 'album-track-name album-track-missing' : 'album-track-name'
            }
          >
            {entry.name ?? 'Missing track'}
          </span>
          <div className="album-track-actions">
            <button
              type="button"
              className="album-track-move"
              onClick={() => void move(row.name, entry.trackId, 'up')}
              disabled={index === 0}
              aria-label="Move up"
            >
              Up
            </button>
            <button
              type="button"
              className="album-track-move"
              onClick={() => void move(row.name, entry.trackId, 'down')}
              disabled={index === row.entries.length - 1}
              aria-label="Move down"
            >
              Down
            </button>
            <button
              type="button"
              className="album-track-remove"
              onClick={() => void removeTrack(row.name, entry.trackId)}
            >
              Remove
            </button>
          </div>
        </li>
      ))}
    </ol>
  )
}

/** Add a library track that is not already in the album. */
function AlbumAddTrack({ row }: { row: AlbumRow }) {
  const tracks = useLibraryStore((state) => state.tracks)
  const addTrack = useAlbumsStore((state) => state.addTrack)
  const [pick, setPick] = useState('')

  const inAlbum = new Set(row.entries.map((entry) => entry.trackId))
  const addable = tracks.filter((track) => !inAlbum.has(track.id))
  if (addable.length === 0) return null

  return (
    <form
      className="album-add"
      onSubmit={(event) => {
        event.preventDefault()
        if (pick === '') return
        void addTrack(row.name, pick).then((ok) => {
          if (ok) setPick('')
        })
      }}
    >
      <select
        className="album-add-pick"
        value={pick}
        onChange={(event) => setPick(event.target.value)}
      >
        <option value="">Choose a track...</option>
        {addable.map((track) => (
          <option key={track.id} value={track.id}>
            {track.name}
          </option>
        ))}
      </select>
      <button type="submit" className="album-add-button" disabled={pick === ''}>
        Add
      </button>
    </form>
  )
}

/**
 * Inline delete: a button that becomes a "Delete 'name'? Delete / Cancel". The
 * confirm copy says the tracks stay, since removing a list keeps every song.
 */
function AlbumDelete({ name }: { name: string }) {
  const confirming = useAlbumsStore((state) => state.confirmingDelete === name)
  const askDelete = useAlbumsStore((state) => state.askDelete)
  const cancelDelete = useAlbumsStore((state) => state.cancelDelete)
  const deleteAlbum = useAlbumsStore((state) => state.deleteAlbum)

  if (!confirming) {
    return (
      <button type="button" className="album-row-delete" onClick={() => askDelete(name)}>
        Delete
      </button>
    )
  }

  return (
    <div className="album-delete-confirm">
      <span className="album-delete-prompt">Delete “{name}”? Its tracks stay in the library.</span>
      <button type="button" className="album-delete-yes" onClick={() => void deleteAlbum(name)}>
        Delete
      </button>
      <button type="button" className="album-delete-cancel" onClick={() => cancelDelete()}>
        Cancel
      </button>
    </div>
  )
}

/** Inline rename: a button that becomes an input + Save/Cancel. */
function AlbumRename({ name }: { name: string }) {
  const rename = useAlbumsStore((state) => state.rename)
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(name)

  if (!editing) {
    return (
      <button
        type="button"
        className="album-row-rename"
        onClick={() => {
          setDraft(name)
          setEditing(true)
        }}
      >
        Rename
      </button>
    )
  }

  return (
    <form
      className="album-rename"
      onSubmit={(event) => {
        event.preventDefault()
        void rename(name, draft).then((ok) => {
          if (ok) setEditing(false)
        })
      }}
    >
      <input
        className="album-rename-input"
        type="text"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />
      <button type="submit" className="album-rename-save" disabled={draft.trim() === ''}>
        Save
      </button>
      <button type="button" className="album-rename-cancel" onClick={() => setEditing(false)}>
        Cancel
      </button>
    </form>
  )
}
