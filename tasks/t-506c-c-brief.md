# T-506c-c: the artwork listing store

**Depends:** T-506c-a (`library_art` and `art_image_path` are registered), T-506a (`ArtSet`,
`ArtWarning`, `Artwork` exist in Rust)
**Dir:** `app/src` | **Lane:** Aider — one new bridge, one new store, one small extraction so the
new store does not become the third copy of four functions, and the two reload call sites that
already exist for tracks.

**Files to create/modify (six, plus four test files):**
- `app/src/state/provenance.ts` — **new**, four helpers extracted from `library.ts` unchanged
- `app/src/state/provenance.test.ts` — **new**
- `app/src/bridge/art.ts` — **new**, the types, `listArt`, `artImageUrl`, `subscribeArt`
- `app/src/state/art.ts` — **new**, the gallery store
- `app/src/state/art.test.ts` — **new**
- `app/src/state/library.ts` — imports the four helpers instead of defining them
- `app/src/state/library.test.ts` — the moved imports
- `app/src/views/Library.tsx` — one call site, `provenanceView(track)` → `provenanceView(track.provenance)`
- `app/src/state/projects.ts` — the art store reloads on project switch and project delete
- `app/src/state/projects.test.ts` — a `../bridge/art` mock

## Goal

Cover Art (T-506d) has a list to render: artwork rows over `library_art`, an asset URL per artwork
over `art_image_path`, warnings that name what could not be read, and the `art://saved`
subscription that makes a finished cover appear without a reload. No UI in this lane.

## Spec

### 0. `state/provenance.ts` — the extraction, first, and verbatim

`state/library.ts` today defines five functions that read **only** `track.provenance`:
`trackModel`, `seedValue`, `createdDate`, `formatValue`, `provenanceView`. The artwork store needs
every one of them, and `library.ts`'s own comment on `trackModel` already says `state/queue.ts`'s
`modelName` is a "deliberate twin ... the absent-versus-empty rule and the fallback chain are the
same". A third copy is where that rule drifts, and the copy would drift in the sidecar the user
reads to prove where a file came from.

Create `app/src/state/provenance.ts` holding them, keyed on `Provenance` rather than `Track`:

```ts
import type { Provenance } from '../bridge/library'
import type { InputValue } from './params'

/**
 * What a row calls the model that produced it.
 *
 * Named `modelLabel` rather than `modelName` because `state/queue.ts` has a
 * `modelName` with the same fallback chain and a different input -- a queued
 * job has no provenance yet, only a profile id. Two names, so a reader can tell
 * which side of the run they are on.
 */
export function modelLabel(p: Provenance): string

/** The seed as text, or `--`. `inputs.seed` is the tagged value, not a number. */
export function seedText(p: Provenance): string

/** The date half of the RFC 3339 stamp. Never parsed into a `Date`. */
export function createdDate(p: Provenance): string

export interface ProvenanceFact { label: string; value: string }
export interface ProvenanceSection { title: string; facts: ProvenanceFact[] }

/** The full sidecar as inspector sections (T-406). */
export function provenanceView(p: Provenance): ProvenanceSection[]
```

**Move the bodies unchanged.** `trackModel(track)` becomes `modelLabel(p)` with `track.provenance`
replaced by `p` and nothing else touched; same for the other four. `formatValue` moves with them and
stays module-private — its doc comment (`v.value`, never `String(v)`) is the whole reason it exists.
The doc comments move with their functions; do not rewrite them.

**What stays in `library.ts`:** `EMPTY_LIBRARY`, `TrackRow`, `trackName`, `formatDuration`,
`loraStack`, `trackRows`, `warningLine`, `LibraryState`, `useLibraryStore` — every one of them
unchanged in name and signature. `library.ts` re-exports `ProvenanceFact`, `ProvenanceSection` and
`provenanceView` so `Library.tsx`'s existing import line keeps resolving; only the **argument** at
`Library.tsx:369` changes, from `track` to `track.provenance`.

`trackRows` now reads `modelLabel(track.provenance)`, `seedText(track.provenance)` and
`createdDate(track.provenance)`. Its output is byte-identical to what it produces today, which is
what makes this extraction safe to land in the same commit as a new store.

### 1. `bridge/art.ts`

Mirrors `bridge/library.ts`, and **reuses `Provenance` from it** rather than redeclaring it — the
Rust `Artwork` embeds `create_core::provenance::Provenance` verbatim, so a second TS copy would be a
second thing to keep in step with a struct that already has one mirror.

