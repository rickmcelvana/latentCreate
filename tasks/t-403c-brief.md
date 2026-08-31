# T-403c: album lists -- the Library album panel

**Depends:** T-403b (the albums store) | **Crate/dir:** app/src
**Files to create/modify:**
- `app/src/components/AlbumPanel.tsx` (new)
- `app/src/views/Library.tsx` (modify: render `<AlbumPanel />` + load albums on project change)
- `app/src/theme.css` (modify: append the album rules)

## Goal

The Library's album section: create an album, open one to see its tracks in order, reorder them
with up/down moves, remove them, add a track from the library, and rename an album. The component
renders the T-403b store's data; every decision already lives in `state/albums.ts` -- this file is
wiring (WORKFLOW section 1: UI wiring is the Aider lane, and the gate cannot exercise it, so
correctness rests on the store's tests plus the producer's click-through).

## The trap to design against

A track deleted after being added to an album stays in the album (T-403a). `albumRows` joins it to
a `null` name; this panel must render that row as **"Missing track"** -- visibly present, not
dropped (the T-403 trap). The add-track picker offers only library tracks not already in the album;
the reorder buttons call `move`, which computes the full new order in the store and sends it to
`album_reorder` (a full-order replace, validated as a permutation by the backend).

## Spec

### `app/src/components/AlbumPanel.tsx` (new)

```tsx
import { useState } from 'react'
import { albumRows, useAlbumsStore, type AlbumRow } from '../state/albums'
import { useLibraryStore } from '../state/library'

/**
 * The Library's album section: create albums, open one to see its tracks in
 * order, add/remove/reorder them, rename them. Every decision lives in
 * `state/albums.ts`; this component renders (T-403).
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
                <AlbumRename name={row.name} />
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
```

### `app/src/views/Library.tsx` (modify: anchors, not the whole file)

Add the imports (after the existing `import { usePlayerStore } ...` line):

```tsx
import { AlbumPanel } from '../components/AlbumPanel'
import { useAlbumsStore } from '../state/albums'
```

The Library loads tracks and projects in effects; albums get the same treatment, keyed on the
selected project so a project switch reloads them (the backend resolves the project itself, but the
view must ask). Add the store hook at the top of `Library` (after `const selectProject = ...`):

```tsx
  const albumsLoad = useAlbumsStore((state) => state.load)
```

The album load effect goes **after** `const selected = effectiveProjectSlug(config, projects)` --
the effect's dependency array reads `selected`, which is declared after the existing effects, so
this effect must come after that line (a `const` in its temporal dead zone would throw):

```tsx
  const selected = effectiveProjectSlug(config, projects)
  const rows = projects.map(projectRow)

  useEffect(() => {
    void albumsLoad()
  }, [albumsLoad, selected])
```

Render the panel between the track list conditional and the player. The view currently ends:

```tsx
      {tracks.length === 0 ? (
        <p className="library-empty">{EMPTY_LIBRARY}</p>
      ) : (
        <ul className="track-list">
          {tracks.map((row) => (
            <TrackCard key={row.id} row={row} />
          ))}
        </ul>
      )}

      <Player />
    </>
  )
}
```

Change it to:

```tsx
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
```

### `app/src/theme.css` (modify: append)

Append these rules at the end of the file. Do not change any existing rule.

