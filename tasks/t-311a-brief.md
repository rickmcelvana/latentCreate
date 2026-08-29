# T-311a — the track sidecar, and a duration read from the file

**Lane: Aider.** On-disk plumbing that mirrors an existing module almost line for line, plus one
pure function this brief hands over already written and verified. **Depends:** T-306b (the pipeline),
T-309a (LoRA refs in the spec). **Crate/dir:** `crates/create-core`, `crates/library`.

**Files to create/modify:**

- `crates/create-core/src/project.rs` — add `Project::next_track_seq`
- `crates/create-core/src/audio.rs` — **new**, one pure function
- `crates/create-core/src/lib.rs` — `pub mod audio;`
- `crates/library/src/tracks.rs` — **new**
- `crates/library/src/lib.rs` — **`pub mod tracks;` and nothing else**. See the note below;
  this line previously said "and the re-exports" and was wrong.

`testdata/audio/ace-step.flac.head` already exists and is committed. Do not regenerate it.

---

## Why this is T-311**a**

T-311 as the phase file writes it is output ingestion, the library write, the sidecar, *and* the
"reproduces from its sidecar alone" bar. That is far past a 400-line diff, and the halves have
different verification stories. So:

- **T-311a (this brief)** — everything that can be tested with no ComfyUI: the id, the sidecar
  writer, the duration read.
- **T-311b** — ingestion: retain the submit-time record, `fetch_outputs` on completion, build the
  `Provenance`, write the `Track`. Needs a live run to verify, and carries a real design problem
  named at the bottom of this brief.
- **T-311c** — the Library view. `app/src/views/Library.tsx` is a 13-line placeholder, so a sidecar
  with no reader is invisible. Not scoped by the phase file; it should be.

## What already exists — checked, not assumed

Read these before writing anything; three of them are the pattern this task copies.

- **`create-core::provenance`** (193 lines) already defines `Track`, `Provenance` and
  `ComfyServerInfo`, with a round-trip test covering a two-LoRA spec. **No type work is needed for
  the sidecar itself.** The phase file reads as though the sidecar must be designed; it was designed
  in T-003b.
- **`create-core::project`** has `TrackId`, `Project::tracks: Vec<TrackId>` and
  `Project::next_lyric_seq` with the never-reused rationale in its doc comment.
- **`library::lyrics`** is the module to mirror: `LYRICS_DIR`, `lyrics_dir`, `doc_path`,
  `mint_doc_id`, `save_doc`, `load_doc`. Ids are `ld-0001`, four digits, minted from the counter.
- **`library::atomic::write_json`** is how every JSON file in this crate is written. Use it.
- **`mcp-bridge::LocalComfy::outputs`** already wraps `fetch_outputs` and its `OutputBatch` matches
  the live payload exactly (verified 2026-08-29, MCP-SURFACE 26). **T-311b's, not yours.**

## The paths

**ARCHITECTURE section 8 was wrong about this until 2026-08-29 and is now correct** — there is no
`library/` directory level, and the identifier is `com.latentbeats.create`. Verified on disk. What
`library::projects::project_dir` actually builds:

```
<app config dir>/projects/<slug>/
├── project.json
├── lyrics/<doc-id>.json
└── tracks/<track-id>.flac   <- the audio
    tracks/<track-id>.json   <- the sidecar, same stem
```

## Spec

### 1. `Project::next_track_seq`

Exactly `next_lyric_seq`: `u32`, `#[serde(default = "default_track_seq")]` returning `1`, set in
`Project::new`, and a doc comment carrying the same reason — **monotonic and never reused, even
after a delete.** Deriving an id from the files present would hand a deleted track's id to a later
one, and an `AlbumList` still holding the old `TrackId` would then point at unrelated audio.

There is an existing test asserting a project file written before `next_lyric_seq` existed still
loads with the default. Add its twin for tracks: an old `project.json` with no `next_track_seq` must
load with `1`, not fail.

### 2. `create_core::audio::flac_duration_s`

**Integrate verbatim.** Derived and run against a real 13.7 MB ACE-Step FLAC on 2026-08-29 (120.000 s,
48 kHz, stereo, 16-bit), checked on four inputs, and already `cargo fmt` clean:

