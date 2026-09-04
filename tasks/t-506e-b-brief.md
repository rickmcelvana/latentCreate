# T-506e-b: the cover stores

**Depends:** T-506e-a (`Track.cover`, `AlbumList.cover`, `set_track_cover`, `album_set_cover`,
`delete_art` are registered)
**Dir:** `app/src` | **Lane:** Aider — three bridge functions, two wire types, one new selector
module, and four store additions. **No views.**

**T-506e's frontend half is split in two**, the c/d rhythm this phase has already used twice: **e-b
is the stores** (this brief), **e-c is the views** and carries the click-through. Every rule below
is a pure function or a store action with a test; e-c is the layer this repo cannot test in `node`.

**Files to create/modify (nine, plus five test files):**
- `app/src/bridge/library.ts` — `Track.cover`
- `app/src/bridge/projects.ts` — `AlbumList.cover`
- `app/src/bridge/tracks.ts` — `setTrackCover`
- `app/src/bridge/albums.ts` — `setAlbumCover`
- `app/src/bridge/art.ts` — `deleteArt`
- `app/src/state/covers.ts` — **new**, four selectors
- `app/src/state/covers.test.ts` — **new**
- `app/src/state/library.ts` — `TrackRow.cover`
- `app/src/state/albums.ts` — `AlbumRow.cover`, `setCover`
- `app/src/state/trackActions.ts` — `setCover`
- `app/src/state/art.ts` — the delete confirm and `remove`
- `app/src/state/library.test.ts`, `albums.test.ts`, `trackActions.test.ts`, `art.test.ts`

## Goal

Everything the views in T-506e-c will render or call: which cover a row has, whether it still
exists, what a delete will detach, and the four actions that set, clear and delete.

## Spec

### 1. The wire types and the three bridge functions

`Track` gains `cover: string | null`; `AlbumList` gains `cover: string | null`. Both mirror an
`Option<ArtId>`, which is `#[serde(transparent)]`, so it arrives as a bare id string or `null`.

```ts
// bridge/tracks.ts
/** Set or clear a track's cover. `null` clears it. */
export async function setTrackCover(id: string, cover: string | null): Promise<void>

// bridge/albums.ts -- returns the refreshed list, like every other album call.
export async function setAlbumCover(album: string, cover: string | null): Promise<AlbumList[]>

// bridge/art.ts
/**
 * Delete an artwork: image and sidecar to the OS trash, the id unlisted, and
 * every track and album cover naming it cleared. The caller must reload the
 * library and the albums as well as the gallery -- records the frontend already
 * holds were changed on disk by this call.
 */
export async function deleteArt(id: string): Promise<void>
```

Command names are `set_track_cover`, `album_set_cover`, `delete_art`, with argument keys `id` /
`cover`, `album` / `cover`, `id`.

### 2. `state/covers.ts` — the selectors

Its own module rather than lines in `art.ts`: these describe a cover **on something else**, and both
the Library and the album panel need them without pulling in the gallery store.

```ts
/** What a row shows where its cover goes. */
export type CoverView =
  | { state: 'none' }
  /**
   * The row names an artwork the gallery does not have. **Rendered as missing,
   * never as an error** -- the T-403 rule, and a state T-506e-a can really
   * leave behind: clearing covers on delete is N atomic writes, not one
   * transaction, so a crash part-way leaves a track naming a deleted artwork.
   */
  | { state: 'missing'; id: string }
  | { state: 'shown'; id: string; name: string; url: string | null }

export function coverView(coverId: string | null, art: ArtRow[]): CoverView

/** How many tracks and albums use this artwork as their cover. */
export function coverUsage(
  artId: string,
  tracks: TrackRow[],
  albums: AlbumList[],
): { tracks: number; albums: number }

/**
 * The delete confirm, as one sentence plus the specifics when they are known.
 *
 * The rule is stated unconditionally -- the image and its record go to the
 * Recycle Bin, and anything using it as a cover loses it -- because it is true
 * whether or not the library has loaded. The counts are appended only when
 * there are any, so a view that has not loaded the tracks understates nothing;
 * it simply says less.
 */
export function deleteArtPrompt(name: string, usage: { tracks: number; albums: number }): string

/**
 * The options a cover picker offers: "No cover" first, then every artwork in
 * gallery order. `id: null` is the clear.
 */
export function coverChoices(art: ArtRow[]): { id: string | null; label: string }[]
```

`deleteArtPrompt` wording:

- base — ``Delete “${name}”? The image and its record go to the Recycle Bin, and anything using it as a cover loses it.``
- append when either count is non-zero — ` It is the cover for 2 tracks and 1 album.` — singular
  and plural both correct (`1 track`, `2 tracks`), and a zero side omitted entirely
  (`It is the cover for 1 album.`), never `0 tracks`.

`coverView`'s `url` comes straight from the `ArtRow`; it can be `null` when the path would not
resolve, which the tile already renders as missing-image. `name` is the row's name, which is already
"title else id".

### 3. `TrackRow.cover` and `AlbumRow.cover`

