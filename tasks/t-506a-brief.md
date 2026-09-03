# T-506a: the artwork record and its storage

**Depends:** T-505d (an image profile exists and generates) | **Crates:** `create-core`, `library`
**Lane:** Aider — broad mechanical mirroring of an existing module across two crates, which is
exactly what the executor exists for (WORKFLOW §1). The decided parts are pre-written below;
transcribe them, then mirror `tracks.rs` for the rest.

**Files to create/modify (six, nothing else):**
- `crates/create-core/src/project.rs` — `ArtId`, `Project.art`, `Project.next_art_seq`
- `crates/create-core/src/provenance.rs` — `Artwork`
- `crates/create-core/src/image.rs` — **new**, `png_dimensions` (pure, no I/O)
- `crates/create-core/src/lib.rs` — `pub mod image;`
- `crates/library/src/art.rs` — **new**, the on-disk half
- `crates/library/src/lib.rs` — `pub mod art;` + the two re-exports

## Goal

A generated image can be recorded the way a generated track is: `art/<id>.png` beside
`art/<id>.json`, the sidecar holding the whole `Artwork` record including its full `Provenance`,
and `project.json` holding **ids only**. Nothing here talks to ComfyUI and nothing here draws a
screen — this is the storage T-506b's ingest writes into and T-506d's view reads back.

## Why it is a task rather than a detail

`src-tauri/src/ingest.rs` files an output only when its extension is in `AUDIO_EXTS`, and returns
`None` for anything else **without an error**. An image job run through the app today completes,
reports Done, and saves nothing anywhere (docs/MCP-SURFACE.md §35.4). The generation half already
works untouched (§35.1, §35.5) — everything missing is on this side of the output.

## Spec

### 1. `create-core::project`

```rust
/// Stable id for one generated artwork, e.g. `"ar-0001"`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArtId(pub String);
```

Declare it exactly like `TrackId` — same derives, same position in the file.

Two new `Project` fields, both defaulted so **every `project.json` on disk today still loads**:

```rust
    /// Artwork generated in this project. Ids only, same rule as `tracks`:
    /// every fact about an artwork lives in its sidecar (ARCHITECTURE 8).
    #[serde(default)]
    pub art: Vec<ArtId>,
    /// Sequence number the next artwork in this project will be minted from.
    ///
    /// Monotonic and **never reused**, for the same reason `next_track_seq` is:
    /// a freed id handed to a later artwork would silently re-point whatever
    /// referenced the old one.
    #[serde(default = "default_art_seq")]
    pub next_art_seq: u32,
```

plus `fn default_art_seq() -> u32 { 1 }` beside the other two, and `art: Vec::new(), next_art_seq: 1`
in `Project::new`. There are **no other** `Project` struct literals in the workspace (checked), so
nothing else needs touching.

### 2. `create-core::provenance`

```rust
/// One generated image: the contents of `art/<id>.json`, the sidecar that is the
/// single source of truth for this artwork (ARCHITECTURE 8).
///
/// `Provenance` is reused verbatim rather than forked. It records a *generated
/// asset* -- profile, licence, the spec the user chose, the resolved slots the
/// engine received, the server, the prompt id -- and none of that is audio. A
/// second near-identical struct would be two things to keep in step for no gain,
/// and the "re-use these settings" path (T-406) already reads this shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artwork {
    /// Artwork id, minted by `library`.
    pub id: ArtId,
    /// User-facing title, if the user has set one.
    #[serde(default)]
    pub title: Option<String>,
    /// Path relative to the project directory, e.g. `"art/ar-0001.png"`.
    pub file: String,
    /// Pixel size, when it could be read from the file's header.
    #[serde(default)]
    pub width: Option<u32>,
    /// Pixel size, when it could be read from the file's header.
    #[serde(default)]
    pub height: Option<u32>,
    /// Full generation recipe for this image.
    pub provenance: Provenance,
}
```

`width`/`height` are `Option` and defaulted for the same reason `Track::duration_s` is: a size that
could not be read must never stop the artwork being recorded.

