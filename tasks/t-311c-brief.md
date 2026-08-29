# T-311c — the Library's data path: tracks reach the frontend, with every decision already made

**Lane: Aider.** Transcription across five files against patterns that all exist, plus a pure
decisions module in the shape T-310a proved. **Depends:** T-311b, T-311d (both landed).
**Crate/dir:** `crates/library`, `src-tauri`, `app/src`.

**Files to create/modify:**

- `crates/library/src/tracks.rs` — `TrackSet`, `TrackWarning`, `list_tracks`
- `crates/library/src/lib.rs` — the two new re-exports
- `src-tauri/src/library.rs` — **new**, the `library_tracks` command
- `src-tauri/src/lib.rs` — `mod library;` and the handler registration
- `app/src/bridge/library.ts` — **new**, typed wrapper and mirrored types
- `app/src/state/library.ts` — **new**, the pure decisions
- `app/src/state/library.test.ts` — **new**

**No component and no CSS.** `<Library>` is **T-311e**, the next task, and it is the whole reason
this one exists in the shape it does — see below.

---

## Why this is split, and why the split is here

T-310a proved the shape: the queue's two worst bugs were *sentences* derived in JSX, where a
DOM-less vitest could not reach them. Everything a Library row says — what a track is called, how
long it is, which model and licence, what the LoRA stack was — is that same kind of decision.

So this task ends at a store with tested decisions and no pixels, and **T-311e is only JSX and
CSS**. Splitting also keeps both under the 400-line limit; together they are around 600.

**This leaves `state/library.ts` consumer-less until T-311e**, which is the same debt T-310a took
on deliberately and is acceptable only because T-311e is the very next task.

## What exists — checked, not assumed

- **`library::tracks`** (T-311a) has `sidecar_path`, `load_track`, `mint_track_id`, `save_track`,
  `duration_of`. **`list_tracks` was deliberately left out**, with the note that it belongs with its
  consumer. This is the consumer.
- **`library::lyrics::list_docs` is the template.** Copy its structure: walk the ids in
  `Project::lyrics` order, and turn every per-item failure into a warning rather than an error.
  `LyricDocSet`/`LyricWarning` are the types to mirror.
- **`Provenance` carries everything a row needs** — `profile_display_name`, `model_license`,
  `template`, `spec` (inputs, `loras`, `lyrics` ref), `resolved_slots`, `comfy`, `created_at`, and
  `prompt_id` since T-311d.
- **`app/src/bridge/loras.ts` is the bridge template**: mirrored interfaces with `Mirrors Rust ...`
  doc comments, one `invoke` wrapper.
- **`track://saved`** is emitted per saved track by T-311b and **nothing consumes it yet**. This
  task is what starts.
- The producer has exactly **one real track on disk** (`tr-0001`, two LoRAs, 120.000 s,
  `prompt_id: None` because it predates T-311d). It is the first thing this view will render, so
  **a `null` prompt id must not blank a row.**

## 1. `library::tracks::list_tracks`

```rust
pub struct TrackSet {
    /// In `Project::tracks` order -- the order they were generated.
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub warnings: Vec<TrackWarning>,
}

pub enum TrackWarning {
    Missing { id: String },
    Unreadable { id: String, detail: String },
    Malformed { id: String, detail: String },
}

pub fn list_tracks(root: &Path, project: &Project) -> TrackSet;
```

**It never fails**, exactly as `list_docs` never fails. The invariant, and it is the one that
matters: **one unreadable sidecar costs one track, not the library.** A `Result` here would let a
single malformed file hide every other track a user has made, which for a provenance store is the
worst possible failure.

Warnings are surfaced rather than dropped for the same reason `list_docs` surfaces them: a track
the user generated has gone missing, and silence reads as "there never was one".

`lib.rs` gains `pub use tracks::{TrackSet, TrackWarning};` — both are defined in this crate, which
is the rule T-311a established for that file.

## 2. `src-tauri/src/library.rs`

```rust
#[tauri::command]
pub fn library_tracks(config_dir: State<'_, ConfigDir>) -> Result<TrackSet, String>;
```

Resolve the project with **`crate::projectctx::default_project`** — the module T-311b created so
lyrics and tracks cannot disagree about which project they are in. Do not add a second resolver.

`Err` only when the *project* cannot be read; a bad sidecar is a warning inside `TrackSet`.

Register it in `lib.rs`'s `invoke_handler` alongside the existing commands.

## 3. `app/src/bridge/library.ts`

Mirror `Track`, `Provenance`, `ComfyServerInfo`, `TrackSet`, `TrackWarning`, following
`bridge/loras.ts`'s style — each with a `Mirrors Rust ...` doc comment.

```ts
export async function listTracks(): Promise<TrackSet>
export async function subscribeTracks(onSaved: (e: TrackSaved) => void): Promise<UnlistenFn>
```

`subscribeTracks` listens to **`track://saved`** and follows `bridge/jobs.ts`'s `subscribeJobs`
shape, returning its unsubscribe.

**`prompt_id` is `string | null`**, and `title` likewise. Optional-in-Rust means `null` here, not
`undefined` — `serde` sends JSON `null`. Getting this wrong is how `??` guards start missing cases.

## 4. `app/src/state/library.ts` — every decision, and nothing else

The point of the task. One `TrackRow` per track, with the view left nothing to compute:

```ts
export interface TrackRow {
  id: string
  /** The user's title, else the id -- never empty. */
  name: string
  model: string          // display name, else the profile id, else 'Unknown model'
  license: string
  duration: string       // '2:00', or '--' when unknown
  created: string        // '2026-08-29', from the RFC 3339 stamp
  loras: string          // 'vocal_instrument_merge x1.0, adapter_model x1.0', or '' for none
  seed: string           // the seed, or '--'
  /** `null` when the track predates T-311d, which is not an error. */
  promptId: string | null
  file: string
}

export const EMPTY_LIBRARY = 'Tracks you generate will appear here, with the recipe that made them.'

export function trackRows(set: TrackSet): TrackRow[]
export function warningLine(warnings: TrackWarning[]): string | null
```

Rules, each of which is a test:

- **`duration`** is `m:ss` with the seconds zero-padded — `120.0` is `'2:00'`, not `'2:0'`.
  `null` is `'--'`. Do not round a 119.6 up into a lie; floor the seconds.
- **`name`** falls back to the id when `title` is `null` **or empty after trimming**. The
  absent-versus-empty rule; five bugs now.
- **`model`** falls back to the profile id, then to `'Unknown model'` — reuse the reasoning in
  `state/queue.ts`'s `modelName`, which solved this exact problem for the queue. **Do not import it**;
  its input is a job, not a track. Say in a comment that the two are deliberate twins.
- **`loras`** lists the *file stem only*, in stack order, with strength: the full path is
  `ACE-Step-v1.5-acoustic-guitar-and-a-merge-LoRA\vocal_instrument_merge_adapter_model.safetensors`
  and a row cannot carry that. Take the segment after the last `\` or `/`, drop `.safetensors`.
  **The stored path is never modified** — this is display only, and MCP-SURFACE 27.4 records that
  those separators are load-bearing.
  Disabled entries (`enabled: false`) are **skipped**, because they did not shape the audio.
- **`created`** takes the date half of the RFC 3339 stamp. Do not parse it into a `Date` and
  reformat -- that reintroduces a timezone, and the stamp is already the truth.
- **`warningLine`** returns `null` for none, else one sentence naming the count and what to do:
  the sidecars are files, and the user can look at them. It must never be a modal (CONVENTIONS).

## 5. The store

`useLibraryStore`, Zustand, selector-subscribed by convention:

- `tracks: TrackRow[]`, `warnings: string | null`, `loading: boolean`, `error: string | null`
- `load()` — calls `listTracks`, maps through `trackRows`
- `startListening()` — subscribes to `track://saved` and **re-loads**

**Re-load, do not append.** `track://saved` carries `{id, project_slug, file}`, not a `Track`, so
appending would mean inventing a row from three fields and getting the recipe wrong. Guard against
double subscription the way `useJobsStore.startListening` does with its `listening` flag.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] Rust tests in `tracks.rs`, each naming its invariant:
  - `test_list_tracks_returns_them_in_project_order`
  - **`test_a_malformed_sidecar_costs_one_track_not_the_library`** — three ids, the middle one
    corrupt on disk; two tracks come back plus one `Malformed` warning. The invariant of the whole
    module.
  - `test_a_missing_sidecar_is_a_warning_not_a_silence`
  - `test_list_tracks_is_empty_for_a_project_with_none`
- [ ] Frontend tests in `state/library.test.ts`:
  - `duration` — `120.0 -> '2:00'`, `59.0 -> '0:59'`, `119.6 -> '1:59'` (floored, not rounded),
    `null -> '--'`
  - `name` — a `null` title and a `'   '` title both fall back to the id
  - `loras` — two entries produce stems in order with strengths; a disabled entry is absent; the
    stored path in the input is untouched
  - **a track with `promptId: null` produces a complete row** — the producer's own `tr-0001`, and
    the case that would blank the first thing this view ever renders
  - `warningLine` — `null` for none; a sentence for some
- [ ] Mutation: for each of the four `state/library.ts` fallbacks (title, model, duration, loras),
      removing it must fail a test. Report any that survive rather than adding an assertion to
      paper over it.
- [ ] No changes outside the listed files. **No component, no CSS.**

## Out of scope

- **`<Library>` and `theme.css`** — T-311e. `theme.css` is 1545 lines and is the working-set risk
  there, not here.
- **Playback.** ARCHITECTURE section 9 (`AnalyserNode` + canvas) has no T-number in this phase.
- **Delete, rename, export, reveal, Send to.** Each is its own task; delete goes to OS trash.
- **Showing `resolved_slots`.** `94.duration` means nothing to a reader; the semantic `spec.inputs`
  are what a person recognises. The resolved values stay in the file, where reproduction needs them.
- **Multiple projects.** One default project, as everywhere else in the app today.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-311c-brief.md --read CONVENTIONS.md --read crates/library/src/lyrics.rs --read crates/create-core/src/provenance.rs --read crates/create-core/src/project.rs --read src-tauri/src/projectctx.rs --read app/src/bridge/loras.ts --read app/src/bridge/jobs.ts --read app/src/state/queue.ts --read app/src/state/jobs.ts --file crates/library/src/tracks.rs --file crates/library/src/lib.rs --file src-tauri/src/library.rs --file src-tauri/src/lib.rs --file app/src/bridge/library.ts --file app/src/state/library.ts --file app/src/state/library.test.ts
```

`lyrics.rs` is the module `list_tracks` copies. `loras.ts` and `jobs.ts` are the bridge templates,
`queue.ts` the pure-decisions template and the deliberate twin of `library.ts`, `jobs.ts` the store
template. `provenance.rs` and `project.rs` define the types the new code constructs — WORKFLOW
section 3's rule for `--read` over `--file`.