```css
/* --- Albums (T-403) --- */

.album-panel {
  margin-top: var(--gap-lg);
}

.album-panel-title {
  margin: 0 0 var(--gap-md);
  font-size: 15px;
  font-weight: 600;
}

.album-create {
  display: flex;
  gap: var(--gap-sm);
  margin-bottom: var(--gap-md);
}

.album-create-input {
  flex: 1;
  min-width: 0;
  font: inherit;
  color: var(--text);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: var(--gap-sm);
}

.album-create-input:focus-visible {
  outline: none;
  border-color: var(--accent);
}

.album-create-button {
  font: inherit;
  color: var(--text);
  background: var(--panel-hover);
  border: 1px solid var(--border-bright);
  border-radius: var(--radius);
  padding: var(--gap-sm) var(--gap-md);
  cursor: pointer;
}

.album-create-button:hover:not(:disabled) {
  border-color: var(--accent);
}

.album-create-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.album-empty {
  margin: 0;
  color: var(--text-muted);
  font-size: 13px;
}

.album-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
}

.album-row {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg);
}

.album-row-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
  padding: var(--gap-sm) var(--gap-md);
}

.album-row-toggle {
  display: flex;
  align-items: baseline;
  gap: var(--gap-sm);
  font: inherit;
  color: var(--text);
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
}

.album-row-name {
  font-size: 14px;
  font-weight: 600;
}

.album-row-count {
  color: var(--text-muted);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.album-row-rename {
  font: inherit;
  font-size: 12px;
  color: var(--accent);
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: var(--gap-xs) var(--gap-sm);
  cursor: pointer;
}

.album-row-rename:hover {
  color: var(--accent-hover);
  border-color: var(--accent);
}

.album-rename {
  display: flex;
  gap: var(--gap-xs);
}

.album-rename-input {
  font: inherit;
  color: var(--text);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: var(--gap-xs) var(--gap-sm);
}

.album-rename-save,
.album-rename-cancel {
  font: inherit;
  font-size: 12px;
  border-radius: var(--radius);
  padding: var(--gap-xs) var(--gap-sm);
  cursor: pointer;
}

.album-rename-save {
  color: var(--text);
  background: var(--panel-hover);
  border: 1px solid var(--border-bright);
}

.album-rename-cancel {
  color: var(--text-muted);
  background: transparent;
  border: 1px solid var(--border);
}

.album-row-body {
  border-top: 1px solid var(--border);
  padding: var(--gap-sm) var(--gap-md) var(--gap-md);
  display: flex;
  flex-direction: column;
  gap: var(--gap-md);
}

.album-tracks {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);
}

.album-track {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
  font-size: 13px;
}

.album-track-name {
  color: var(--text);
}

.album-track-missing {
  color: var(--danger);
}

.album-track-actions {
  display: flex;
  gap: var(--gap-xs);
}

.album-track-move,
.album-track-remove {
  font: inherit;
  font-size: 12px;
  border-radius: var(--radius);
  padding: var(--gap-xs) var(--gap-sm);
  cursor: pointer;
}

.album-track-move {
  color: var(--text-muted);
  background: transparent;
  border: 1px solid var(--border);
}

.album-track-move:hover:not(:disabled) {
  color: var(--text);
  border-color: var(--accent);
}

.album-track-move:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.album-track-remove {
  color: var(--danger);
  background: transparent;
  border: 1px solid var(--border);
}

.album-track-remove:hover {
  border-color: var(--danger);
}

.album-add {
  display: flex;
  gap: var(--gap-sm);
}

.album-add-pick {
  flex: 1;
  min-width: 0;
  font: inherit;
  color: var(--text);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: var(--gap-sm);
}

.album-add-button {
  font: inherit;
  color: var(--text);
  background: var(--panel-hover);
  border: 1px solid var(--border-bright);
  border-radius: var(--radius);
  padding: var(--gap-sm) var(--gap-md);
  cursor: pointer;
}

.album-add-button:hover:not(:disabled) {
  border-color: var(--accent);
}

.album-add-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
```

## Acceptance criteria

- [ ] `tsc -b`, `oxlint src`, `vitest run` and `vite build` green; frontend stays **373** (no new
      tests -- this brief is wiring).
- [ ] Every new className in the diff has a rule in `theme.css`, and no existing `theme.css` rule changed.
- [ ] `invoke`, `listen` and `convertFileSrc` do not appear in `components/` or `views/` (grep `@tauri-apps` across `app/src` -- only in `bridge/`).
- [ ] The missing-track row renders `entry.name ?? 'Missing track'` -- the entry is visible, not dropped.
- [ ] The album load effect is declared after `const selected`, so it never reads `selected` in its temporal dead zone.

## Out of scope

- Album delete (not in phase scope).
- Drag-and-drop reorder (up/down buttons instead; drag is a later polish).
- Any store, bridge, backend or schema change (T-403a/T-403b).

## Manual verification (producer click-through -- the gate cannot check these)

1. Library: create an album, then a second one; a duplicate name shows the "already exists" error.
2. Add tracks from the library into an album; the picker stops offering tracks already inside it.
3. Move a track up and down; the order persists across a Library reload (switch project and back).
4. Remove a track from the album; it returns to the add picker.
5. Delete a track's audio file + sidecar from the project folder, reload the Library: the album
   shows **"Missing track"** in place, and the row is not dropped.
6. Rename an album; the open album follows the rename; a rename onto a taken name errors.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read app/src/state/albums.ts --read app/src/state/library.ts --read app/src/views/Library.tsx --read app/src/components/Player.tsx --file app/src/components/AlbumPanel.tsx --file app/src/views/Library.tsx --file app/src/theme.css
```