```ts
/** Mirrors Rust `create_core::provenance::Artwork`. */
export interface Artwork {
  id: string
  title: string | null
  /** Relative to the project directory, e.g. `art/ar-0001.png`. */
  file: string
  /** Pixel size read off the file's own header; `null` when unreadable. */
  width: number | null
  height: number | null
  provenance: Provenance
}

/** Mirrors Rust `library::art::ArtSet`. */
export interface ArtSet { art: Artwork[]; warnings: ArtWarning[] }

/** Mirrors Rust `library::art::ArtWarning`. */
export type ArtWarning =
  | { kind: 'missing'; id: string }
  | { kind: 'unreadable'; id: string; detail: string }
  | { kind: 'malformed'; id: string; detail: string }

/** Payload of `art://saved`. Mirrors Rust `jobs::ArtSaved`. */
export interface ArtSaved { id: string; project_slug: string; file: string }

/** List every artwork in the selected project, with warnings for bad sidecars. */
export async function listArt(): Promise<ArtSet>

/**
 * Resolve an artwork id to a URL the webview can display.
 *
 * The twin of `bridge/player.ts`'s `trackAudioUrl`, and for the same reason both
 * halves live here: the backend returns an absolute path (`art_image_path`,
 * which validates the id and the stored `file`), and `convertFileSrc` turns it
 * into an asset URL. **Resolving is not checking** -- the backend refuses a path
 * that escapes the project, but it does not stat the file, so a URL from here is
 * not a promise that an image is behind it. The view handles `onError` (T-506d).
 */
export async function artImageUrl(id: string): Promise<string>

/**
 * Subscribe to `art://saved`.
 *
 * Re-load on every save rather than appending, exactly as `subscribeTracks`
 * does: the event carries id, slug and file, not the provenance a row needs.
 */
export async function subscribeArt(onSaved: (e: ArtSaved) => void): Promise<UnlistenFn>
```

### 2. `state/art.ts`

```ts
/** Shown in place of the grid when nothing has been generated yet. */
export const EMPTY_ART = 'Cover art you generate will appear here, with the recipe that made it.'

/** One tile of the gallery, with every decision already made. */
export interface ArtRow {
  id: string
  /** The user's title, else the id -- never empty. */
  name: string
  model: string
  license: string
  /** `768 x 768`, or `--` when the header could not be read. */
  size: string
  created: string
  seed: string
  promptId: string | null
  file: string
  /**
   * The asset URL, or `null` when it could not be resolved. `null` is a tile
   * that says so; it is not a reason to drop the artwork from the gallery.
   */
  url: string | null
}

/** Map an `ArtSet` and the resolved URLs to the tiles Cover Art will render. */
export function artRows(set: ArtSet, urls: Record<string, string>): ArtRow[]

/** A single sentence describing warnings, or `null`. Never a modal. */
export function artWarningLine(warnings: ArtWarning[]): string | null
```

`artRows` is pure and takes the URL map as a parameter, rather than the store resolving inside a
component: this repo runs vitest in `node` with no DOM, so a URL fetched in a tile's effect is a URL
no test can reach (T-301b, and `profileRow`'s comment says the same thing). The store awaits, the
selector maps, the view renders.

`artWarningLine` is its own sentence — `N artwork sidecars could not be read; check the files in
your project's art folder.` — beside `warningLine` and `projectWarningLine`, which is already the
established shape: one sentence per domain, naming the folder to open.

`size` is `${width} x ${height}` when **both** are present, else `--`. A missing size is a header
that could not be read, never a reason to hide the artwork; `library::art::dimensions_of` returns
`None` for exactly that and the record is written anyway.

The store:

```ts
interface ArtState {
  art: ArtRow[]
  /** The raw artworks by id, for the provenance inspector and T-506e's attach. */
  byId: Record<string, Artwork>
  warnings: string | null
  loading: boolean
  error: string | null
  listening: boolean
  load: () => Promise<void>
  startListening: () => Promise<void>
}
```

`load` lists, then resolves one URL per artwork, then sets. Two rules with reasons:

- **Resolve the URLs with `Promise.all`, and catch per artwork.** The sequential-`await` rule that
  `artGenerate.ts` carries is about the one stdio transport to comfy-mcp and about `submittedAt`
  ordering the queue; neither applies to a path lookup, and a gallery of twenty tiles should not be
  twenty round trips deep. Each promise catches its own failure to `null`, so the combined promise
  never rejects: **one unreadable sidecar must not blank the whole gallery**, which is the same rule
  `list_art` follows on the Rust side by returning a warning instead of an error.
