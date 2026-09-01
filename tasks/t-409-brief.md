# T-409 — the song title, carried

The last-but-one Phase 4 task (T-406 follows). Opened by owner decision 5: **a
title named once in Lyrics Studio should reach the track, the Library and the
exported file.** The field exists at both ends and connects to nothing —
`Track.title` is hardcoded `None` at ingest ([ingest.rs:147](../src-tauri/src/ingest.rs)),
and `LyricDoc.title` is in the schema but has never been writable.

## What is already wired (verified 2026-09-01, before briefing)

Two findings shrink this task and shape the lanes:

- **Export already defaults to the title.** [Library.tsx:314](../app/src/views/Library.tsx)
  calls `runExport(row.id, `${row.name}.${ext}`)`, and `row.name` is already
  `trackName(track)` = `track.title` else the id ([library.ts:47](../app/src/state/library.ts)).
  `runExport` → `pickExportPath(defaultName)` → the OS dialog's `defaultPath` is
  a chain that already exists ([tracks.ts](../app/src/bridge/tracks.ts)). So the
  moment `Track.title` is set, the export filename follows for free. **The only
  export work left is trap 4: sanitise the name before it reaches the dialog** (a
  title with `/`, `:` or a trailing dot is legal in a doc, illegal as a Windows
  filename).
- **`specFor` already threads the selected document in.** [generate.ts:175](../app/src/state/generate.ts)
  is `specFor(profileId, model, values, stack, doc: LyricDoc | null)`. Prefilling
  a title from `doc.title` is a one-field addition, not new plumbing. And
  `saveLyricDoc(doc)` ([lyricdoc.ts:104](../app/src/bridge/lyricdoc.ts)) already
  persists a whole document, so the Lyrics Studio title input needs no new
  command — only the input.

So the substance is: **add one field to `GenerationSpec`, flow it to `Track.title`
at ingest, give `LyricDoc.title` an input, prefill the spec title in the Audio
Studio, and sanitise the export name.** The Library display is already there.

## The interface change, and its cost

`GenerationSpec` gains `title: Option<String>`. It is **stored in provenance**
(trap 5), so the title lands in every sidecar for free and T-406's "re-use these
settings" will carry it. This is an **ARCHITECTURE 5/7 interface change** — the
doc edit lands in the **same commit** as lane a (AGENTS hard rule).

`#[serde(default)]` on the new field keeps every existing sidecar readable (old
specs deserialize with `title: None`). But `GenerationSpec` does not derive
`Default` (its `profile_id` is a required `String`), so **every struct-literal
construction site must add `title: None`** or it will not compile. There are
~35, nearly all in `#[cfg(test)]` modules; several are helper fns
(`full_ace_spec`, `full_minimax_spec`, `sample_track`, `generate.rs::spec`,
ingest/jobs/sendto `PendingTrack` builders) where one edit covers many callers.
This mechanical spread is the bulk of lane a — a **candidate to hand to Aider**
if context is tight, since it is wide, low-risk, and compiler-checked.

## Rejected alternative (record the decision, do not re-litigate)

Resolving the title at ingest from `spec.lyrics.doc_id` — reading the document's
current title when the track is saved — is **rejected on evidence**: one of the
producer's 20 tracks carries **no lyric ref at all**, so ingest would have no
title source for it. Provenance must record **what the user chose at generation**,
not what a second file happens to say later. The title travels **on the spec**.

## Lane a — the field flows (`GenerationSpec.title` → `Track.title`)

**Schema** ([generation.rs:135](../crates/create-core/src/generation.rs)):

```rust
pub struct GenerationSpec {
    pub profile_id: String,
    pub inputs: BTreeMap<String, InputValue>,
    #[serde(default)]
    pub loras: Vec<LoraRef>,
    #[serde(default)]
    pub lyrics: Option<LyricRef>,
    /// The song title the user named at generation, carried to the track and its
    /// exported filename. `None` is an untitled track (the Library falls back to
    /// the id). A **snapshot**, not a link: it records what the user chose here,
    /// so retitling the source `LyricDoc` later never retitles tracks already
    /// made (ARCHITECTURE 8, trap 2).
    #[serde(default)]
    pub title: Option<String>,
}
```

**Ingest** ([ingest.rs:147](../src-tauri/src/ingest.rs)): `title: None` becomes
`title: pending.spec.title.clone()`. `PendingTrack.spec` is the `GenerationSpec`,
so nothing new is threaded.

**Construction sites:** add `title: None` to every `GenerationSpec { … }` literal
(and to the ingest/jobs/sendto `PendingTrack` `spec:` blocks). Prefer editing the
shared helper fns so their callers are covered once. `cargo build --tests` is the
checklist — it fails until every site is updated.

**TS type** ([generate.ts:27](../app/src/bridge/generate.ts)): add
`title: string | null` to the `GenerationSpec` interface. The wire fixture in
`testdata/wire/` is config-only, so no shared fixture changes; but keep the TS
type and the Rust struct in lockstep (the house rule the config wire test
enforces for its own type).