```rust
/// Length in seconds, read from a FLAC file's STREAMINFO block.
///
/// `None` for anything that is not a FLAC file, and for a FLAC whose
/// STREAMINFO reports `total_samples` of 0 -- the format's own "unknown
/// length" value, which a stream-encoded file legitimately carries.
///
/// Only the first 42 bytes are needed. STREAMINFO is mandatory and must be the
/// first metadata block, so its position is fixed: 4 bytes of `fLaC` magic, a
/// 4-byte block header, then 34 bytes of STREAMINFO. Sample rate is 20 bits
/// and total samples 36, both unaligned, which is why this is bit arithmetic
/// rather than a struct read.
pub fn flac_duration_s(head: &[u8]) -> Option<f64> {
    if head.len() < 42 || &head[..4] != b"fLaC" {
        return None;
    }
    let si = &head[8..42];
    let sample_rate =
        (u32::from(si[10]) << 12) | (u32::from(si[11]) << 4) | (u32::from(si[12]) >> 4);
    let total_samples = (u64::from(si[13] & 0x0F) << 32)
        | (u64::from(si[14]) << 24)
        | (u64::from(si[15]) << 16)
        | (u64::from(si[16]) << 8)
        | u64::from(si[17]);
    if sample_rate == 0 || total_samples == 0 {
        return None;
    }
    Some(total_samples as f64 / f64::from(sample_rate))
}
```

**No new dependency.** A crate to read one 34-byte header would be the wrong trade, and both shipped
profiles force FLAC (`prefer_lossless: true`, `SaveAudioAdvanced`), so this covers every file the app
itself produces. A profile opting out gets `None`, which is what `Option<f64>` is for.

### 3. `library::tracks`

**`lib.rs` gets `pub mod tracks;` and no `pub use` line** (corrected 2026-08-29, after the executor
rightly queried it). Every existing re-export in that file names a type **defined in one of this
crate's own modules** -- `Config`, `LyricDocSet`, `ProfileSet`, `ProjectSet`, `SecretKey`. The crate
re-exports nothing from `create-core`: `lyrics.rs` works with `LyricDoc` throughout and does not
re-export it, and callers such as `src-tauri/src/lyricdoc.rs` import it directly as
`create_core::project::LyricDoc`.

`tracks.rs` as scoped here defines **no types at all** -- only functions -- so it has nothing to
re-export, and re-exporting `Track`/`TrackId` from `create-core` would invent a second import path
for one type. A `pub use tracks::TrackSet;` line becomes correct in **T-311c**, when `list_tracks`
and its warning set arrive with the Library view.

Mirror `library::lyrics`. Public surface, and nothing beyond it:

```rust
pub const TRACKS_DIR: &str = "tracks";

pub fn tracks_dir(root: &Path, slug: &str) -> Result<PathBuf, LibraryError>;
pub fn sidecar_path(root: &Path, slug: &str, id: &TrackId) -> Result<PathBuf, LibraryError>;
pub fn audio_path(root: &Path, slug: &str, id: &TrackId, ext: &str) -> Result<PathBuf, LibraryError>;
pub fn mint_track_id(project: &mut Project) -> TrackId;
pub fn save_track(root: &Path, slug: &str, track: &Track) -> Result<(), LibraryError>;
pub fn load_track(root: &Path, slug: &str, id: &TrackId) -> Result<Track, LibraryError>;
pub fn duration_of(path: &Path) -> Option<f64>;
```

- **`mint_track_id`** produces `tr-0001` — prefix `tr-`, four digits, `saturating_add(1)`, exactly as
  `mint_doc_id` does with `ld-`.
- **Every path goes through `project_dir`**, which already refuses a slug containing `..`, a
  separator or an absolute path. Do not join a slug yourself anywhere.
- **`audio_path` takes the extension from the produced file**, never from what the app asked for.
  The extension on `Track::file` *is* this app's record of the real output format, which is why no
  separate format field is being added: a second copy of that fact could disagree with the file.
  `ext` is lowercased and must be rejected if it is not entirely ASCII alphanumeric — it reaches
  this function from a filename comfy-cli chose.
- **`duration_of`** opens the file, reads **at most 42 bytes**, and calls `flac_duration_s`. It
  returns `Option`, not `Result`: a duration that cannot be read is a missing nicety, not a failed
  save, and a track whose audio is fine must never fail to be recorded because its header was odd.
- **`save_track` writes only the sidecar.** Copying or moving the audio is T-311b's; this function
  must not touch the audio file, and must not modify `project.json` either. The caller registers
  the id.

### The write-order invariant, and it is the one that matters

`library::lyrics::create_doc` carries it and the same rule applies here: **the sidecar is written
before `project.json` gains the id.** If the second write fails, the result is a sidecar nothing
references -- invisible, recoverable, harmless -- rather than a project listing a track whose file is
not there, which is the state that makes the Library view lie. Put that reason in the doc comment;
the ordering is not obvious and a later refactor will otherwise "tidy" it.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] `flac_duration_s` tested against **`testdata/audio/ace-step.flac.head`** — the real first 42
      bytes of a real generated file, committed for this purpose. Assert `Some(120.0)`.
      **Do not hand-write a FLAC header in a test.** Every recurring bug in this project has come
      from a fixture built out of what the code expected rather than what the surface sent; a
      synthetic header would test the reader against this brief's description of FLAC instead of
      against FLAC.
