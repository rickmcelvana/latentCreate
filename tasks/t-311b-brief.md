# T-311b — ingestion: the finished audio becomes a track with its recipe

**Lane: Aider.** Wiring across `src-tauri` against seams that all exist, with the two awkward parts
(the pending record, the write order) pre-derived below. **Depends:** T-311a (landed), T-306b.
**Crate/dir:** `src-tauri`.

**Files to create/modify:**

- `src-tauri/src/ingest.rs` — **new**, the whole of the disk work and its tests
- `src-tauri/src/jobs.rs` — `ComfyState` holds the pending records; `pump` takes one; the `Done`
  arm ingests
- `src-tauri/src/generate.rs` — build the pending record, capture `server_info`
- `src-tauri/src/lyricdoc.rs` — `default_project` moves out (see below)
- `src-tauri/src/lib.rs` — `mod ingest;`

**Scope warning.** This is close to the 400-line limit. If it runs over, **stop at the end of
section 3** — `ingest.rs` and its tests stand alone and are fully testable without the wiring — and
say so rather than trimming tests to fit.

---

## What is already true, and was checked

- **`Provenance` needs nothing new.** Every field maps to something in hand:
  `profile.id`, `profile.display_name`, `profile.license`, `profile.comfy.template`, the
  `GenerationSpec`, the `ResolvedSlots`, and `created_at`. **Do not add a field to `Provenance` or
  `Track`.**
- **The sidecar really is self-contained.** Lyrics are an ordinary declared input
  (`"lyrics": {"type": "lyrics", "slots": ["94.lyrics"]}`), so the **text** lands in `spec.inputs`
  and again in `resolved_slots`. `spec.lyrics` is a `LyricRef` recording *where that text came
  from*, attached by `lyricRefFor` only when the submitted text is byte-for-byte the approved
  version. So "reproduces from its sidecar alone" holds without reading the lyric document, and the
  `prompt_optimized` consent flag stays a property of that document, reachable through the ref.
  **No optimized-flag field is needed on `Provenance`**, despite ARCHITECTURE section 8's prose,
  which predates `LyricRef`.
- **`LocalComfy::outputs(id, out_dir)` -> `OutputBatch { prompt_id, out_dir, files: [{url, path,
  size}] }`**, verified against a real completed job (MCP-SURFACE 26). comfy-cli names each download
  `<short_prompt_id>_<NNN>.<ext>`; ComfyUI's own filename survives only inside `url`.
- **`LocalComfy::health()` -> `ServerInfo`** carries everything `ComfyServerInfo` wants:
  `server.url`, `compatibility.comfy_cli_version`, `freshness.core.installed`.
- **`library::tracks`** (T-311a) has `mint_track_id`, `save_track`, `audio_path`, `sidecar_path`,
  `duration_of`. **`save_track` writes only the sidecar** and deliberately does not touch
  `project.json`; registering the id is this task's job.

## 1. The problem this task exists to solve

`generate_audio` resolves the slots inside `build_and_submit`, returns a `Submission` to the
frontend and **retains nothing**. Minutes later `monitor_job` emits `job://done` knowing only a
`prompt_id`. At that moment nothing knows the spec, the resolved slots, the profile's licence or
which project this belongs to.

**Recomputing `resolve_slots` at completion is not equivalent.** It yields what the app would
resolve *now*; a profile edited in between would produce a sidecar describing a run that never
happened. Provenance exists to record what did happen.

**Ingestion therefore runs in Rust, on the completion path, not in the frontend.** A sidecar written
by the webview would be lost whenever the view is unmounted or the window closed mid-job, and
provenance is a data-integrity guarantee rather than a display concern.

## 2. The pending record

```rust
/// Everything a finished job needs to become a track, captured when it was
/// submitted.
///
/// Held in memory only. **An app restart mid-job loses that job's provenance**,
/// and that is deliberate rather than overlooked: the queue itself is in-memory
/// and does not survive a restart either, so a job whose row is already gone
/// writing a sidecar nobody expected would be the stranger behaviour. A job
/// that outlives the app still finishes on the GPU; it simply is not ingested.
#[derive(Debug, Clone)]
pub struct PendingTrack {
    /// Project the track is filed under.
    pub project_slug: String,
    pub profile_id: String,
    pub profile_display_name: String,
    pub model_license: String,
    pub template: Option<String>,
    /// What the user chose.
    pub spec: GenerationSpec,
    /// What ComfyUI actually received -- captured at submit, never recomputed.
    pub resolved_slots: ResolvedSlots,
    /// The server that ran it, when `server_info` could be read.
    pub comfy: Option<ComfyServerInfo>,
}
```

