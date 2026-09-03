# T-506b: `generate_image` and the art ingest

**Depends:** T-506a (the artwork record exists) | **Crate:** `src-tauri`
**Lane:** Aider — mechanical work across three files with one shared rename, and every hard
decision is pre-made below.

**Files to modify (four, nothing else):**
- `src-tauri/src/ingest.rs` — the pending record learns its kind; art ingest beside track ingest
- `src-tauri/src/jobs.rs` — the pump dispatches on kind and emits `art://saved`
- `src-tauri/src/generate.rs` — `generate_image`, the kind guard, one shared queue function
- `src-tauri/src/lib.rs` — register the new command

## Goal

A generation queued from an image profile ends as `art/<id>.png` with a provenance sidecar and an
`art://saved` event, exactly as an audio generation ends as a track. And a profile queued by the
wrong command is refused with a sentence, rather than running to completion and saving nothing.

## Why it is a task rather than a detail

`ingest_one_file` returns `None` for any extension not in `AUDIO_EXTS`, **without an error**. An
image job run through the app today completes, reports Done, emits no event and writes no file
anywhere — the failure is entirely silent (docs/MCP-SURFACE.md §35.4). Everything ahead of that
point already works: verified live on 2026-09-03 by driving the adopted Klein profile through the
same sequence `build_and_submit` performs, where the prompt, negative, seed and steps writes all
reached the engine, the lossless swap was a clean no-op, and a 768x768 PNG came back in 22 s
(§35.1, §35.5).

**One thing that would otherwise have to be assumed, and was checked instead:** `audit_slots` over
the frozen Klein graph and the emitted profile's five addresses reports **nothing unchecked and
nothing inert** — `75/73.noise_seed` and `75/74.text` are `Boundary`-fed (a promoted subgraph
widget, which is a real write target), the other three carry no link. So `build_and_submit`'s
inert-slot refusal does not fire for an image profile, and the pipeline test below is safe to
require.

## Spec

### 1. `ingest.rs` — the pending record learns its kind

**Rename `PendingTrack` to `PendingOutput`** (10 references across `ingest.rs`, `generate.rs` and
`jobs.rs`; all mechanical) and give it one new field:

```rust
    /// Which kind of asset this job's outputs become, taken from the profile at
    /// submit time. The *record* decides, not the file extension: an image job
    /// that somehow emits a `.flac` must not quietly become a track.
    pub kind: ModelKind,
```

The name changes because the struct no longer describes a track. Everything else about it is
unchanged — the fields were already asset-agnostic.

**One `Saved` value per ingested asset**, so the caller can emit the right event without
re-deriving anything:

```rust
/// One asset a finished job produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Saved {
    Track(Track),
    Art(Artwork),
}
```

`ingest_outputs` keeps its signature and returns `Vec<Saved>`, in batch order.

**Extensions.** Keep `AUDIO_EXTS` exactly as it is and add:

```rust
/// Image extensions the app files as artwork.
///
/// Wider than the two save nodes `emit` recognises (`SaveImage` -> png,
/// `SaveImageWebP` -> webp) on purpose. A filter that is too tight drops a real
/// output **silently**, which is the exact failure this task exists to remove;
/// one that is too wide files something the user can see and delete.
const IMAGE_EXTS: &[&str] = &["png", "webp", "jpg", "jpeg"];
```

**Dispatch on the record's kind, then filter by that kind's extension list.** `ModelKind::Music`
files audio and skips everything else, exactly as today; `ModelKind::Image` files images and skips
everything else. Neither list is consulted for the other kind.

**`ingest_one_art_file` mirrors `ingest_one_file` step for step**, and the order matters for the
same reason it does there: mint the id and **persist the counter before any file write**, so a
crash burns an id rather than overwriting an artwork the user already has. Then

```rust
    let dst = library::art::image_path(root, slug, id, ext)?;
    std::fs::rename(src, &dst)?;
    let (width, height) = match library::art::dimensions_of(&dst) {
        Some((w, h)) => (Some(w), Some(h)),
        None => (None, None),
    };
```

and build the record:

```rust
    Artwork {
        id: id.clone(),
        // The title the user named at generation, carried on the spec, exactly
        // as `Track::title` is (T-409). A snapshot, not a link.
        title: pending.spec.title.clone(),
        file: format!("art/{}.{}", id.0, ext),
        width,
        height,
        provenance: /* the same `Provenance` `build_track` builds -- factor the
                       shared construction out rather than writing it twice */,
    }
```

`project.art.push(id)` then `save_project`, mirroring the track path.

**The `Provenance` construction is now needed twice.** Factor it into one function both callers
use. Two copies of a provenance record is how a field gets added to one and not the other, and
provenance that is wrong in one asset kind is the failure CONVENTIONS names.

### 2. `jobs.rs` — the pump dispatches, and a second event

`ArtSaved` mirrors `TrackSaved` field for field (`id`, `project_slug`, `file`), and

```rust
/// Emitted once per artwork saved after a successful generation.
```

In `ingest_if_pending`, the **download directory is chosen by the pending record's kind** —
`library::tracks::tracks_dir` or `library::art::art_dir` — created with `create_dir_all` as now,
and passed to `comfy.outputs`. Then emit per saved asset:

```rust
            for saved in items {
                match saved {
                    Saved::Track(track) => { /* "track://saved", TrackSaved  */ }
                    Saved::Art(art) => { /* "art://saved", ArtSaved */ }
                }
            }
```

Nothing else in the pump changes. The failure path, the session-log line and the re-emitted
`job://failed` on an ingest failure all stay exactly as they are — an image job that cannot be
filed must fail as loudly as a track that cannot be.

### 3. `generate.rs` — two commands over one pipeline

