# T-408c — delete an album list

Part **c** of T-408 (delete for every kind of created content). Parts a (lyric
version) and b (lyric document) landed and passed their click-throughs. This is
the simplest part, and the one that breaks the pattern: **an album has no file.**

## What an album is, and why this part is different

An album is a **named entry in `project.albums`** (`AlbumList { name, tracks }`),
edited in place in `project.json`. It is name-addressed and has no file of its
own — the T-403 decision that gave it a name instead of a slug precisely because
a name never maps to a path. Three consequences, all of which the other T-408
parts do *not* share:

- **No trasher.** The roadmap line "T-408 … reusing T-405's `trash_to_os`"
  applies to documents (b) and the project tree (d), which are real files.
  Deleting an album moves nothing to the Recycle Bin — there is nothing on disk
  to move. **Do not inject a trasher here**; it would be dead machinery.
- **No refusal.** a and b refuse when a track references the version/document
  (`VersionReferenced` / `DocumentReferenced`), because the reference points the
  wrong way to render-missing safely. An album points *at* tracks; **nothing
  points at an album.** So delete always succeeds when the album exists — there
  is no reference to check and no new `LibraryError` variant.
- **The tracks are never touched.** An album is a view over `project.tracks`.
  Deleting the list removes the `AlbumList` and nothing else — every track it
  held stays in the library, still on disk, still in `project.tracks`. This is
  the one invariant that matters, and the one test a mutation would most want to
  break.

`NotFound { kind: "album", .. }` on an unknown name, exactly as every other
album mutation (`find_album` already gives this).

## c-back — `library::albums::delete_album`

Add next to `rename_album` in `crates/library/src/albums.rs`:

```rust
/// Deletes an album list, returning the project's remaining albums.
///
/// **Removes only the list, never its tracks.** An album is a named view over
/// `project.tracks` with no file of its own, so — unlike deleting a track,
/// document or project — this trashes nothing and leaves every track it held in
/// the library. Nothing references an album, so there is no refusal path: an
/// existing album always deletes. An unknown name is a NotFound error, never a
/// silent no-op.
pub fn delete_album(root: &Path, slug: &str, name: &str) -> Result<Vec<AlbumList>, LibraryError> {
    let mut project = load_project(root, slug)?;
    let index = find_album(&project, name)?;
    project.albums.remove(index);
    save_project(root, &project)?;
    Ok(project.albums)
}
```

Tests (append to the module):

1. `test_delete_album_removes_the_list_and_persists` — create "A" and "B",
   delete **"B"**, assert the returned list is `["A"]` and a reload agrees.
   (Deleting "B" not "A" is what kills a `remove(0)` mutation.)
2. `test_delete_album_leaves_its_tracks_in_the_project` — album "A" with two
   tracks added, `delete_album("A")`, then assert `load_project(...).tracks`
   still holds both ids. **The invariant: deleting a list never deletes songs.**
3. `test_delete_album_of_an_unknown_name_errors` — `delete_album("nope")` is
   `NotFound { kind: "album", .. }` and the album list is unchanged.

Mutation pass (file-copy backup, **not `git checkout`** — that trap cost a
session in T-408a): (1) `remove(index)` → `remove(0)` killed by test 1;
(2) `remove(index)` → no-op killed by test 1; (3) drop the `save_project` call
killed by test 1's reload; (4) also mutate the tracks — there is no line
touching `project.tracks`, so test 2 stands as the guard that none is ever
added.

## c-cmd — `album_delete`

Add to `src-tauri/src/albums.rs`, same shape as `album_rename`:

```rust
/// Delete an album from the selected project. Removes only the list; every
/// track it held stays in the library.
#[tauri::command]
pub fn album_delete(config_dir: State<'_, ConfigDir>, name: String) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::delete_album(&config_dir.0, &project.slug, &name).map_err(|e| e.to_string())
}
```

Register `albums::album_delete` in the `invoke_handler!` list in
`src-tauri/src/lib.rs`, beside the other `album_*` commands.

## c-front — the delete affordance

**Bridge** (`app/src/bridge/albums.ts`):

```ts
/** Delete an album. Returns the refreshed album list. */
export async function deleteAlbum(name: string): Promise<AlbumList[]> {
  return await invoke<AlbumList[]>('album_delete', { name })
}
```

**Store** (`app/src/state/albums.ts`) — mirror the confirm pattern a and b use
(store-held, so one row confirms at a time), aliasing the bridge import to avoid
the name clash:

- import `deleteAlbum as deleteAlbumRequest`
- `confirmingDelete: string | null` (the album name awaiting confirm; `null` when none)
- `askDelete(name)` → `set({ confirmingDelete: name, error: null })`
- `cancelDelete()` → `set({ confirmingDelete: null })`
- `deleteAlbum(name)`:
  - `const albums = await deleteAlbumRequest(name)`
  - if the deleted album was `open`, close it (`open: get().open === name ? null : get().open`)
  - `set({ albums, open, confirmingDelete: null, error: null })`, return `true`
  - catch → `set({ error, confirmingDelete: null })`, return `false`

Album delete has no far-from-interaction refusal message (only rare
infrastructure errors), so the shared `error` at the panel top is fine — no
per-row error field like a/b needed. Reset `confirmingDelete: null` in the
test's `reset()`.

**View** (`app/src/components/AlbumPanel.tsx`) — a Delete control beside Rename
in `album-row-head`. Wrap the two in `<div className="album-row-actions">`. New
component next to `AlbumRename`:

```tsx
/** Inline delete: a button that becomes a "Delete 'name'? Delete / Cancel". */
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
```

The confirm copy carries the one thing a user needs reassuring about — deleting
the list keeps the songs. CSS: reuse the `.album-row-rename` / `.album-rename-*`
look for `.album-row-delete`, `.album-delete-confirm`, `.album-delete-prompt`,
`.album-delete-yes` (danger-toned via `--danger`), `.album-delete-cancel`, and
add `.album-row-actions { display: flex; gap: var(--gap-xs); }`.

**Store tests** (`app/src/state/albums.test.ts`): add `mockDeleteAlbum`, wire it
into the `vi.mock`, reset it in `beforeEach`, and add:
- delete adopts the refreshed list and clears the confirm
- deleting the open album closes it (`open` back to `null`)
- deleting a different album leaves `open` untouched
- `askDelete` / `cancelDelete` toggle `confirmingDelete`

## Click-through (producer, on the desktop app)

1. In the Library, create two albums; add a track or two to one of them.
2. Delete the **empty** album — it disappears, the other stays.
3. Delete the album **with tracks** — confirm it warns the tracks stay; after
   deleting, the album is gone **and every one of its tracks is still in the
   library list** (the whole point).
4. Open an album, then delete it — the open body closes cleanly.
5. Nothing hit the Recycle Bin (no file was ever involved).

## Not this task

- T-408d: delete a project (the whole `projects/<slug>/` tree → OS trash; this
  one *does* reuse `trash_to_os`, and exercises `selected_project`'s
  "configured slug no longer exists" fallback).
- Persisting which album is open across restarts (out of scope, as in T-403).