- **Do not cache URLs across loads.** Art ids are per-project — every project has an `ar-0001` — so
  a map keyed by id and kept across a project switch would show the previous project's cover under
  the new project's artwork. Re-resolving is cheap; a wrong image beside the wrong provenance is a
  lie about where a file came from.

`startListening` mirrors `useLibraryStore`'s exactly: `listening` guard, `isTauri()` guard, then
`subscribeArt(() => { void get().load() })`. It is called by the view (T-506d), the way
`Library.tsx` calls the track one.

### 3. `state/projects.ts` — the two reloads that already exist for tracks

`select` and `deleteProject` both call `useLibraryStore.getState().load()` today, with a comment
saying the track list belongs to the selected project. Artwork belongs to it in exactly the same
way. Add `await useArtStore.getState().load()` beside each, and extend the existing comments rather
than writing new ones — `// The track list and the artwork both belong to the selected project ...`.

Without this, switching projects leaves the gallery showing the previous project's covers, and it is
worse than a stale list: the resolved URLs still point at the other project's files, so the tiles
would render real images from a project the user is no longer in.

## Tests — named by the invariant

`provenance.test.ts` — the moved tests. Take the `provenanceView` and model-name cases out of
`library.test.ts` and rework them onto `Provenance` (the existing `makeTrack` helper can stay in
`library.test.ts` and the new file build a provenance directly). Nothing about the assertions
changes; if a moved test needs its assertion edited to pass, the move was not verbatim.

`art.test.ts` — mock `../bridge/art` (`listArt`, `artImageUrl`, `subscribeArt`) and
`../bridge/jobs` (`isTauri: () => false`), the way `library.test.ts` mocks its pair:

- **a row carries the title, else the id** — an untitled artwork is not a blank tile.
- **the size reads off the record, and a missing one is `--`** — assert both `width` and `height`
  absent, and one of the two absent, because `768 x null` on a tile would look like a real size.
- **every artwork gets a URL, and one that fails to resolve leaves that tile's `url` null while the
  rest still have theirs** — *the reason the per-artwork catch is there: without it one hand-edited
  sidecar empties the gallery.*
- **a malformed sidecar is a warning and the readable artwork still lists** — the Rust half already
  guarantees the shape; this is the frontend half of it.
- **no warnings is `null`, not an empty sentence.**
- **`load` twice re-resolves the URLs** — assert `artImageUrl` was called again for an id already
  loaded. *The anti-cache test: it is what stops a project switch from showing the previous
  project's cover.*
- **a listing failure sets `error` and clears `loading`**, and does not throw.
- **`startListening` subscribes once**, however many times it is called.

`projects.test.ts`:

- **switching projects reloads the artwork, not just the tracks** — assert `listArt` was called by
  `select`. Same for `deleteProject`.

`library.test.ts`: unchanged apart from imports and the cases that moved. **Its remaining tests must
pass without edits** — `trackRows`'s output does not change in this lane, and any assertion that
needs adjusting means the extraction changed behaviour.

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the listed files
- [ ] `library.ts` still exports `EMPTY_LIBRARY`, `TrackRow`, `trackRows`, `warningLine`,
      `useLibraryStore`, `ProvenanceFact`, `ProvenanceSection` and `provenanceView`, with the same
      names and signatures except `provenanceView`'s single argument
- [ ] the four extracted bodies are unchanged apart from `track.provenance` becoming `p`
- [ ] `Library.tsx`'s import line is untouched; only the argument at its `provenanceView` call changes

## Out of scope
- **Any view or component** beyond the one-argument edit in `Library.tsx`. No `CoverArt.tsx`, no
  grid, no `theme.css` — T-506d.
- **A provenance inspector for artwork.** `provenanceView` now accepts one, and `byId` holds the
  records; wiring it to a disclosure is the view's lane.
- **`delete_art`, rename, export, reveal.** No artwork mutations exist yet — T-506e.
- **Checking that an artwork's image file is still on disk.** `art_image_path` resolves rather than
  stats, deliberately; a missing file is an `onError` in T-506d, not a new Rust command here.
- **`Track.cover`.** T-506e.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/bridge/player.ts --read app/src/state/params.ts --read app/src/state/queue.ts --read app/src/bridge/jobs.ts --file app/src/bridge/library.ts --file app/src/bridge/art.ts --file app/src/state/provenance.ts --file app/src/state/provenance.test.ts --file app/src/state/art.ts --file app/src/state/art.test.ts --file app/src/state/library.ts --file app/src/state/library.test.ts --file app/src/views/Library.tsx --file app/src/state/projects.ts --file app/src/state/projects.test.ts
```
