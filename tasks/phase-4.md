# Phase 4 — Library & Player

Goal: the tracks Phase 3 files become a real library — playable, organisable into albums,
deletable, renamable, exportable, and handable to the mixing/mastering apps — with the recipe
that made each one inspectable and reusable.

**Read first:** [ARCHITECTURE 8](../ARCHITECTURE.md) (library/provenance/storage), [9](../ARCHITECTURE.md)
(player & visualizer), and the Send-to note in 8. Then the Phase 3 handoff in
[PROJECT.md](../PROJECT.md) — Phase 3 closed 2026-08-30 with T-301 … T-317 landed and all five
ROADMAP milestone lines discharged.

**The habit Phase 3 ended on, restated because this phase is where it gets expensive again.**
Every defect the Phase 2 and Phase 3 milestones found came from a person clicking, and most were
invisible to `tsc`, `oxlint` and the whole suite because they were **correct logic derived inline
in a view**. The Library is the most stateful view yet — a player, an album organiser, per-track
actions, a provenance inspector — so a value derived in JSX here is a defect nothing in the gate
can see. Pull every decision into the store where a test can reach it.

---

## Phase-start check — DONE 2026-08-30

ROADMAP Phase 4 says: *"re-read the mixing/mastering repos — if their file-handoff protocol has
landed by then, implement against it instead of the v1 link-out."*

**It has not.** Both `../latent-mixing` and `../latent-mastering` are **web-first** (browser,
`vite`/`wasm-pack`, no desktop handoff). Neither has a landed file-handoff protocol; the closest
thing is a shared feedback endpoint, which is unrelated. **Send-to stays the v1 link-out +
reveal-file** exactly as ARCHITECTURE 8 already specifies: open `https://app.latentmixer.com` /
`https://app.latentmastering.com` in the browser and reveal the file for drag-in. The real
handoff protocol is still owned by those repos and does not exist yet.

**Owner decisions this session (2026-08-30):**
1. **Projects become first-class** — multiple projects, create/switch, generation targets the
   selected project. Not the single-project model.
2. **Milestone-first ordering** — playback+visualizer, album list, and send-to land before
   delete/rename/export/reveal and the provenance inspector.

**Owner decisions 2026-09-01**, from a progress review that read this file against the repo and
against the producer's own app data:

3. **T-404 ships as the v1 link-out now**, without waiting for the sibling apps. Re-checked
   2026-09-01: neither `../latent-mixing` nor `../latent-mastering` has an import surface, and
   latent-mixing's own docs plan a *mixing -> mastering* handoff, not a *create ->* one. The real
   pass-off is mostly work in those repos; when it lands it is a **new task here, not a change to
   T-404**.
4. **Delete covers every kind of created content, not only tracks** -- lyric versions, lyric
   documents, albums and projects (T-408). A lyric version a track's provenance points at is
   **refused, with the reason named**, not deleted-and-rendered-missing.
5. **A song title named once is carried to the track and to the exported file** (T-409), on
   `GenerationSpec` rather than resolved at ingest.

---

## What already exists (verified against the repo, not assumed)

- **`library::tracks`** (T-311a) has `mint_track_id`, `save_track`, `load_track`, `list_tracks`,
  `duration_of`, `tracks_dir`, `sidecar_path`, `audio_path`. **No rename, no delete, no export.**
- **`library::projects`** has `create_project`, `load_project`, `save_project`, `list_projects`,
  `slugify`, `project_dir`. **No delete, no rename.**
- **`AlbumList`** exists in the schema (`create_core::project::Project.albums`) with `name` +
  `tracks: Vec<TrackId>`. **No library functions, no commands, no UI.**
- **`projectctx::default_project`** (renamed `selected_project` by T-401a) is the single-project
  seam: every command (`generate`, `ingest`, `lyricdoc`, `tracks`) resolves to "first project in
  slug order, or create `My First Song`". This is what T-401 replaces.
- **`Track.file` is relative** (`tracks/tr-0001.flac`); nothing resolves it to an absolute path
  the webview can play. Playback needs a command that does.