`ComfyState` gains `pending: Arc<Mutex<HashMap<String, PendingTrack>>>` beside the existing `jobs`
map, and **`pump` takes `Option<PendingTrack>`**:

- `run_workflow` passes **`None`** — a bare workflow has no profile behind it, so it cannot have
  provenance, and an `Option` says that in the type instead of leaving it to a comment.
- `generate_audio` passes `Some(..)`.

Remove the entry when the pump retires, next to the existing `jobs.remove(&id)`, on **every**
outcome — a failed or cancelled job must not leave its record behind for ever.

## 3. `src-tauri/src/ingest.rs` — the disk work, and all of the tests

**Keep the MCP call out of this module.** The function takes an already-fetched `OutputBatch`, so
every test here runs on real temp files with no transport:

```rust
pub fn ingest_outputs(
    root: &Path,
    pending: &PendingTrack,
    batch: &OutputBatch,
    created_at: &str,
) -> Result<Vec<TrackId>, IngestError>;
```

For **each** file in `batch.files`, in order:

1. **Skip anything that is not audio.** Keep `flac`, `wav`, `mp3`, `ogg`, `opus`, `m4a`; skip the
   rest and do not error. A workflow that also saves a spectrogram PNG must not file it as a track.
   Take the extension from the downloaded `path`, lowercased.
2. Load the project, **`mint_track_id`, and save the project immediately** — see the write order
   below.
3. Rename the downloaded file to `audio_path(root, slug, &id, ext)`.
4. `duration_of` on the renamed file.
5. Build the `Track` (`file` is the project-relative `tracks/<id>.<ext>`, `title: None`) and
   `save_track`.
6. Push the id onto `project.tracks` and save the project again.

### The write order, and why it is three writes rather than two

**Minting an id and not persisting the counter is how a track gets overwritten.** If the counter
bump lived only in memory until the end, a crash after the sidecar was written would leave
`next_track_seq` unadvanced on disk -- and the next generation would mint the *same* id and
overwrite both the audio and the sidecar of a track the user already has.

So: **save the counter first** (step 2), then write the file and its sidecar, then register the id
(step 6). A crash in between burns an id and leaves an unreferenced sidecar -- invisible to the
Library, recoverable by hand, and harmless. That is the direction T-311a's `save_track` doc comment
already argues for, and a burned id costs nothing because ids are never reused by design.

`title` stays `None`. The file name is the id, and the user has not named anything yet; inventing a
title here would put a second copy of a fact in a file that is meant to be its only home.

### Errors

`IngestError` is a `thiserror` enum per CONVENTIONS: at least `Library(#[from] LibraryError)` and
`Io(#[from] std::io::Error)`. **No `unwrap`/`expect` anywhere** -- this runs on a background task
where a panic is invisible.

An ingest failure must **not** be silent. Log it to the session log and emit the failure on the
existing `job://failed` shape if the job itself succeeded but its ingestion did not, so the queue
row does not claim Done for a track that was never saved. If that reads as over-reach, do only the
logging and say so.

## 4. Wiring

**In `generate.rs`:** after a successful submit, build the `PendingTrack`. `resolved` is already in
hand inside `build_and_submit` -- pass it out rather than recomputing. Capture the server with
`comfy.health()`, mapping `ServerInfo` to `ComfyServerInfo`:

```rust
fn server_info_of(info: &ServerInfo) -> ComfyServerInfo {
    ComfyServerInfo {
        comfyui_version: info
            .freshness
            .as_ref()
            .and_then(|f| f.core.as_ref())
            .and_then(|c| c.installed.clone()),
        comfy_cli_version: info
            .compatibility
            .as_ref()
            .and_then(|c| c.comfy_cli_version.clone()),
        url: info.server.as_ref().and_then(|s| s.url.clone()),
    }
}
```

