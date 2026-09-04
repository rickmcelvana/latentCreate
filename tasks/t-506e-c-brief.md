# T-506e-c: the cover views — and the last click-through in T-506

**Depends:** T-506e-a (the three commands), T-506e-b (`covers.ts`, `TrackRow.cover`,
`AlbumRow.cover`, the four store actions)
**Dir:** `app/src` | **Lane:** Aider — one new component, three views wired to it, and the CSS.
**No new logic.** Every decision this lane needs already exists as a tested function in
`state/covers.ts`; if something here seems to need a new one, that is the signal to stop and say so
rather than to write it in a component.

**Files to create/modify (five):**
- `app/src/components/CoverPicker.tsx` — **new**
- `app/src/views/Library.tsx` — the picker on a track card, and loading the gallery
- `app/src/components/AlbumPanel.tsx` — the picker on an album row
- `app/src/views/CoverArt.tsx` — Delete on a tile, with its confirm
- `app/src/theme.css`

## Goal

Attach a cover to a track or an album, clear it, and delete an artwork from the gallery — with a
dangling cover rendered as missing and repairable rather than as an error.

## Spec

### 1. `components/CoverPicker.tsx`

Presentational: values in, one callback out. It reads **no store**, because its two callers write
through different ones (`useTrackActionsStore.setCover` and `useAlbumsStore.setCover`) and a store
read inside would make the component pick a side. Same shape as `ProfilePickerRow`.

```tsx
export function CoverPicker({
  view,
  choices,
  disabled,
  onChange,
}: {
  view: CoverView
  choices: { id: string | null; label: string }[]
  disabled?: boolean
  /** `null` clears the cover. */
  onChange: (cover: string | null) => void
})
```

It renders, in this order:

- **The current cover**, from `view.state`:
  - `shown` — an `<img className="cover-thumb">` at `view.url` with `alt={view.name}`, and the name
    beside it. When `view.url` is `null`, or the image fails to load, the `.cover-missing` box
    instead — the same `onError` + local `broken` state `ArtTile` uses, **reset on a change of
    `view`**, for the reason T-506d found the hard way: a flag that never clears leaves a tile
    saying "not found" after the file comes back.
  - `missing` — `.cover-missing` reading `Artwork ${view.id} is no longer in this project.` The row
    keeps everything else; this is the T-403 rule, and the state T-506e-a's non-atomic cover
    clearing can genuinely leave behind.
  - `none` — `.cover-none` reading `No cover`.
- **The select**, unless `choices.length === 1` (only "No cover" — the project has no artwork yet),
  in which case a muted `Generate cover art to use it here.` and no control. An empty dropdown is
  worse than a sentence.

`null` maps to the option value `''` and back — the `''` sentinel `AlbumAddTrack`'s select already
uses. The select's `value` is `view.state === 'none' ? '' : view.id`, so a **missing** cover shows
its id selected-but-absent; that is honest, and picking "No cover" repairs it.

### 2. `views/Library.tsx`

- Load the gallery on mount: `useArtStore((s) => s.load)` in its own effect, beside the existing
  `load` / `startListening` / `loadProjects` ones. **No `startListening` here** — the Library does
  not generate artwork, and Cover Art arms that subscription for the session when it mounts.
- In `TrackCard`, between the `<dl className="track-recipe">` and `<TrackDetails>`:

```tsx
<div className="track-cover">
  <span className="track-cover-label">Cover</span>
  <CoverPicker
    view={coverView(row.cover, art)}
    choices={coverChoices(art)}
    disabled={busy}
    onChange={(cover) => void actions.setCover(row.id, cover)}
  />
</div>
```

`art` is `useArtStore((state) => state.art)`. `actions.setCover` reloads the library itself, so
there is nothing to chain. A failure already renders through the existing `actionError` line, which
`errorFor` scopes to this row.

### 3. `components/AlbumPanel.tsx`

The same control inside `album-row-body`, above `<AlbumTrackList>`, writing through
`useAlbumsStore.setCover`. The albums store returns the refreshed list, so again nothing to chain,
and its `error` already renders at the top of the panel.

### 4. `views/CoverArt.tsx` — Delete on a tile

Two more effects on mount: `useLibraryStore.load()` and `useAlbumsStore.load()`. **This is not
incidental** — `deleteArtPrompt` can only name what a delete will detach if those two stores are
loaded, and the alternative is a confirm dialog that understates what it is about to do. e-b made
the prompt degrade safely when they are empty; this lane makes sure they are not.

`ArtTile` gains, below its facts, the inline confirm the rest of the app uses (`ProjectDelete` is
the model — a Delete button that becomes prompt + Delete + Cancel):

```tsx
const confirming = useArtStore((s) => s.confirmingDelete === row.id)
// ...
{confirming ? (
  <div className="art-delete-confirm">
    <span className="art-delete-prompt">
      {deleteArtPrompt(row.name, coverUsage(row.id, tracks, albums))}
    </span>
    <button className="art-delete-yes" onClick={() => void remove(row.id)}>Delete</button>
    <button className="art-delete-cancel" onClick={() => cancelDelete()}>Cancel</button>
  </div>
) : (
  <button className="art-delete" onClick={() => askDelete(row.id)}>Delete</button>
)}
```