`trackRows` carries `cover: track.cover` through; `albumRows` carries `cover: album.cover`. Both are
the **id only** — the join against the gallery is `coverView`'s job, done by the caller, the same
split `albumRows` already makes when it joins track ids against library rows and leaves an unknown
id as `null` rather than dropping the entry.

### 4. `trackActions.setCover`

```ts
/** Set or clear a track's cover, then reload the library so the row shows it. */
setCover: (id: string, cover: string | null) => Promise<boolean>
```

Follows `submitRename` exactly: `busy` set to the id, the error stored **against that track id** so
`errorFor` renders it under the right row, `busy` cleared in both paths. `set_track_cover` returns
nothing, so this reloads through `useLibraryStore.getState().load()` on success — the sidecar
changed on disk and `TrackRow` is built from it.

### 5. `albums.setCover`

```ts
/** Set or clear an album's cover. Resolves `true` on success. */
setCover: (album: string, cover: string | null) => Promise<boolean>
```

The command returns the refreshed albums, so this sets them directly and reloads nothing — the shape
`addTrack` and `removeTrack` already use. Error into the store's own `error`, as its siblings do.

### 6. `state/art.ts` — the delete confirm and `remove`

```ts
/** The artwork id awaiting a delete confirm, or `null`. */
confirmingDelete: string | null
askDelete: (id: string) => void
cancelDelete: () => void
/** Delete an artwork and refresh everything it could have touched. */
remove: (id: string) => Promise<boolean>
```

`confirmingDelete` is store-held so **only one tile confirms at a time** — the shape
`useProjectsStore`, `useAlbumsStore` and `useTrackActionsStore` all use for their deletes.

`remove` calls `deleteArt(id)` and then reloads **three** stores:

```ts
await get().load()
await useLibraryStore.getState().load()
await useAlbumsStore.getState().load()
```

Not defensiveness: `delete_art` clears the cover on every referencing track sidecar and album, so
the library rows and album lists the frontend is holding are stale the moment it returns. Skipping
either leaves a cover on screen that no longer exists on disk — the exact lie ARCHITECTURE §8's
one-source-of-truth rule exists to prevent, arriving one layer up.

On failure: keep `confirmingDelete` set so the tile can retry or cancel, store the message verbatim
in `error`, and reload nothing — the same choice `trackActions.confirmDelete` makes.

**Import direction:** `art.ts` may import `library.ts` and `albums.ts`; neither imports `art.ts`, so
there is no cycle. `projects.ts` already imports `art.ts` and stays above all three.

## Tests — named by the invariant

`covers.test.ts`:
- **a null cover is `none`; a cover the gallery has is `shown` with its name and URL.**
- **a cover id no artwork answers to is `missing`, carrying the id** — *not an error and not
  `none`: `none` would say "this track never had a cover", which is a different and false thing,
  and it is reachable because e-a clears covers in N separate writes.*
- **`coverUsage` counts tracks and albums separately, and counts nothing for an unrelated id.**
- **the prompt states the rule with no counts when nothing uses it**, and **appends singular and
  plural correctly**, and **omits a zero side rather than saying `0 tracks`.**
- **`coverChoices` puts "No cover" first with `id: null`, then the gallery in order.**

`library.test.ts` / `albums.test.ts`:
- **the row carries the cover id through** — one assertion each; the fixtures gain the field.

`trackActions.test.ts`:
- **setting a cover reloads the library** — the sidecar is the source of truth and the row is built
  from it, so without the reload the change is invisible until something else reloads.
- **a failure stores the message against that track id and clears `busy`.**

`albums.test.ts`:
- **setting an album cover uses the returned list and does not re-list** — asserting `listAlbums`
  was not called, the way the other album actions are tested.

`art.test.ts`:
- **`remove` reloads the gallery, the library and the albums** — *the one that matters: a track's
  cover was cleared on disk by this call, and a stale row would show a cover that is gone.*
- **a failed remove keeps `confirmingDelete` set, stores the error, and reloads nothing.**
- **`askDelete` arms one id at a time; `cancelDelete` clears it.**

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the listed files
- [ ] no view or component changes — no `.tsx` file is touched
- [ ] `trackRows`' other columns are unchanged

## Out of scope
- **Every view and component** — `CoverPicker`, the track card, the album panel, the tile's Delete
  button, the CSS: T-506e-c, which carries the click-through.
- **Choosing a cover at generation time.** A cover is attached afterwards, from artwork that exists.
- **More than one cover per track or album.**

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/state/projects.ts --file app/src/bridge/library.ts --file app/src/bridge/projects.ts --file app/src/bridge/tracks.ts --file app/src/bridge/albums.ts --file app/src/bridge/art.ts --file app/src/state/covers.ts --file app/src/state/covers.test.ts --file app/src/state/library.ts --file app/src/state/library.test.ts --file app/src/state/albums.ts --file app/src/state/albums.test.ts --file app/src/state/trackActions.ts --file app/src/state/trackActions.test.ts --file app/src/state/art.ts --file app/src/state/art.test.ts
```