- **`tauri-plugin-opener`** (2.5.4) is already a dependency and registered; its JS surface is
  `openUrl` / `openPath` / `revealItemInDir`. **`tauri-plugin-dialog`** is also registered, with
  `save()` for export.
- **The asset protocol is not enabled.** `tauri.conf.json` has `csp: null` and no
  `assetProtocol` block. Playback needs `assetProtocol: { enable: true, scope: [...] }` plus a
  CSP `media-src asset: http://asset.localhost`, and `convertFileSrc` from `@tauri-apps/api/core`.
- **`trash` 5.2.6** (MIT, permissive) is the OS-trash delete crate; not yet a dependency.

**Added 2026-09-01, from the progress review:**

- **Nothing in `library` deletes anything.** `projects.rs` is create/save/load/list, `lyrics.rs`
  create/save/load/list, `tracks.rs` mint/save/load/list/resolve, `albums.rs`
  create/rename/add/remove/reorder. There is **no `delete_album`** either -- T-403 did not build
  one. `trash` is still not a dependency.
- **A project can hold exactly one lyric document.** `lyricdoc::lyrics_open` returns
  `project.lyrics.first()` or creates one, and there is no `lyrics_create` command. That file's
  own header says so ("until Phase 4's Library view, there is exactly one project and one working
  document"), and Phase 4 never picked it up. `Project.lyrics` is a `Vec` and `mint_doc_id`
  already never reuses a number: **the schema is ready, the command layer is not.**
- **`Track.title` and `LyricDoc.title` both exist and are never set.** `ingest.rs:147` hardcodes
  `title: None`; nothing anywhere writes `LyricDoc.title`. `state/library.ts` already falls back
  to the id for display, which is the only reason untitled tracks read as `tr-0002` rather than as
  a blank row.
- **`GenerationSpec` has no title field** -- `profile_id`, `inputs`, `loras`,
  `lyrics: Option<LyricRef>`, and that is all.
- **Measured on the producer's app data, 2026-09-01** (`%APPDATA%\com.latentbeats.create`):
  `my-first-song` holds **31 versions in one document** (`ld-0001`, `title: null`, `approved: 31`)
  and **20 tracks, every one `title: null`**; `testproject` holds 1 version and 2 tracks. **19 of
  the 20 sidecars reference `ld-0001` version 31** -- the approved one -- and one references no
  lyric at all. Two numbers that shape T-408 and T-409: under decision 4, **30 of the 31 versions
  are free to delete and version 31 is pinned by 19 tracks**, and the lyric-less track is the case
  that leaves ingest with no title to resolve.

---

## Tasks

Briefs are written one at a time, each after the previous lands. Each gets its own
`tasks/t-4NN-brief.md` when written.

Ordering principle: **the milestone line first** (playback+visualizer, album list, send-to),
then the track actions, then the provenance inspector — with the multi-project foundation first
because every later task operates on a project and the Library's "project/track lists" is part of
the phase scope. The same shape that made Phase 3's pure half testable applies here: everything
that can be tested without a running ComfyUI comes first, and the player/visualizer is the only
part that needs a real audio file.

### T-401 — projects become first-class — **the foundation**
The single-project seam (`projectctx::default_project`, renamed `selected_project` by T-401a)
becomes a selected-project seam. This is the cross-cutting refactor every later task builds on,
and it is the one task that touches every command.

**Complete 2026-08-30.** Split into [T-401a](t-401a-brief.md) (backend seam: the config field,
`projectctx` resolution, `projects_list`/`projects_create`, the four call sites — landed) and
[T-401b](t-401b-brief.md) (frontend picker — landed); split so each run stayed under the ~400-line
rule. **Click-through passed 2026-08-30:** a track generated with a second project selected lands
in `projects/<slug>/tracks/` and the Library shows it under that project.

Scope:
- **Config** gains `default_project_slug: Option<String>` (persisted like `default_profile_id`).
- **`projectctx`** resolves the *selected* project: the configured slug if it exists, else the
  first project, else create `My First Song` — and the selection is what `generate`, `ingest`,
  `lyricdoc` and `tracks` all target. The "first project or create" fallback stays as the
  bootstrap, but the selection is now explicit and persisted.
- **New commands**: `projects_list` (the `ProjectSet`) and `projects_create(name)`.
  **`projects_select` is deliberately not built** — the selection persists through the existing
  `save_config` path exactly like `default_profile_id` (T-303), so the config store stays the
  single writer of config (decision log 2026-08-30). `library_tracks` and the lyric commands
  already resolve through `projectctx`, so they follow the selection for free once it is
  persisted.
- **Frontend**: a project picker in the Library view (list + create + select), a
  `state/projects.ts` store, and the config store gains the slug.

**The trap to design against:** `default_project` is called in four places, and the symptom of a
half-done refactor is a track saved into one project while its lyrics sit in another — the exact
"two correct layers deaf together" failure Phase 3 hit repeatedly. The test that matters asserts
that `generate`, `ingest`, `lyricdoc` and `tracks` all resolve to the *same* project for a given
selection, not just that each resolves to *a* project.

### T-402 — playback + visualizer — **milestone line**
The player: `<audio>`/Web Audio → `AnalyserNode` FFT → canvas spectrum + waveform. Read-only
visualizer, zero custom DSP (ARCHITECTURE 9).

**Briefed 2026-08-30.** Split three ways to stay under the ~400-line rule (the T-401 pattern):
[T-402a](t-402a-brief.md) (backend + config: the asset protocol, the CSP, `resolve_track_file`,
and the `track_audio_path` command), [T-402b](t-402b-brief.md) (the player state machine: the
`trackAudioUrl` bridge wrapper, `state/player.ts` and its pure fold), and [T-402c](t-402c-brief.md)
(the `Player` + `Visualizer` components, the Library play button, and the CSS). Executed one at a
time, in that order. **T-402d** ([brief](t-402d-brief.md)) is the click-through fix: the asset
protocol is cross-origin from the page, so `createMediaElementSource` emitted silence until the
`<audio>` element went `crossOrigin="anonymous"`, and the `AudioContext` now resumes on `play`.

Scope:
- **Enable the asset protocol** in `tauri.conf.json` (`assetProtocol.enable` + scope over the app
  config dir) and a CSP `media-src asset: http://asset.localhost`. This is the one change the
  gate cannot check — `npm run gate` runs `vite build`, never `tauri build` — so it is a
  producer click-through item, not a CI item.
- **`track_audio_path` command**: resolve `Track.file` (relative) to an absolute path, so the
  frontend can `convertFileSrc` it. Refuses a path that escapes the project, the same whitelist
  discipline as `sidecar_path`.
- **`<Player>` + `<Visualizer>`**: play/pause/seek, spectrum + waveform on canvas. The
  visualizer is a clean-room reimplementation against `AnalyserNode` (ARCHITECTURE 9) — no code
  from the sibling repos.
- **Frontend**: a `state/player.ts` store (the playing track, position, playing state), and the
  Library view gains a play affordance per track.

**The trap to design against:** the webview review environment cannot composite frames or fire
`requestAnimationFrame` (WORKFLOW 5), so the visualizer's *drawing* is verified by
`getBoundingClientRect`/store reads or listed as unverified for the producer's click-through —
never silently assumed. The player's *state machine* (play/pause/seek/end) is pure and tested.

### T-403 — album lists — **milestone line**
`AlbumList` (already in the schema) becomes real: create, rename, add/remove tracks, reorder.

**Complete 2026-08-31 (a/b/c); click-through passed.** Split three ways to stay under the
~400-line rule (the T-402 pattern): [T-403a](t-403a-brief.md) (backend: `library::albums` + the
six `albums_*`/`album_*` commands), [T-403b](t-403b-brief.md) (the frontend store:
`bridge/albums.ts`, `state/albums.ts` and its pure `albumRows`/`moveTrackId`), and
[T-403c](t-403c-brief.md) (the Library album panel + CSS), executed one at a time, in that order.
library 58 -> 74 tests, frontend 355 -> 373. The producer ran the six click-through steps on a
built app, all passed, and **this milestone line is discharged.**

Scope:
- **`library::albums`**: functions over `Project.albums` — create, rename, add track, remove
  track, reorder. All operate on the project record, which already holds `albums`; no new files.
- **Commands**: `albums_list`, `album_create`, `album_rename`, `album_add_track`,
  `album_remove_track`, `album_reorder`.
- **Frontend**: an album view in the Library — list albums, open one, see its tracks in order,
  add/remove/reorder. A `state/albums.ts` store.

**Three design decisions, recorded rather than guessed (T-403a):**
1. **Albums are name-addressed; names are unique within a project.** The schema has no album id
   and gets none: albums never map to a path, so the name is a safe handle the way a slug is not.
   A duplicate name is refused at create and rename ("choose another name"), so "open this album"
   is never ambiguous.
2. **A reorder is a full-order replace, validated as a permutation.** The frontend computes the
   new order after an up/down move and sends the whole list; the backend refuses any list that is
   not exactly the album's current tracks rearranged. A stale frontend can never silently wipe
   part of an album, and there is no move-one-off-by-one arithmetic to get wrong on the wire.
3. **`add_track` refuses an id the project does not own.** Adding is the one moment a dangling id
   can be prevented; deleting is the only legitimate source of one, and the frontend renders those
   as "Missing track" rather than dropping them (the trap below).

**The trap to design against:** `AlbumList.tracks` holds `TrackId`s, and a deleted track's id
must not be handed to a later one (the `mint_track_id` invariant). An album still holding a
deleted id must render as "missing" rather than silently dropping the entry — the same
absent-versus-empty discipline that has produced four bugs in this repo. The join lives in
`state/albums.ts` `albumRows`, where a test guards it (the `?? null` fallback is the mutation
check).

### T-404 — Send-to — **milestone line**
The v1 link-out: open the mixing/mastering site and reveal the file for drag-in.

**Briefed 2026-09-01** ([t-404-brief.md](t-404-brief.md)), split by lane rather than by size:
**T-404a** (backend) is architect-direct and **landed 2026-09-01** -- `src-tauri/src/sendto.rs`,
the `send_to` command, `target_url`, and four tests; src-tauri 107 -> 111. **T-404b** (frontend:
`bridge/sendto.ts`, `state/sendto.ts` + tests, the `TrackCard` affordance, the CSS) is the Aider
run, with its launch command in the brief. **T-404b landed 2026-09-01** and the two files of pure
reference transcribed byte-identically; frontend 373 -> **382**. Review found three defects,
all fixed directly (WORKFLOW 2): the run **deleted `.track-head-actions`**, the flex rule the
row's whole action cluster hangs off, while merging the selector list next to it; and two
mutations survived the seven tests it wrote -- hardcoding the destination to `'mixing'` and
dropping the success path's `sending` reset both left the suite green. Two tests added, both
mutations re-run and dead. **Click-through PASSED 2026-09-01, all five steps** -- both
destinations open their own site with the file revealed, a missing file gives the sentence and
opens nothing (and works again once restored), the window order is good, and the error moves
with the row rather than multiplying. **T-404 is complete, and the third and last Phase 4
milestone line is discharged.**

Verified while briefing, and worth keeping: **no capability change is needed** (`opener:default`
already grants both permissions, and the Rust API does not consult the JS scope at all), and
**`reveal_item_in_dir` canonicalizes first**, so a deleted file is an error rather than a no-op --
which is why the command checks `is_file()` itself and reveals *before* opening the browser.

**The URLs were re-verified and ARCHITECTURE 8 is right.** `../latent-mixing` mentions
`latentmixing.com` 59 times against `latentmixer.com` 17, and the majority is stale: that repo's
2026-08-08 entry records the app deployed at `app.latentmixer.com` with a doc sweep still owed, and
the live `latentbeats.com` links `app.latentmixer.com` / `app.latentmastering.com`.

Scope:
- **`send_to` command** (or two): `openUrl` to `https://app.latentmixer.com` /
  `https://app.latentmastering.com` and `revealItemInDir` on the resolved audio path. The
  opener plugin is already registered; this is a thin command over it.
- **Frontend**: a "Send to" affordance per track, offering Mixing and Mastering.

**The trap to design against:** the URLs are the only place the app hardcodes the sibling apps'
addresses, and they are a product decision, not a constant to bury. Keep them in one place and
say in the code that the real handoff protocol is owned by those repos (ARCHITECTURE 8).

### T-405 — track actions: delete, rename, export, reveal
The per-track actions the milestone line does not require but the phase scope names.

**Briefed 2026-09-01** ([t-405-brief.md](t-405-brief.md)), split three ways by lane. **T-405a
(backend) is architect-direct and landed 2026-09-01**: `library::tracks::delete_track` /
`rename_track` / `export_track` / `trash_to_os`, the `trash = "5.2"` (MIT) dependency, a
`LibraryError::Trash` variant, and the four Tauri commands (`delete_track`, `rename_track`,
`export_track`, `reveal_track`). **library 74 -> 84**, 10 new tests, 5 mutations run by hand and
all killed. **T-405b** (`bridge/tracks.ts`, `state/trackActions.ts` + tests) and **T-405c** (the
`TrackCard` controls + CSS) are the two Aider runs, each with its own launch command in the brief;
c is the only part with a producer click-through. **T-405b and T-405c landed 2026-09-01** and the store transcribed byte-identically; frontend 382 -> **395**. Review found the two failure tests never armed `confirming`/`renaming` before acting, so "keeps it set" passed vacuously (the store was correct) -- fixed to arm the precondition, and the clear-on-failure mutation now dies. **Click-through PASSED 2026-09-01, all six steps** -- delete moves the `.flac` and its sidecar to the OS Recycle Bin (not gone), a deleted id is never reused, rename persists and clears to the id, export copies while leaving the original, reveal selects the file, and a row's error stays on its row. **T-405 is complete.**

Verified while briefing: **`trash::delete` canonicalizes first and errors on a missing path**, so
delete guards each file with `exists()` and a retry after a partial delete self-heals; and **`trash`
moves to the real Recycle Bin**, so the trash operation is *injected* (production `trash_to_os`,
tests a recording fake) rather than called directly -- the only way `cargo test` avoids filling the
developer's trash, and what lets the CONVENTIONS "assert the trash call, not that the file is gone"
test exist.

Scope:
**T-405 is where `trash` enters the workspace**, and T-408 reuses both the dependency and the
discipline (ordering note below). Rename is also how the producer's 20 existing untitled tracks
get titles at all -- T-409 sets a title at ingest and does not backfill.

- **Delete → OS trash** (`trash` crate, never hard delete — CONVENTIONS). Removes the audio file
  and the sidecar, and unlists the id from `Project.tracks` (and from any album). The id is
  **not** reused (`mint_track_id` invariant).
- **Rename**: sets `Track.title` (the sidecar is the single source of truth — ARCHITECTURE 8).
- **Export**: copy the audio file to a user-chosen location via `tauri-plugin-dialog`'s `save()`.
- **Reveal**: `revealItemInDir` on the resolved path.

**The trap to design against:** delete is the one destructive action in the app, and the
CONVENTIONS rule is "delete moves to OS trash, no hard deletes". The test that matters asserts
the file is *not* hard-deleted (the `trash` call is made, not `fs::remove_file`), and that a
deleted track's id is not handed to the next generation.

### T-406 — provenance inspector — "re-use these settings"
The recipe that made a track, shown and reusable.

Scope:
- **Frontend**: a provenance panel per track — the `GenerationSpec` (semantic inputs), the
  resolved slots, the LoRA stack, seed, lyric ref, licence, template, comfy server info. The
  "re-use these settings" action loads the spec back into the Audio Studio's param panel and
  LoRA stack (the `GenerationSpec` is already the shape `specFor` builds, so this is a store
  handoff, not a new type).

**The trap to design against:** "re-use these settings" must not silently re-roll the seed the
way a fresh Generate does (T-316) -- the user is asking to reproduce a specific track, so the
seed is pinned, not re-rolled. The test that matters asserts the loaded spec carries the
sidecar's seed verbatim.

### T-407 - shared scrollbar styling - **styling debt pulled forward**
Not a milestone task: the first entry in [docs/CSS-TODO.md](../docs/CSS-TODO.md) (found
2026-08-27, T-301b click-through), pulled forward and paid 2026-09-01 at the producer's request,
out of milestone order.

**Complete 2026-09-01; landed directly as architect work** (the T-207/T-208 lane: a change this
small, written and verified by the architect, is not worth an executor round trip).

Scope:
- One shared rule in `theme.css`, not per view: standard `scrollbar-width: thin` /
  `scrollbar-color: var(--border-bright) transparent` for Firefox, plus a `::-webkit-scrollbar`
  treatment (10px, rounded thumb, muted hover, transparent track) for the WebView2/Chromium the
  shipped app runs in. Tokens only, no forked values, nothing hardcoded.

**The trap to design against:** solving it per view (the model list gets a rule, then the next
overflowing list rediscovers the gap). The rule is global, so any pane that can overflow --
nav rail, content pane, model/profile/project lists, the lyric draft -- is covered the moment
it overflows, including views that do not exist yet.

**Manual verify (producer click-through):** in a built app, open the Setup wizard's lyric-model
step against the QwenCloud endpoint (163 models) -- the list's scrollbar is a thin rounded thumb
in the border-bright tone against the dark ground, not the browser default, and it brightens on
hover. Same treatment on the main content pane scroll and the profile/project pickers.


### T-408 - deleting created content
Delete for everything the app makes, not only tracks. Opened 2026-09-01 by owner decision 4: the
phase scope named a track delete, and the app has none for lyric versions, lyric documents, albums
or projects, so a person testing one song accumulates 31 lyric versions with no way to remove one.

Scope:
- **a. Delete a lyric version.** `library::lyrics` gains a delete that **refuses when any track in
  the project references that `(doc_id, version)`** and names the track(s) holding it -- the
  refusal is the feature, not a limitation to work around. Versions are **never renumbered**:
  `push_version` already counts from the highest present, so a hole is legal, and renumbering
  would silently repoint every sidecar's `LyricRef`. **Backend landed 2026-09-01**
  ([t-408a-brief.md](t-408a-brief.md), architect-direct, six mutations killed):
  `library::lyrics::delete_version`, `LibraryError::VersionReferenced`, the `lyrics_delete_version`
  command; library 84 -> 92. **a-front (the Lyrics Studio delete affordance) landed the same day**
  (architect-direct; frontend 395 -> 399), a `VersionRow` Delete with an inline confirm. **Producer
  click-through passed all five steps** (v31 correctly refused, naming the track); one defect found
  and fixed -- the refusal message now renders inline at its row rather than the top of a 31-item
  list. **Part a is complete.**
- **b. Many lyric documents per project, and delete a document.** `lyrics_create`, `lyrics_list`,
  `lyrics_open(id)` and a document picker in Lyrics Studio, retiring the Phase 2 one-document
  shortcut. A document is deletable only when none of its versions is referenced -- the same rule
  as (a), applied to the whole file.
- **c. Delete an album.** `library::albums::delete_album`, an `album_delete` command, and the
  affordance in the album panel. Deleting a list never touches the tracks in it.
- **d. Delete a project.** The whole `projects/<slug>/` tree to OS trash -- tracks, sidecars,
  lyrics, `project.json`. Deleting the *selected* project falls through `selected_project`'s
  existing "configured slug that no longer exists" arm (decisions log 2026-08-30); that arm is
  built and tested but has never been exercised by anything, and this task is the first thing
  that can reach it.

**The traps to design against:**
1. **OS trash, never `fs::remove_file`** (CONVENTIONS). The test that matters asserts the `trash`
   call was made, not that the file is gone -- a hard delete passes the second check and fails the
   rule.
2. **Ids are never reused.** `next_lyric_seq` and `next_track_seq` are monotonic *because of*
   delete, and until this task nothing in the repo could prove it. A test that deletes and then
   mints is the first real exercise of the invariant both fields' doc comments describe.
3. **A refusal that does not name its obstruction is a dead end.** "Cannot delete this version"
   with no subject is the failure mode; it says which track holds it, the way the album panel says
   "Missing track".
4. **A delete that half-succeeds.** File removed, id still listed (or the reverse) is the
   two-layers-deaf-together shape this repo has hit repeatedly. Order the writes so the record is
   the last thing changed, and test the interrupted path, not only the happy one.
5. **Album membership.** A deleted track must leave every `AlbumList` that holds it -- T-403
   renders a dangling id as "Missing track", which is the safety net, not the plan.

### T-409 - the song title, carried
A title named once in Lyrics Studio reaches the track, the Library and the exported file. Opened
2026-09-01 by owner decision 5. The field exists at both ends and connects to nothing: `Track.title`
is hardcoded `None` at ingest and `LyricDoc.title` has never been writable.

Scope:
- **`LyricDoc.title` gets a UI** in Lyrics Studio. The field is already in the schema and in
  `bridge/lyricdoc.ts`; only the input is missing.
- **`GenerationSpec` gains `title: Option<String>`**, prefilled in the Audio Studio from the
  selected lyric document and editable there. Resolving it at ingest from `spec.lyrics.doc_id` was
  the alternative and is **rejected on evidence**: one of the producer's 20 tracks carries no
  lyric ref at all, so ingest would have no title source for it, and provenance should record what
  the user chose rather than what a second file happened to say later.
- **`ingest.rs` sets `Track.title` from the spec** instead of the hardcoded `None`.
- **Export (T-405) offers the title as the default filename**, sanitised for the filesystem, with
  the OS save dialog handling collisions -- a batch of five is five tracks with one title.
- **The Library shows it.** `state/library.ts`'s id fallback stays, for untitled and pre-existing
  tracks.

**The traps to design against:**
1. **The audio file on disk keeps its id name.** `tracks/tr-0007.flac` does not become
   `Midnight.flac`. ARCHITECTURE 8: the id addresses the file and the sidecar is the only truth;
   a title in the filename puts one fact in two places for the first rename to break. The title is
   a **display-and-export** name.
2. **`Track.title` is a snapshot, not a link.** Renaming the lyric document later must not retitle
   tracks already made from it -- the same one-source-of-truth rule.
3. **Titles are not unique and are not ids.** Albums are name-addressed (T-403 decision 1);
   tracks stay id-addressed.
4. **Sanitise before the dialog, not after.** A title with `/`, `:` or a trailing dot is legal in a
   `LyricDoc` and illegal as a Windows filename; the symptom is the OS refusing the save with its
   own message instead of the app preventing it.
5. **`GenerationSpec` is stored in provenance**, so the title lands there for free and T-406's
   "re-use these settings" carries it. The new field is an ARCHITECTURE 5/7 interface change, so
   that doc edit lands in the **same commit** as the code (AGENTS).

---

## Ordering, set 2026-09-01

**T-404 -> T-405 -> T-408 -> T-409 -> T-406.** The owner left the order to judgement; the
dependencies decide most of it.

- **T-404 first** -- it discharges the last milestone line and depends on nothing.
- **T-405 before T-408** -- T-405 brings `trash` into the workspace and establishes the
  delete-to-trash discipline once, and T-408 reuses both. T-405's rename is also the only way the
  20 tracks that already exist ever get titles, since T-409 sets a title at ingest and does not
  backfill.
- **T-408 before T-409** -- the two are independent, but the 31 accumulated lyric versions are the
  live pain, and T-409's export half needs T-405's export to exist anyway.
- **T-406 last**, unchanged -- it is the only task nothing else waits on, and T-409 adds one field
  to the spec it inspects.

**Not in this phase:** the real file handoff to the mixing/mastering apps. It is mostly work in
those repos (decision 3 above); when an import surface lands there, it opens as its own task here.

---

## Milestone check (live)

**generate → play with visualizer → album list → send-to opens site with file revealed.**

**DISCHARGED 2026-09-01**, in four dated click-throughs rather than one sitting: generate (Phase 3,
T-311), play with visualizer (T-402, 2026-08-30), album list (T-403, 2026-08-31), send-to (T-404,
2026-09-01). **The phase's milestone check is met; the phase is not finished** -- T-405, T-408,
T-409 and T-406 remain, and they are the half of the scope the milestone line never covered.

Run by the producer at the end of the phase, from a checklist in the phase file. The automated
steps (store logic, library functions, command wiring) are covered by tests; the visualizer's
drawing, the asset-protocol playback, and the send-to reveal are producer click-through items —
the same split Phase 3 used, where every defect came from a person clicking.