**A failed `health()` must not fail the generation** -- it becomes `comfy: None`. The job is already
queued by then, and refusing to record it because a version string could not be read would be
absurd.

**Which project.** `src-tauri/src/lyricdoc.rs` has `default_project` (first project in slug order,
created on first use), which lyrics already rely on. **Move it, unchanged, somewhere both callers
can use it** -- `library` is wrong (it is policy, not storage), so a small `projectctx` module in
`src-tauri`, or `ingest.rs` with `lyricdoc.rs` calling it. A second copy would drift, and lyrics and
tracks landing in different projects is the exact bug that would follow.

**In `jobs.rs`:** in `monitor_job`'s `TerminalOutcome::Done` arm, after emitting `job://done`, take
the pending record and, when there is one, `comfy.outputs(&id, tracks_dir)` then `ingest_outputs`.
Emit **`track://saved`** with `{ id, project_slug, file }` per track so a Library view can react
later; nothing consumes it yet, and that is fine.

**Fetch straight into the project's `tracks/` directory.** comfy-cli's `<prompt_id>_<NNN>` naming
cannot collide with another job's, so there is no temp directory and no cross-filesystem move --
the rename in step 3 is within one directory. Create the directory first.

## 5. Acceptance criteria

- [ ] `npm run gate` green.
- [ ] Tests in `ingest.rs`, each naming its invariant:
  - `test_ingest_writes_the_audio_the_sidecar_and_the_project_entry` — the whole path, on temp
    files, with the **two-LoRA** spec, since that is the milestone's bar.
  - `test_ingest_reproduces_from_the_sidecar_alone` — load the written sidecar back and assert it
    carries the LoRA stack in order with strengths, the seed, the resolved slot values **and the
    lyric text**. This is T-311's acceptance bar; it must be a test, not a click-through.
  - `test_ingest_advances_the_counter_before_writing` — mint, then read `project.json` back from
    disk **before** the sidecar write would have happened, and assert `next_track_seq` already
    moved. Aimed at the overwrite hazard above; a version that persists the counter last passes
    every other test here.
  - `test_ingest_skips_a_non_audio_output` — a `.png` in the batch produces no track.
  - `test_ingest_records_the_real_extension_not_the_requested_one` — a `.wav` download is filed as
    `.wav` even though both shipped profiles ask for FLAC.
  - `test_ingest_duration_comes_from_the_file` — use `testdata/audio/ace-step.flac.head` as the
    downloaded file; assert `duration_s == Some(120.0)` while the spec's `duration_s` input says
    something else. **Set the spec's value to 90.0**, so a version copying the input rather than
    reading the file fails.
  - `test_a_bare_run_workflow_job_has_no_pending_record` — `pump(.., None)` ingests nothing.
- [ ] The mock-transport test from T-306b still passes; extend it if the call sequence changed.
- [ ] `cargo clippy --all-targets -- -D warnings` clean.
- [ ] No changes outside the listed files.

## 6. Out of scope

- **The Library view.** T-311c. Nothing needs to *read* these tracks yet.
- **Delete, rename, export, Send to.** Each is its own task; delete goes to OS trash.
- **Batch by seeds** (T-312) and **cover art** — `art/` is untouched here.
- **Persisting pending records across a restart.** Named as a known limit above; if it later
  matters, it is a design change with its own entry, not a quiet addition.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-311b-brief.md --read CONVENTIONS.md --read crates/library/src/tracks.rs --read crates/create-core/src/provenance.rs --read crates/create-core/src/generation.rs --read crates/create-core/src/project.rs --read crates/mcp-bridge/src/jobs.rs --read crates/mcp-bridge/src/health.rs --file src-tauri/src/ingest.rs --file src-tauri/src/jobs.rs --file src-tauri/src/generate.rs --file src-tauri/src/lyricdoc.rs --file src-tauri/src/lib.rs
```

Every `--read` file defines a type the new code constructs -- `Track`, `Provenance`,
`ComfyServerInfo`, `GenerationSpec`, `ResolvedSlots`, `Project`, `TrackId`, `OutputBatch`,
`ServerInfo` -- which is WORKFLOW section 3's rule for `--read` rather than `--file`.