**ARCHITECTURE 5/7:** add `title` to the `GenerationSpec` interface description,
same commit.

**Tests (lane a):**
1. `create-core`: a `GenerationSpec` with a title **round-trips** through JSON,
   and one serialized **without** a `title` key deserializes to `title: None`
   (the `serde(default)` guard — kills a mutation dropping the attribute).
2. `src-tauri` ingest: `build_track` **copies the spec's title onto the track**
   (a titled spec → `Track.title == Some(...)`), and a `None`-title spec → a
   `None`-title track. Kills the mutation that re-hardcodes `None`.

Mutation pass (file-copy backup, **not `git checkout`**): `pending.spec.title.clone()`
→ `None` (killed by ingest test 2); drop `#[serde(default)]` on `title` (killed
by round-trip test 1's missing-key half).

## Lane b — a title input in Lyrics Studio

`LyricDoc.title` is `string | null` and `saveLyricDoc(doc)` persists it. Add a
title input at the top of the Lyrics Studio document, bound to the open doc's
title, saving through the existing save path (the same debounce/save the body
text uses — check how the editor persists edits and mirror it). Empty input →
`title: null` (untitled), never `""`. **No new command, no backend.** Retitling
here must not touch any track (trap 2 — automatic, since the track copied its
title at ingest).

Store/tests: whichever `state/lyrics.ts` (or `lyricdoc` store) holds the open
doc — a set-title action that trims, maps empty → null, and calls
`saveLyricDoc`. Test: setting a title persists it; clearing it stores null.

## Lane c — Audio Studio prefill + export sanitise

**Prefill** ([generate.ts:175](../app/src/state/generate.ts)): `specFor` sets
`title` from the selected document, editable in the panel:

- Add `title` to the returned spec: the user's Audio-Studio title override when
  they typed one, else `doc?.title ?? null`.
- The Audio Studio grows a small **title field** (in the generate panel, near the
  lyric-document picker), prefilled from the selected doc's title and editable,
  so the title on the spec is what the user sees. A batch of five shares the one
  title (the seed varies, per T-312; the title does not).

**Export sanitise (trap 4)** — the one real export change. Add a small
`filenameSafe(name)` helper (frontend) and apply it where the default name is
built ([Library.tsx:314](../app/src/views/Library.tsx)):
`runExport(row.id, `${filenameSafe(row.name)}.${ext}`)`. Replace the Windows-illegal
set `\ / : * ? " < > |`, strip trailing dots/spaces, collapse to a non-empty
stem (fall back to the id when nothing survives). Sanitise **before** the dialog,
never after — the symptom of getting this wrong is the OS refusing the save with
its own message instead of the app preventing it.

Tests: `filenameSafe` maps `"My Song: Vol. 2/2"` to a legal stem, a title of only
illegal characters to the id fallback, and leaves a clean title unchanged.

## Traps (from phase-4.md, all still live)

1. **The audio file on disk keeps its id name.** `tracks/tr-0007.flac` never
   becomes `Midnight.flac`. The id addresses the file; the title is a
   **display-and-export** name only (ARCHITECTURE 8). Nothing in this task
   renames a file on disk.
2. **`Track.title` is a snapshot, not a link.** Guaranteed by carrying the title
   on the spec and copying it at ingest — retitling the doc later cannot reach an
   existing track.
3. **Titles are not unique and are not ids.** Tracks stay id-addressed; only
   albums are name-addressed (T-403).
4. **Sanitise before the dialog, not after** (lane c).
5. **The spec is in provenance**, so the title is recorded for T-406 for free;
   the ARCHITECTURE edit rides lane a's commit.

## Click-through (producer, on the desktop app)

1. In Lyrics Studio, give the open document a **title** (e.g. `Midnight: Drive/2`).
   It persists across a document switch and a restart.
2. In the Audio Studio, the title field is **prefilled** from that document and
   is editable. Generate a track (a batch of two is fine).
3. In the Library, the new track(s) show **that title**, not `tr-00NN`. A batch
   shares the one title.
4. Export the track — the save dialog's **default filename is the title,
   sanitised** (`Midnight_ Drive_2.flac` or similar — legal, no `:` or `/`), and
   the saved file plays.
5. An **untitled** track (or one of the producer's 20 pre-existing, which never
   had a title) still shows and exports by its id — the fallback is intact.
6. Rename the source lyric document — a track already generated from it **keeps**
   its old title (snapshot, not link).

## Lane order and commits

`a` (field + ingest + ARCHITECTURE) → `b` (Lyrics Studio input) → `c` (prefill +
export sanitise). Each lane commits on a green `npm run gate`. Lane a must land
first: b and c have nothing to carry until the field exists and flows.

## Not this task

- Renaming the audio file on disk (trap 1 — never).
- Backfilling titles onto the 20 pre-existing tracks (T-405's rename is the only
  way those get titles; T-409 sets a title at generation and does not backfill).
- T-406 (provenance inspector) — the last task; it will surface the new `title`
  field, which is why lane a's ARCHITECTURE edit matters to it.