- [ ] Named tests, each protecting a stated invariant:
  - `test_flac_duration_reads_a_real_generated_file` — the fixture, `Some(120.0)`.
  - `test_flac_duration_is_none_for_a_truncated_header` — 20 bytes in, `None`, no panic. **This is
    the one that matters**: every index in that function is unchecked past the length guard, so
    removing the guard is a crash on a partial download rather than a wrong number.
  - `test_flac_duration_is_none_for_a_non_flac_file` — an ID3 header.
  - `test_flac_duration_is_none_when_total_samples_is_unknown` — the fixture with `si[13..18]`
    zeroed, i.e. bytes 21..26 of the head. FLAC's own "length unknown"; `Some(0.0)` would be a lie.
  - `test_mint_track_id_never_reuses_a_deleted_id` — mint, clear `project.tracks`, mint again, and
    assert the second id is not the first. Aimed at a *counter that gets derived from the file list*,
    which is the regression the doc comment exists to prevent.
  - `test_track_seq_defaults_for_a_project_written_before_it_existed`.
  - `test_save_track_then_load_track_round_trips` — with two LoRAs in the spec, since the
    milestone bar is a two-LoRA run.
  - `test_track_paths_refuse_a_slug_that_escapes_the_root` — `../../etc` and friends.
  - `test_audio_path_refuses_an_extension_that_is_not_alphanumeric`.
- [ ] `cargo clippy --all-targets -- -D warnings` clean; no `unwrap()`/`expect()` outside tests.
- [ ] No changes outside the listed files.

## Out of scope — do not write these

- **`fetch_outputs`, copying audio, building `Provenance`, any Tauri command.** All T-311b.
- **`list_tracks`.** The Library view needs it; ingestion does not, and it belongs with its consumer.
- **Any new field on `Track` or `Provenance`.** They were designed in T-003b and the round-trip test
  covers them. If something appears to be missing, say so and stop rather than adding it.
- **Deleting a track.** Delete is to OS trash and is its own task.

## If unclear

Do not guess. Output a numbered list of questions and stop.

---

## For T-311b, recorded now so it is not rediscovered

**The provenance record needs data that only exists at submit time, and ingestion happens at
completion.** `generate_audio` computes `profile.resolve_slots(spec)` inside `build_and_submit`,
returns a `Submission` to the frontend and **retains nothing**. Minutes later `monitor_job` emits
`job://done` knowing only a `prompt_id`. Nothing at that point knows the spec, the resolved slots,
the profile's display name or its licence.

Recomputing `resolve_slots` at ingestion is *not* equivalent: it yields what the app would resolve
now, not what it did resolve, and a profile edited in between would silently produce a sidecar
describing a run that never happened. Provenance exists to record what happened.

So T-311b must retain a submit-time record keyed by `prompt_id`. The likely shape is a map in
`ComfyState`, which already holds `HashMap<String, AbortHandle>` for the pumps — with the honest
consequence, to be stated in its doc comment rather than discovered: **an app restart mid-job loses
that job's provenance.** That is consistent with the queue itself, which is in-memory and does not
survive a restart either.

Also unresolved for T-311b: **which project a track belongs to.** `generate_audio` takes no slug.
`src-tauri/src/lyricdoc.rs` has a `default_project` helper (first project in slug order, created on
first use) that lyrics already rely on; a second copy of that logic would be a drift hazard, so it
should move somewhere both can use it.

## Aider launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-311a-brief.md --read CONVENTIONS.md --read crates/library/src/lyrics.rs --read crates/library/src/atomic.rs --read crates/library/src/projects.rs --read crates/create-core/src/provenance.rs --read crates/create-core/src/generation.rs --file crates/create-core/src/project.rs --file crates/create-core/src/audio.rs --file crates/create-core/src/lib.rs --file crates/library/src/tracks.rs --file crates/library/src/lib.rs
```

`lyrics.rs` is `--read` because this task is largely a transcription of it. `atomic.rs` and
`projects.rs` are `--read` because the new code calls `write_json` and `project_dir` and must not
change either. `provenance.rs` and `generation.rs` are `--read` because `Track`, `Provenance` and
`GenerationSpec` are constructed in the tests — WORKFLOW section 3's rule: code that constructs a
type needs that type in view.