### 3. `create-core::image` — new module

Mirrors `audio.rs`: a pure header parser, no I/O, so it is testable without a file. **PNG only.**
ComfyUI's `SaveImage` writes PNG, which is what the pipeline produces; a JPEG or WebP parser would
be code with no caller. Anything that is not a PNG returns `None` rather than guessing.

```rust
//! Image file introspection. Pure functions only -- no I/O.

/// Pixel size, read from a PNG's IHDR chunk.
///
/// `None` for anything that is not a PNG, and for a header claiming a zero
/// dimension -- which no real image has, and which would otherwise be recorded
/// as a fact.
///
/// Only the first 24 bytes are needed and their position is fixed: IHDR is
/// mandatory and must be the first chunk, so it is 8 bytes of magic, a 4-byte
/// length, 4 bytes of `IHDR`, then width and height as big-endian `u32`s.
pub fn png_dimensions(head: &[u8]) -> Option<(u32, u32)> {
    const MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
    if head.len() < 24 || &head[..8] != MAGIC || &head[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([head[16], head[17], head[18], head[19]]);
    let height = u32::from_be_bytes([head[20], head[21], head[22], head[23]]);
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}
```

**Fixture, already committed:** `testdata/images/klein-cover.png.head` — the first 33 bytes of a
real cover generated by the adopted Klein profile on 2026-09-03 (768x768, 8-bit truecolour).

Tests, each named by the invariant it protects — mirror `audio.rs`'s set, which is the model:

- **the real generated header decodes to the size the file actually is** — `Some((768, 768))`.
- **a truncated header is rejected rather than indexed past the end** — `&head[..20]` is `None`.
- **the magic bytes are actually checked** — take the real header, assert it is valid first, then
  change *only* the 8 magic bytes and assert `None`. (`audio.rs`'s comment explains why the fixture
  must otherwise stay perfect: a too-short forgery is rejected by the length guard before the magic
  comparison ever runs, so the test would still pass with the magic check deleted.)
- **the `IHDR` marker is checked too** — same shape, corrupting only bytes 12..16.
- **a zero dimension is `None`, not `Some(0)`** — zero bytes 20..24.

### 4. `library::art` — new module

Mirror `crates/library/src/tracks.rs` structurally: same doc-comment style, same guards, same
ordering rules. Constants:

```rust
/// Directory inside a project holding its images and sidecars.
pub const ART_DIR: &str = "art";
/// Prefix every minted artwork id carries.
const ID_PREFIX: &str = "ar-";
/// Digits a minted id is padded to, so name order matches creation order.
const ID_DIGITS: usize = 4;
```

Public surface — each one the `tracks.rs` function of the same name with `TrackId`/`Track`/
`tracks_dir` swapped for the artwork equivalents:

| function | notes |
|---|---|
| `art_dir(root, slug) -> Result<PathBuf, LibraryError>` | `project_dir(root, slug)?.join(ART_DIR)` |
| `sidecar_path(root, slug, id)` | refuses an unsafe id with `LibraryError::UnusableName` |
| `image_path(root, slug, id, ext)` | the same id guard **and** the same extension guard `audio_path` uses: lowercased, non-empty, ASCII-alphanumeric only |
| `resolve_art_file(root, slug, file)` | the `resolve_track_file` guard verbatim — a hand-edited sidecar must not be able to name an absolute path or walk out with `..`, because this path is handed to the webview to display |
| `mint_art_id(project) -> ArtId` | reads and advances `next_art_seq` with `saturating_add`; the counter is the **only** source of ids |
| `save_art(root, slug, artwork)` | `atomic::write_json` to the sidecar path |
| `load_art(root, slug, id) -> Result<Artwork, _>` | `NotFound { kind: "artwork sidecar", .. }` on a missing file, `Io` otherwise |
| `list_art(root, project) -> ArtSet` | **never fails**; driven by `project.art`, not by the directory |
| `dimensions_of(path) -> Option<(u32, u32)>` | opens the file, `read_exact` into `[0u8; 24]`, hands it to `png_dimensions` |