`tracks` is `useLibraryStore((s) => s.tracks)`, `albums` is `useAlbumsStore((s) => s.albums)`.
`remove` reloads all three stores itself (T-506e-b), so the tile disappears and the Library's
thumbnails update without anything here chaining a reload.

The store's `error` already renders at the top of the gallery, and `remove` keeps
`confirmingDelete` set on failure, so a failed delete leaves the confirm open with the message
above it — retry or cancel, no dead end.

### 5. `theme.css`

Append to the existing `/* --- Cover Art (T-506d) --- */` section using only existing tokens:

- `.cover-picker` — a row: thumbnail, then name/sentence, then the select
- `.cover-thumb` — small and square (`48px`, `aspect-ratio: 1`, `object-fit: cover`,
  `border-radius: var(--radius)`)
- `.cover-none`, `.cover-missing` — the same box, muted, `border: 1px dashed var(--border)`;
  `.cover-missing` in the warning colour the Library's `.library-warning` already uses
- `.cover-select` — matching `.album-add-pick`
- `.track-cover`, `.track-cover-label` — following `.track-fact` / its `dt`
- `.art-delete`, `.art-delete-confirm`, `.art-delete-prompt`, `.art-delete-yes`,
  `.art-delete-cancel` — matching the `project-delete-*` set exactly, which is the delete confirm
  this app already has

## Tests

**None.** Everything decidable is already a tested function in `state/covers.ts`, and this repo runs
vitest in `node` with no DOM. That is not a gap being waved through — it is why the click-through
below is the acceptance criterion for this lane, and why it reads the files rather than the screen.

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the listed files
- [ ] **no new exported function anywhere** — if a decision is needed, stop and ask
- [ ] `CoverPicker` reads no store
- [ ] no new CSS custom properties

## Out of scope
- **Choosing a cover at generation time.** A cover is attached afterwards, from artwork that exists.
- **Showing a track's cover in the player or the queue.** T-507's polish sweep, if at all.
- **Reordering or filtering the gallery.**
- **Anything in Rust.** e-a is landed and complete.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/state/covers.ts --read app/src/state/art.ts --read app/src/state/albums.ts --read app/src/state/trackActions.ts --read app/src/state/library.ts --read app/src/components/ProfilePickerRow.tsx --file app/src/components/CoverPicker.tsx --file app/src/views/Library.tsx --file app/src/components/AlbumPanel.tsx --file app/src/views/CoverArt.tsx --file app/src/theme.css
```

## Click-through (producer) — the lane's acceptance

`npm run tauri dev`, with at least two artworks and two tracks in one project. **Read the files, not
only the screen.**

1. **Attach a cover to a track.** Library → a track card → pick an artwork. The thumbnail appears
   without a manual reload.
2. **Check the sidecar.** `tracks/<id>.json` has `"cover": "ar-000N"` — and its whole `provenance`
   block is **unchanged**. *A cover is a pointer, not part of the recipe; that is the rule the field
   is placed to keep.*
3. **Clear it.** Pick "No cover" — the thumbnail goes and the sidecar's `cover` is `null`.
4. **Attach a cover to an album.** Library → Albums → open one → pick an artwork.
   `project.json`'s album gains `"cover"`; the track sidecars are untouched.
5. **Delete an artwork that is in use.** Attach the same artwork to a track *and* an album, then
   Cover Art → its tile → Delete. **The confirm names both**: "It is the cover for 1 track and 1
   album."
6. **Confirm, and watch three views settle.** The tile goes; back in the Library the track's
   thumbnail is gone and the album's is too — **with no manual reload**. *That is what `remove`'s
   three store reloads buy; one skipped would leave a cover on screen that is gone from disk.*
7. **Check the disk after the delete.** The image and its sidecar are in the **Recycle Bin, not
   erased**; the track sidecar's `cover` is back to `null`; `project.json`'s album cover is `null`.
8. **Cancel is a real cancel.** Arm a delete on another tile, press Cancel: the confirm closes and
   nothing on disk changed.
9. **A dangling cover renders as missing and is repairable.** Hand-edit a track sidecar to
   `"cover": "ar-9999"`, reopen the Library: the row reads *"Artwork ar-9999 is no longer in this
   project."*, **everything else on the card still renders**, and picking "No cover" fixes it.
   *This is the state T-506e-a's non-atomic cover clearing can leave, named there so it would be
   designed for rather than discovered.*
10. **An unused artwork's confirm has no counts sentence** — just the rule.
11. **Ids are still not reused.** Generate one more cover after a delete: it is `ar-0003`, not the
    freed `ar-0002`.
12. **Project switch.** Switch projects in the Library and back: covers follow their project, and
    no thumbnail from the other one appears.

Report which of the twelve passed. A bare "passed" is read as all twelve.
