# T-506e-a: the cover backend — attach, detach, and delete an artwork

**Depends:** T-506a (`ArtId`, `Artwork`, `library::art`), T-506d (cover art works end to end)
**Dir:** `crates/`, `src-tauri/` | **Lane:** Aider — two struct fields, three library functions, three
Tauri commands, and the tests for all of it. **No frontend.**

**T-506e is split in two**, for the reason T-506c was: the stores need a schema change and three
commands that do not exist, and mixing Rust into a store lane is what the T-504 → T-505a cadence
exists to avoid. **e-a is this brief; e-b is the frontend**, briefed after this lands.

**Files to modify (nine, plus registration):**
- `crates/create-core/src/provenance.rs` — `Track.cover`
- `crates/create-core/src/project.rs` — `AlbumList.cover`
- `crates/library/src/tracks.rs` — `set_track_cover`, `trash_if_present` made `pub(crate)`
- `crates/library/src/albums.rs` — `set_album_cover`
- `crates/library/src/art.rs` — `delete_art`
- `crates/library/src/lyrics.rs` — one test `Track` literal
- `src-tauri/src/ingest.rs` — one `Track` literal
- `src-tauri/src/sendto.rs` — one test `Track` literal
- `src-tauri/src/tracks.rs`, `src-tauri/src/albums.rs`, `src-tauri/src/art.rs` — the three commands
- `src-tauri/src/lib.rs` — registration

## Goal

A track or an album can name one artwork as its cover, and an artwork can be deleted without
stranding anything.

## Spec

### 1. The two fields

```rust
// create_core::provenance::Track, beside `title` -- NOT inside `Provenance`.
/// The artwork shown with this track, when the user has chosen one.
///
/// **Not provenance.** `Provenance` records what *made* the asset and is never
/// rewritten; a cover is an editable pointer the user changes whenever they
/// like, which is why it sits beside `title` rather than inside the recipe.
/// Nothing about reproducing this track depends on it.
///
/// It lives in the sidecar because the sidecar is the one source of truth for a
/// track (ARCHITECTURE 8) -- `project.json` holds only the id.
#[serde(default)]
pub cover: Option<ArtId>,
```

```rust
// create_core::project::AlbumList, after `tracks`.
/// The artwork shown for this list, when the user has chosen one.
///
/// An album has no file of its own (T-403), so unlike a track's cover this one
/// lives in `project.json` with the rest of the list. Same rule otherwise.
#[serde(default)]
pub cover: Option<ArtId>,
```

Both are `#[serde(default)]`, so every sidecar and `project.json` written before today still loads.
Six `Track { .. }` literals and three `AlbumList { .. }` literals need the new field — the file list
above is complete; if a tenth turns up, it belongs in the list, not in a guess.

### 2. `library::tracks::set_track_cover`

```rust
/// Set or clear a track's cover, returning the updated record.
///
/// Modelled on [`rename_track`]: the sidecar is the single source of truth, so
/// this rewrites the sidecar and nothing else. `None` clears it.
///
/// **The id is checked against the project.** Adding is the one moment a
/// dangling reference can be prevented -- the rule `albums::add_track` already
/// states -- so a cover naming an artwork this project does not own is refused
/// rather than written and rendered as missing later.
pub fn set_track_cover(
    root: &Path,
    slug: &str,
    id: &TrackId,
    cover: Option<&ArtId>,
) -> Result<Track, LibraryError>
```

An `ArtId` the project does not list is `LibraryError::NotFound { kind: "artwork", .. }`.

### 3. `library::albums::set_album_cover`

```rust
/// Set or clear an album's cover, returning the project's albums.
///
/// Albums are name-addressed (T-403); an unknown name is `NotFound`, never a
/// silent no-op. The artwork id is checked against the project for the same
/// reason `add_track` checks a track id.
pub fn set_album_cover(
    root: &Path,
    slug: &str,
    album: &str,
    cover: Option<&ArtId>,
) -> Result<Vec<AlbumList>, LibraryError>
```

### 4. `library::art::delete_art` — and the rule it follows

```rust
pub fn delete_art<F>(root: &Path, slug: &str, id: &ArtId, trash: F) -> Result<(), LibraryError>
where
    F: Fn(&Path) -> Result<(), LibraryError>,
```

**A cover reference does not block the delete. It is cleared.** This is the opposite of
`lyrics::delete_doc`, and the difference is the point:

- `delete_doc` **refuses** when a track's provenance points at the document, because a `LyricRef` is
  part of the recipe. Deleting the document would leave a track whose sidecar names lyrics nobody
  can show — the refusal protects a record that must stay reproducible.
- `delete_track` **clears** the id from every album that holds it, because an album is the user's
  current arrangement, not a record of how anything was made.

A cover is the second kind. It is an editable pointer, like `title`; nothing about reproducing the
track depends on it, and no record becomes unreadable when it goes. Refusing would mean a user has
to detach a bad cover from every track and album before they can delete it — friction bought with no
protection. *(phase-5.md's original note guessed the `tracks_referencing` shape for this; reading
both precedents says otherwise, and the note has been corrected.)*