Add the guard as a **pure function**, because the commands themselves are unreachable from a test
(no test in this crate builds an `AppHandle` — see the comment on `ComfyState::remember`):

```rust
/// Why this profile cannot be queued by this command, or `None` to go ahead.
///
/// The two commands are one pipeline with one difference: which kind of asset
/// the outputs become. A music profile queued as an image would download its
/// FLAC into `art/` and file nothing, because ingest dispatches on the record's
/// kind -- a Done row and no output, which is the silent failure this task
/// exists to remove. So the mismatch is refused at submit, in a sentence that
/// says where the profile does belong.
fn kind_error(profile: &ModelProfile, wanted: ModelKind) -> Option<String> {
    if profile.kind == wanted {
        return None;
    }
    Some(match wanted {
        ModelKind::Music => format!(
            "{} is an image model, so it cannot generate audio. \
             Pick a music profile here, or generate artwork from Cover Art.",
            profile.id
        ),
        ModelKind::Image => format!(
            "{} is a music model, so it cannot generate cover art. \
             Pick an image profile here, or generate audio from the Audio Studio.",
            profile.id
        ),
    })
}
```

Extract the body of `generate_audio` into

```rust
async fn queue_generation(
    app: AppHandle,
    state: &ComfyState,
    config_dir: &ConfigDir,
    profiles_dir: &ProfilesDir,
    spec: GenerationSpec,
    kind: ModelKind,
) -> Result<Submission, String>
```

unchanged except that it calls `kind_error` immediately after the profile loads (before
`ensure_connected` — a refusal must not start a `comfy-mcp`), and puts `kind` on the
`PendingOutput`. `generate_audio` and the new `generate_image` become two-line commands that
differ only in the `ModelKind` they pass. **`build_and_submit` is not touched.**

### 4. `lib.rs`

Register `generate::generate_image` beside `generate::generate_audio`.

## Tests — named by the invariant, not the mechanics

In `ingest.rs` (the existing track tests must keep passing unchanged; the rename is mechanical):

- **an image output is filed as artwork rather than skipped** — a `PendingOutput` with
  `kind: Image` and a batch holding one `.png` writes `art/ar-0001.png`, writes its sidecar, and
  leaves `project.art` holding that id. *This is the regression the whole task exists for: before
  it, the same call returned an empty list and wrote nothing.*
- **the kind decides, not the extension** — an image pending whose batch holds a `.flac` files
  nothing, and a music pending whose batch holds a `.png` files nothing. Both directions, because
  a dispatch that reads the extension first would pass one of them.
- **the artwork's provenance is the track's provenance** — profile id and display name, licence,
  template, the spec, the resolved slots and the prompt id all land in `art/<id>.json`. Assert
  against the pending record's own values, not against a literal, so the shared constructor is
  what is under test.
- **the pixel size is read from the file that was written** — use
  `testdata/images/klein-cover.png.head` as the output file's content and assert
  `width == Some(768)`, `height == Some(768)`. A file with no readable header records `None` for
  both and is still saved.
- **the id counter is persisted before the file is written** — the mirror of the existing track
  test: `next_art_seq` on disk is 2 after one artwork.
- **two images in one batch become two artworks, in batch order**, with distinct ids.
- **the title travels from the spec** — `Artwork::title` is the spec's title, and `None` when the
  spec has none.

In `generate.rs`:

- **each command refuses the other's profile, and says where it belongs** — `kind_error` on the
  shipped `ace-step-1.5-turbo` profile with `ModelKind::Image`, and on
  `testdata/profiles/flux2-klein-9b-image.json` with `ModelKind::Music`. Assert the message names
  the profile id; do not assert the whole sentence.
- **a matching kind is permitted** — both profiles against their own kind return `None`. Without
  this, a guard that refused everything would pass the test above.
- **the image pipeline copies its workflow instead of fetching one, and makes no save-node
  edit** — drive `build_and_submit` with the Klein profile through the mock rig the existing
  `test_pipeline_calls_the_tools_in_order_on_one_working_copy` uses. Two differences from the
  audio case, and they are the point of the test: the reply list has **no `fetch_template`
  reply** (a workflow-backed profile copies the file — ARCHITECTURE §5b), and the returned
  `Submission::output_format` is `None` because `prefer_lossless` is false. `unchecked_slots` and
  `lora_nodes` both come back empty.

  **The fixture's `comfy.workflow` is repo-relative** (`testdata/workflows/flux2_klein_9b.json`,
  see `testdata/profiles/README.md`), so the test must rewrite it to an absolute path built from
  `CARGO_MANIFEST_DIR` before handing the profile to `build_and_submit`.

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the four listed files
- [ ] `build_and_submit`, the graph edits, and the job pump's polling/failure paths are unchanged
- [ ] every existing `ingest.rs` and `jobs.rs` test still passes with no assertion edited — the
      rename is the only thing that touches them

## Out of scope
- **The frontend.** No bridge, no store, no view, no listener for `art://saved` — T-506c/d. This
  lane is verifiable by tests alone and has **no click-through**; the first sight of a cover in the
  app is T-506d.
- **`Track.cover` and attaching artwork to anything** — T-506e, with `delete_art`.
- **A size control** — deferred with its evidence (MCP-SURFACE §35.2).
- **Batching images by seed.** `specsFor` is frontend (T-312); when the CoverArt view wants it, it
  gets it there, and this lane's N-outputs-per-job path already handles a batch.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/library/src/art.rs --read crates/library/src/tracks.rs --read crates/create-core/src/provenance.rs --read crates/create-core/src/profile.rs --read crates/create-core/src/project.rs --file src-tauri/src/ingest.rs --file src-tauri/src/jobs.rs --file src-tauri/src/generate.rs --file src-tauri/src/lib.rs
```