`ArtWarning` / `ArtSet` mirror `TrackWarning` / `TrackSet` exactly — `Missing`, `Unreadable`,
`Malformed`, the same serde tagging, and the same rule that an id with nothing behind it becomes a
warning rather than a silent omission.

**`dimensions_of` uses `read_exact`, not `read`** — the reason `duration_of` does: a single `read`
may legally return fewer bytes than asked for, and a short read on a good file would then be
indistinguishable from a truncated one.

### 5. `library::lib`

`pub mod art;` in alphabetical position (before `albums`), and re-export `art::ArtSet` and
`art::ArtWarning` beside the existing re-exports, in their doc-comment form.

## Tests to write in `library::art`

Named by invariant, not by mechanics (WORKFLOW §4.2 — "would this fail if the thing it guards were
broken?"). Mirror the equivalent `tracks.rs` tests:

- **a minted id is padded and the counter advances** — two mints from a fresh project give
  `ar-0001` then `ar-0002`, and `next_art_seq` is 3.
- **ids are never reused after the list shrinks** — remove an id from `project.art` by hand, save,
  reload, mint: the new id is *not* the removed one. This is the rule that stops a later T-506e
  cover reference from silently re-pointing at a different image.
- **a project written before this task still loads** — deserialize a `project.json` with **no**
  `art` and **no** `next_art_seq` key, and assert `art == []` and `next_art_seq == 1`. This is the
  serde-default guarantee, and the one test whose absence would be found by a user rather than by
  the suite.
- **an id that could escape the project is refused** — `sidecar_path` and `image_path` on
  `"../../etc/passwd"` and on `"ar-0001/x"` both return `UnusableName`.
- **an extension that could escape is refused** — `image_path(.., "pn/g")` and `image_path(.., "")`.
- **`resolve_art_file` refuses an absolute path and one containing `..`**, and accepts
  `"art/ar-0001.png"`.
- **a save/load round-trip preserves the whole record**, provenance included.
- **`list_art` returns them in `project.art` order, and reports a missing sidecar as a warning
  rather than dropping the id.**
- **`dimensions_of` reads a real PNG** — write `testdata/images/klein-cover.png.head` to a temp file
  and assert `Some((768, 768))`; and it returns `None` for a file too short to hold a header.

## Acceptance criteria
- [ ] `cargo test -p create-core -p library` green; `npm run gate` green
- [ ] no changes outside the six listed files (the fixture is already committed)
- [ ] `Project` gains **only** `art` and `next_art_seq`, both defaulted; no existing field changes
- [ ] no new dependency in either crate — the PNG header is parsed by hand, exactly as the FLAC
      header is

## Out of scope — deliberately, each with its reason

- **`delete_art`.** Deleting an artwork has to decide what happens when a track or album uses it as
  a cover, and that reference does not exist until **T-506e** — which is where it lands, with the
  T-408 rule (to OS trash, injected as a parameter so `cargo test` never fills the developer's
  Recycle Bin) and the `tracks_referencing` shape. Writing it now would be writing the easy half of
  a decision.
- **`rename_art` and titles.** `Artwork.title` exists as a field so the sidecar shape is settled;
  nothing sets it in this lane.
- **Anything in `src-tauri`** — no command, no ingest, no event. That is T-506b.
- **`Track.cover`** — T-506e.
- **Image size as a control.** Deferred with its evidence (MCP-SURFACE §35.2, §35.3): on Klein every
  size-shaped slot is inert and the effective address is a `PrimitiveInt`, so a size role is a
  role-suggestion problem, not a storage one.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/library/src/tracks.rs --read crates/library/src/projects.rs --read crates/library/src/atomic.rs --read crates/create-core/src/audio.rs --file crates/create-core/src/project.rs --file crates/create-core/src/provenance.rs --file crates/create-core/src/image.rs --file crates/create-core/src/lib.rs --file crates/library/src/art.rs --file crates/library/src/lib.rs
```