The body, in order, following `delete_track` line for line:

1. Load the project; an id it does not list is `NotFound { kind: "artwork", .. }`.
2. Load the sidecar to learn the image filename, and trash the image. **A sidecar that will not
   load is tolerated** — the record is still cleaned and at worst one orphan image is left for a
   degraded artwork the user is deleting anyway.
3. Trash the sidecar.
4. Clear the cover on every track sidecar naming it, and on every album naming it.
5. Remove the id from `Project::art`; save the project once.
6. **`next_art_seq` is untouched**, so the freed id is never reissued and a surviving cover
   reference can never come to mean a different image.

**Order: files first, record last, missing files tolerated** — the same reasoning `delete_track`
gives. A crash after trashing leaves the project listing an artwork whose files are gone, and a
retry completes cleanly because the trash step skips what is already missing; the reverse order
would strand files nothing references with no id left to retry.

Say plainly in the doc comment what step 4 cannot promise: it is **N separate atomic writes, not one
transaction**, so a crash part-way leaves some tracks with no cover and some naming a deleted one.
Both are states the view has to render anyway, which is why this is tolerable rather than hidden.

Reuse `tracks::trash_if_present` — make it `pub(crate)` rather than writing a second copy. It is
three lines, and the comment on it (`trash::delete` canonicalizes first and errors on a missing
path) is the reason it exists.

### 5. The three Tauri commands

`set_track_cover` in `src-tauri/src/tracks.rs` beside `rename_track`; `album_set_cover` in
`albums.rs` beside `album_add_track`; `delete_art` in `art.rs`, passing
`library::tracks::trash_to_os` exactly as `delete_track` does. All three resolve the project through
`crate::projectctx::selected_project` and map errors with `.to_string()`, like every command around
them. `cover: Option<String>` on the wire, `None` meaning clear.

Register all three in `lib.rs`. No tests in `src-tauri` for these three — they take Tauri `State`,
which no test in the crate builds, the same rule `tracks.rs` and `art.rs` already follow.

## Tests — named by the invariant

`provenance.rs`:
- **a track sidecar written before covers existed still loads, with `cover: None`** — strip the key
  from real JSON and assert, the shape `test_an_artwork_sidecar_loads_without_its_optional_fields`
  uses. Not a round-trip: a round-trip of a fully-populated struct cannot fail unless a derive is
  removed.

`project.rs`:
- **an album written before covers existed still loads, with `cover: None`.**

`tracks.rs`:
- **setting a cover rewrites only the sidecar** — `project.json` is byte-identical afterwards.
- **setting a cover leaves the provenance untouched** — assert the whole `provenance` equals what it
  was. *The one invariant that makes putting `cover` on `Track` safe: if this can fail, the field is
  in the wrong struct.*
- **a cover naming an artwork the project does not own is refused** — nothing is written.
- **`None` clears a cover that was set.**

`albums.rs`:
- **an unknown album name is `NotFound`, not a silent no-op.**
- **an unowned artwork id is refused.**

`art.rs`:
- **deleting an artwork trashes both its files and unlists the id.**
- **a track whose cover names it loses the cover; a track naming a *different* artwork is
  untouched.** *The twin of `test_a_track_referencing_another_version_does_not_block` — a
  clear-everything bug passes the first half of this and fails the second.*
- **an album whose cover names it loses the cover.**
- **an unknown id is `NotFound` and trashes nothing** — assert the fake trasher was never called.
- **a missing image file is tolerated** — the sidecar still goes and the id still unlists.
- **the id is not reissued after a delete** — mint again and assert it differs, the shape
  `test_mint_art_id_never_reuses_a_deleted_id` already uses.

Every delete test passes a **fake trasher**. `cargo test` must never fill the developer's Recycle
Bin (T-405/T-408, CONVENTIONS).

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the listed files
- [ ] `Provenance` is unchanged — `cover` is on `Track`, not in the recipe
- [ ] `delete_art` and `delete_track` read as the same procedure; `trash_if_present` has one copy
- [ ] no test calls `trash_to_os`

## Out of scope
- **Any frontend.** No bridge, no store, no view, no CSS — T-506e-b.
- **Rendering a dangling cover.** Step 4's non-atomicity means a track can name a deleted artwork;
  T-506e-b must render that as a missing cover rather than an error, the way T-403 renders a missing
  track. Named here so it is designed rather than discovered.
- **More than one cover per track or album.** One is what a cover is.
- **A cover on a project.** Not asked for, and an album is the thing that has artwork.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/library/src/lyrics.rs --read crates/library/src/projects.rs --read crates/library/src/lib.rs --read src-tauri/src/lib.rs --file crates/create-core/src/provenance.rs --file crates/create-core/src/project.rs --file crates/library/src/tracks.rs --file crates/library/src/albums.rs --file crates/library/src/art.rs --file crates/library/src/lyrics.rs --file src-tauri/src/ingest.rs --file src-tauri/src/sendto.rs --file src-tauri/src/tracks.rs --file src-tauri/src/albums.rs --file src-tauri/src/art.rs --file src-tauri/src/lib.rs
```
