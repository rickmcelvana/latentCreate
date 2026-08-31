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

---

## What already exists (verified against the repo, not assumed)

- **`library::tracks`** (T-311a) has `mint_track_id`, `save_track`, `load_track`, `list_tracks`,
  `duration_of`, `tracks_dir`, `sidecar_path`, `audio_path`. **No rename, no delete, no export.**
- **`library::projects`** has `create_project`, `load_project`, `save_project`, `list_projects`,
  `slugify`, `project_dir`. **No delete, no rename.**
- **`AlbumList`** exists in the schema (`create_core::project::Project.albums`) with `name` +
  `tracks: Vec<TrackId>`. **No library functions, no commands, no UI.**
- **`projectctx::default_project`** is the single-project seam: every command (`generate`,
  `ingest`, `lyricdoc`, `tracks`) resolves to "first project in slug order, or create
  `My First Song`". This is what T-401 replaces.
- **`Track.file` is relative** (`tracks/tr-0001.flac`); nothing resolves it to an absolute path
  the webview can play. Playback needs a command that does.
- **`tauri-plugin-opener`** (2.5.4) is already a dependency and registered; its JS surface is
  `openUrl` / `openPath` / `revealItemInDir`. **`tauri-plugin-dialog`** is also registered, with
  `save()` for export.
- **The asset protocol is not enabled.** `tauri.conf.json` has `csp: null` and no
  `assetProtocol` block. Playback needs `assetProtocol: { enable: true, scope: [...] }` plus a
  CSP `media-src asset: http://asset.localhost`, and `convertFileSrc` from `@tauri-apps/api/core`.
- **`trash` 5.2.6** (MIT, permissive) is the OS-trash delete crate; not yet a dependency.

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
The single-project seam (`projectctx::default_project`) becomes a selected-project seam. This is
the cross-cutting refactor every later task builds on, and it is the one task that touches every
command.

Scope:
- **Config** gains `default_project_slug: Option<String>` (persisted like `default_profile_id`).
- **`projectctx`** resolves the *selected* project: the configured slug if it exists, else the
  first project, else create `My First Song` — and the selection is what `generate`, `ingest`,
  `lyricdoc` and `tracks` all target. The "first project or create" fallback stays as the
  bootstrap, but the selection is now explicit and persisted.
- **New commands**: `projects_list` (the `ProjectSet`), `projects_create(name)`, and
  `projects_select(slug)` (persists the selection). `library_tracks` and the lyric commands
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

Scope:
- **`library::albums`**: functions over `Project.albums` — create, rename, add track, remove
  track, reorder. All operate on the project record, which already holds `albums`; no new files.
- **Commands**: `albums_list`, `album_create`, `album_rename`, `album_add_track`,
  `album_remove_track`, `album_reorder`.
- **Frontend**: an album view in the Library — list albums, open one, see its tracks in order,
  add/remove/reorder. A `state/albums.ts` store.

**The trap to design against:** `AlbumList.tracks` holds `TrackId`s, and a deleted track's id
must not be handed to a later one (the `mint_track_id` invariant). An album still holding a
deleted id must render as "missing" rather than silently dropping the entry — the same
absent-versus-empty discipline that has produced four bugs in this repo.

### T-404 — Send-to — **milestone line**
The v1 link-out: open the mixing/mastering site and reveal the file for drag-in.

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

Scope:
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
way a fresh Generate does (T-316) — the user is asking to reproduce a specific track, so the
seed is pinned, not re-rolled. The test that matters asserts the loaded spec carries the
sidecar's seed verbatim.

---

## Milestone check (live)

**generate → play with visualizer → album list → send-to opens site with file revealed.**

Run by the producer at the end of the phase, from a checklist in the phase file. The automated
steps (store logic, library functions, command wiring) are covered by tests; the visualizer's
drawing, the asset-protocol playback, and the send-to reveal are producer click-through items —
the same split Phase 3 used, where every defect came from a person clicking.
