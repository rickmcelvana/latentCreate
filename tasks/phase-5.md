# Phase 5 — Cover art, model catalog, polish, packaging

> **OPEN — 2026-09-02.** Phase-start check done (below). Task map laid out; **briefs are written one
> at a time**, each after the previous lands (the Phase 3/4 habit). No git tag on close is the
> recent precedent (Phase 3, Phase 4 both closed docs-only).

Goal: the app becomes something a stranger can install and use end to end — bring in **new audio
and image models from inside the app** (not by hand-importing a workflow), generate **cover art**
over an image model the same way audio is generated, clear the player/visualizer/lyrics polish the
Phase 4 producer flagged, and ship an **installable build** on a machine that never had the dev
toolchain.

**Read first:** [ARCHITECTURE §10 + §10a](../ARCHITECTURE.md) (setup flow and the model catalog),
[§9](../ARCHITECTURE.md) (player & visualizer), [§8](../ARCHITECTURE.md) (library/provenance), and
**[docs/MCP-SURFACE.md §32](../docs/MCP-SURFACE.md)** — the live image/catalog surface verification
this phase is built against. Then the Phase 4 close in [PROJECT.md](../PROJECT.md).

**The habit, restated because this is where it stays expensive.** Every milestone defect in Phases
2–4 came from a person clicking, and most were invisible to `tsc`/`oxlint`/the suite because they
were **correct-looking logic derived inline in a view**. Cover art adds a second generation view,
and the catalog adds a stateful browse/install surface — pull every decision into a store a test
can reach.

---

## Phase-start check — DONE 2026-09-02

ROADMAP Phase 5 says: *re-verify the image/cover-art comfy-mcp surface before any brief.* Done live
against the running local server — full write-up in **[docs/MCP-SURFACE.md §32](../docs/MCP-SURFACE.md)**.
Headlines:

- **ComfyUI up**, RTX 5060 Ti (15.93 GiB), comfy-cli **1.16.0**, core **v0.34.2** (current).
- **comfy-mcp has no model-hub search.** No CivitAI/HuggingFace registry tool exists. The one
  browsable-before-install surface is the **built-in workflow-template gallery** (`search_templates`,
  ~558 rows, 24h cache) — each row a *workflow + model* bundle. That gallery **is** the catalog.
- **Both audio and image ride one surface.** The gallery is the same one Phases 1–3 used for audio
  (ACE-Step, MiniMax); a `flux text to image` + `exclude_api:true` query returned 13 local image
  templates (Flux.2 Dev / Klein 4B/9B / Chroma, Flux.1 Krea/Dev/Schnell).
- **Readiness = `fetch_template` → `local_check`; install = `download_model` (known URL).**
  `search_models` is local-readiness only, not discovery.
- **A paid hosted/cloud tier exists** (`list_partner_models` / `partner_generate` — 40 models:
  Flux Pro, Ideogram, Seedance) but spends account credits and downloads nothing.

**Owner decisions (2026-09-02):**
1. **The model catalog is a first-class Phase 5 feature** — it was in the original plan, survived
   only as a half-line "advanced expander" in ARCHITECTURE §10 (which wrongly implied `search_models`
   was a discovery search), and was **never built**. It is the point of the Setup page beyond LLM
   selection. Now designed in [ARCHITECTURE §10a](../ARCHITECTURE.md).
2. **Local-only for v1.** The catalog surfaces the free local template gallery (browse → download to
   the user's own GPU). The paid hosted/cloud tier (`list_partner_models`) is a **documented later
   task** (ROADMAP Future), not v1 — keeps the "ship no models, runs on the user's own ComfyUI"
   shape clean.
3. **The catalog reuses T-313's import-to-profile machinery.** A gallery template is a workflow the
   user did not have to hunt for; adopting one is the same role-suggest → confirm → copy-not-reference
   path, so no second import mechanism is built.
4. **Three Phase-4 polish items the producer flagged are pulled into Phase 5** (below): the lyrics
   document picker floats without a card, the Library player sits off-screen at the page foot, and
   the visualizer upscales a fixed 640×120 canvas so it blurs in full screen.

---

## What already exists (verified against the repo, not assumed)

- **CoverArt view is a stub** — [app/src/views/CoverArt.tsx](../app/src/views/CoverArt.tsx) renders
  a "No artwork yet" placeholder. No image generation path exists.
- **Setup has three steps** — ComfyUI, Lyrics model, Models — and the Models step only
  readiness-checks shipped audio profiles ([app/src/views/Setup.tsx](../app/src/views/Setup.tsx)).
  **No search UI, no image-model step, no catalog.**
- **The install seam exists** — Phase 1's `download_model` + `download` progress path installed
  ACE-Step through the app. The catalog reuses it.
- **The import-to-profile machinery exists** — T-313 (`app/src/components/ImportWorkflow.tsx`,
  `state/import.ts`, the role-suggestion + emission path). The catalog's "adopt" step reuses it.
- **The player is in-flow at the Library page foot** — [Library.tsx:122](../app/src/views/Library.tsx),
  `.player` has only `margin-top` ([theme.css:2161](../app/src/theme.css)). Not sticky/fixed.
- **The visualizer canvas is fixed 640×120** — [Visualizer.tsx:91](../app/src/components/Visualizer.tsx)
  — while `.visualizer` CSS stretches it to `width:100%` ([theme.css:2237](../app/src/theme.css)).
- **The lyrics document picker is a bare `<div>`** — [LyricsStudio.tsx:300](../app/src/views/LyricsStudio.tsx),
  `.doc-picker` has only `margin-bottom` ([theme.css:959](../app/src/theme.css)); its siblings are
  `panel` cards.

---

## Task map (scoped; briefed one at a time)

### Polish — Phase-4 carryover (small, independent) — ✅ **COMPLETE: all three landed architect-direct 2026-09-02, producer click-through passed 2026-09-02**
- **T-501 — Lyrics document picker in its own card. ✅ landed.** `DocumentPicker` is now a
  `<section className="panel doc-picker">` capped at `max-width: 720px` to line up with the
  `lyrics-form`/`lyrics-output` cards below it — it had no width cap, which is why it "stuck out to
  the side" at full width. CSS + wrapper only; no store change.
- **T-502 — Player docked on screen with the file. ✅ landed.** `.player` is `position: sticky;
  bottom: 0; z-index: 5` with a lift shadow, so the transport stays at the foot of the scroll pane
  while a track plays. It docks only when loaded — the Player renders nothing when `track === null`,
  so nothing is pinned when idle.
- **T-503 — Sharp visualizer + a better visual. ✅ landed.** The canvas backing store is sized to
  its client size × `devicePixelRatio` via a `ResizeObserver` (kills the full-screen blur); the
  draw was upgraded to gradient bars from the floor + a centered oscilloscope line. The audio-graph
  wiring (`createMediaElementSource` once, analyser → `destination`) is unchanged. Not unit-tested —
  a canvas/`requestAnimationFrame` draw is a click-through item (WORKFLOW §5).

### Model catalog (§10a) — the phase's backbone. **Curated-first + gallery browse** (owner decision 2026-09-02, from the live surface: comfy-mcp gives readiness but **no download URLs**, MCP-SURFACE §33)
- **T-504 — Gallery browse + bare-row readiness (backend seam). ✅ LANDED 2026-09-02** ([t-504-brief.md](t-504-brief.md)).
  `browse_templates` (a `search_templates` variant: `type` + `exclude_api:true` + `offset`) and two
  Tauri commands — `catalog_browse(kind, query, offset)` and `catalog_readiness(name)` returning the
  raw `LocalCheck` tri-state. The Ready/Not-ready/Unknown **verdict is derived in the T-505 store
  (TS)**, matching the repo; `create-core` stays pure (it has no mcp-bridge dep). Three files:
  `mcp-bridge/templates.rs`, new `src-tauri/catalog.rs`, `lib.rs`. **Install is not here** — see below.
  Verified by the live scoping (browse returned 163 image / 19 audio rows; readiness on a
  not-installed template decided) + unit/`#[ignore]`-live tests; **no click-through** possible until
  T-505 wires the UI. mcp-bridge 96→98, app 114→117; gate green.
- **T-505 — Catalog UI, curated one-click install, adopt-to-profile.** Split into lanes:
  - **T-505a — the browse/readiness store (`state/catalog.ts` + `bridge/catalog.ts`). ✅ LANDED 2026-09-02**
    ([t-505a-brief.md](t-505a-brief.md)). Drives the two T-504 commands; derives the Ready/Not-ready/
    Unknown verdict from `LocalCheck` in TS; per-row readiness resolved lazily. Fully unit-tested
    (13 tests), no UI. Three new files; frontend 435→448, gate green.
  - **T-505b — the `<ModelCatalog>` component + Setup wiring. ✅ COMPLETE — click-through passed 2026-09-02** ([t-505b-brief.md](t-505b-brief.md)). The catalog toggle, the 163-row image gallery, search, and readiness pills all worked; a stopped ComfyUI reads "Can't check", never "Not installed". The click-through surfaced a **pre-existing** Models-step defect (not the catalog): a stopped ComfyUI dumped the raw `server_not_running` tool diagnostic instead of the clean "Start ComfyUI above." line. Fixed same-session (architect-direct): `models::inventory_detail` returns `None` for `server_not_running` so the frontend's own fallback speaks; other errors keep their detail. app 117→118.
    **One "Model catalog" Setup step with an Audio | Image toggle** (not two steps) — the T-505a store
    is a singleton, so one kind shows at a time; that both keeps the store correct and delivers the
    owner's "both on the setup page". Renders rows, a debounced search, and per-row readiness pills
    resolved lazily via an IntersectionObserver. No install/adopt buttons yet (c/d). Three files: new
    `ModelCatalog.tsx`, `Setup.tsx` (one line after `ModelsStep`), `theme.css`. Gate green (view
    component, not unit-tested per WORKFLOW §5; frontend 448 unchanged). ARCHITECTURE §10/§10a updated
    to the toggle-step layout in the same commit.
  - **T-505c — curated one-click install. ✅ LANDED 2026-09-02 (awaiting click-through)** ([t-505c-brief.md](t-505c-brief.md)).
    A gallery row whose `name` matches a shipped profile's `comfy.template` (verified live: the
    profiles' `audio_ace_step1_5_xl_turbo` / `audio_minimax_music_3` are the exact gallery `name`s)
    gets the profile's readiness pill + an **Install** button, reusing the Models step's singleton
    `useModelsStore` (`install`/`installView`/`rowFor`) unchanged — an install from the catalog *is*
    the Models step's install. **Curated readiness comes from the profile, never `local_check`** (the
    MiniMax lesson). One new field surfaces the join key: `ProfileStatus.template` (Rust + TS mirror);
    the join `curatedIndex(view)` is a pure, tested TS helper. Bare rows keep T-505b's `local_check`
    path. No new Tauri command, no new install code. Review caught a **rules-of-hooks** bug the gate
    could not (a `CatalogRow` early-returning before its hooks — a race-dependent crash when a row
    turns curated as the models view lands); fixed by branching in the parent between sibling
    `BareRow`/`CuratedRow` components. The required `template` field also needed a `template: null`
    line in an untouched fixture (`profiles.test.ts`) — an eighth file. src-tauri 118→119, frontend
    448→453; gate green. **Click-through passed 2026-09-02 for everything observable** — both curated
    rows (ACE turbo + MiniMax) read **Installed**, matching the Models step, and **MiniMax reads
    Installed, not the `local_check` `runnable:false` trap** (the lane's key correctness proof). The
    Install button could **not** be seen fire: every shipped model on the dev machine is already
    installed (all four ACE turbo files verified present against the live inventory), so no curated
    row is `missing` and the button correctly hides — as it does on the Models step too. The install
    path is the same verbatim `install`/`rowFor` gate as the (proven) Models-step button; it will be
    seen live at **T-506**, whose first image curated profile won't be pre-installed.
  - **T-505d — adopt an installed gallery row into a profile** via the T-313 import path. The "bring
    it in" action for a model the user already has. Split into lanes (backend seam first, per the
    T-504→T-505 cadence):
    - **T-505d-a — the fetch→import backend seam. ✅ LANDED 2026-09-02** ([t-505d-a-brief.md](t-505d-a-brief.md)).
      One command, `catalog_adopt_begin(name)`: fetch the gallery template to a temp file, hand it to
      the **existing** `import_into` (T-313) unchanged, clean up, return the `ImportReport`. Verified
      live 2026-09-03: `fetch_template` writes **frontend format** (`nodes[]`+`links`), so
      `import_into`'s format gate accepts it; and an un-installed row is refused at `validate_workflow`
      with the missing filename (the T-309d `unknown_enum_value` behaviour), so adopt never emits a
      broken profile. Reuses `import_into` + `save_imported_profile` verbatim; nothing in `import.rs`
      changes. Three files; **no click-through** (no UI yet). A factored `adopt_from_fetched` helper
      carries the temp-cleanup logic so it is testable (`fetch_template` writes via comfy-cli and
      can't be mocked into producing a file). Landed matching the brief; the two mock tests (import +
      cleanup, refusal + cleanup) and an `#[ignore]` live test. src-tauri 119→121; gate green.
    - **T-505d-b — generalize profile emission for images (create-core). ✅ LANDED 2026-09-03**
      ([t-505d-b-brief.md](t-505d-b-brief.md)). **Discovered while scoping the UI:** `emit::build_profile`
      was audio-only — it refused any graph with no *audio* save node and hardcoded `kind: Music`, so
      adopting Klein (a `SaveImage` graph) failed at Save. Owner decision 2026-09-03: **generalize emit**
      (keep the one bring-your-own mechanism) rather than hand-author a curated image profile.
      `detect_output_kind` scans top-level nodes and subgraph interiors for either the audio or the
      image save-node set (audio wins if a graph somehow has both) and `build_profile` now emits
      `kind: image` with `output.save_node: "SaveImage"`, `prefer_lossless: false` for an image graph,
      unchanged `Music`/`SaveAudioAdvanced`/lossless for audio, `NoSaveNode` (reworded to name both
      kinds) when neither is present. Single-file `create-core` change, matched the brief exactly;
      `graph.rs` untouched (its audio lossless-swap never sees an image profile). No click-through
      (create-core, no UI) — proven by tests: the new `test_an_image_graph_emits_an_image_profile`,
      the renamed no-save-node-of-either-kind test, and a `kind: Music` guard added to the existing
      no-both-sources test. create-core 10 emit tests (was 8); gate green.
    - **T-505d-c — conditioning polarity in role suggestion (create-core). ✅ LANDED 2026-09-03**
      ([t-505d-c-brief.md](t-505d-c-brief.md)). **Found by verifying the subgraph risk, which turned
      out fine and revealed a worse one.** Subgraph slots do reach `suggest_roles` (addressed
      `A/B.name`), so Klein's controls are all found — but its **two `CLIPTextEncode` nodes both
      expose an input named `text`**: `75/74.text` drives `CFGGuider.positive`, `75/67.text` drives
      `.negative`. Name+type matching cannot tell them apart, so both rank `Strong` for `Tags` and
      `initialSelection` **pre-ticks both** — the adopted profile would silently write the prompt into
      the negative conditioning too, and `Negative` (name table `negative`/`negative_prompt`) would
      match nothing at all. This lane adds `audit::output_targets` (the mirror of `link_origin`:
      forward one hop to the input names a node drives) and lets that outrank the name table for the
      two prompt roles. **Inert for audio** and regression-tested so: both shipped models drive their
      negative from `ConditioningZeroOut`, which exposes no `STRING` slot. Fixtures
      `flux2_klein_9b.json` + its slot capture were committed ahead of the lane. No click-through.
      Landed matching the brief: `output_targets` (one hop, exact names, empty on anything
      unresolvable), a `resolve_subgraph` helper now shared with `resolve_in_subgraph`, and polarity
      computed once per slot outranking the name table for `Tags`/`Negative` only. Klein now maps
      tags→`75/74.text`, negative→`75/67.text` (`Strong`, reason naming the negative conditioning),
      seed/steps/cfg to its subgraph controls; ACE-Step and MiniMax suggestions unchanged with
      `Negative` still absent. create-core 176→184.
    - **T-505d-d — the "Bring in" button + adopt mapping UI. ✅ LANDED 2026-09-03, click-through PASSED**
      ([t-505d-d-brief.md](t-505d-d-brief.md)) (frontend, **has the click-through**). A ready **bare**
      row gets a "Bring in" action → an `adopt(name, title)` import-store action driving
      `catalog_adopt_begin` → the existing role-mapping surface (reusing `roleRows`/`canSave`/
      `saveNotes`/`mappingsOf`) → `save_imported_profile`. Backend prerequisites all landed: a
      (fetch→import seam), b (emit kinds an image graph), c (the prompt maps to the right encoder).
      Two decisions: **bare rows adopt, curated rows install** (adopting a curated row would emit a
      second, worse profile for a model the app already describes); and the store records
      **`adopting`**, the row that started the flow, because `ImportWorkflow` (Audio Studio) and
      `ModelCatalog` (Setup) render one singleton store from different views — without it a file
      import would draw a mapping screen under an unrelated catalog row. The in-progress screen is
      lifted into a shared `RoleMapping` component both surfaces render. **The name must be seeded
      from the gallery title, not `report.workflow_id`**: adopt fetches to `latentcreate-adopt-<row>.json`
      and `emit_profile` derives the profile id from the *display name*, so seeding the id would name
      the model after a temp file. An adopted profile is `workflow`-backed (`template: None`), so it
      does **not** join the T-505c curated index — the row stays bare after adopt, which is expected;
      marking it "adopted" across a reload needs a backend field and is out of scope.
      Landed matching the brief. Fixed in review: a `tsc` unused-import failure; a test guard that was
      vacuous because vitest is not configured to clear mocks between tests (call counts accumulated
      down the file, so `not.toHaveBeenCalled` was reading earlier tests); the lifted `failed` branch
      had picked up the saved branch's button label ("Import another" after a failure); an ASCII-ed
      em dash in an existing **UI string**, where CONVENTIONS allows Unicode; and **a dead end the
      brief missed** — switching the kind toggle or searching unmounts the row that owns an open
      bring-in, taking its mapping screen with it, and since the store allows one flow at a time the
      user could then reach neither Cancel nor any new import. The step now keeps the screen, named,
      when its row is no longer listed. frontend 453→459.
      **Click-through passed** (2026-09-03), verified independently against the emitted profile on
      disk: `kind: image`, `output.save_node: SaveImage`, `prefer_lossless: false`,
      tags→`75/74.text`, negative→`75/67.text`, seed/steps/cfg on the subgraph addresses, and
      `id: flux-2-klein-9b-text-to-image` from the gallery title rather than the adopt temp file —
      so b, c and d are all confirmed end to end. One visual defect found and fixed: the row's action
      button used the wizard's `.setup-actions`/`.setup-button`, which has no top margin (every other
      `.catalog-row` child sets its own) and step-sized padding, so it collided with the tag chips.
      Added `.catalog-row-actions` with a compact button override. **This hit `CuratedRow`'s Install
      button identically and had never been seen** — T-505c's click-through found nothing missing, so
      that button never rendered. Both rows now use the new class.
  **Curated set:** app-curated audio+image list with hand-verified URLs (the shipped-profile
  pattern). **Gallery rows:** installed → adopt (d); not-installed → show missing files verbatim (no
  auto-download, no URL-from-prose). An image curated entry also gives T-506 its profile.

### Cover art — generation over an image profile
- **T-506 — CoverArt generation. ✅ COMPLETE 2026-09-03** (all five lanes landed; both
  click-throughs passed). **SPLIT INTO FIVE LANES 2026-09-03**, scoped against a **live
  end-to-end run of the adopted Klein profile outside the app** — the same sequence
  `build_and_submit` performs, then `GET /history` to read what actually executed
  ([docs/MCP-SURFACE.md §35](../docs/MCP-SURFACE.md)). Three findings decide where the split falls:
  - **The Rust generation half already works, unchanged.** `ensure_lossless_output` is a clean
    no-op at `prefer_lossless: false` (what T-505d-b emits for an image graph), the LoRA splice is
    skipped for a profile with no `loras` block, `validate_workflow` passed, and the prompt,
    negative, seed and steps writes **all reached the engine** (§35.1) — so `build_and_submit` is
    reusable verbatim and the only Rust that must change is what happens to the outputs. 22 s for a
    768×768 cover with warm loaders.
  - **What is missing is everything *after* the output.** `ingest.rs` filters on `AUDIO_EXTS` and
    returns `None` for a PNG **without error** (§35.4): an image job run through the app today
    completes, reports Done, and saves nothing, anywhere. The library has no artwork record and
    `Project` has no art list. So the first lanes are storage and ingest, not generation.
  - **Image size stays the graph's own for v1** — 1024×1024 on Klein, which is the right shape for
    cover art anyway. Every size-shaped slot Klein exposes is **inert**: `75/62.width|height` and
    `75/66.width|height` are link-fed from `PrimitiveInt`s, and the effective addresses are
    `75/68.value` / `75/69.value` — proved by writing 1536 to the obvious ones and 768 to the
    primitives and measuring the PNG at 768×768 (§35.2). A size control is therefore a
    **role-suggestion** problem of exactly the T-505d-c kind (read the graph, do not match the
    name), not a UI one. Deferred with its evidence rather than guessed at; the owner's call
    whether it joins this phase.
  - **T-506a — the artwork record and its storage. ✅ LANDED 2026-09-03** ([t-506a-brief.md](t-506a-brief.md))
    (`create-core` + `library`, no MCP, no UI).
    `ArtId`, `Artwork` (reusing `Provenance` verbatim — it is already asset-agnostic), a
    `library::art` module mirroring `tracks.rs` (dir/paths/mint/save/load/list/delete-to-trash),
    and `Project.art` + `next_art_seq`, both `serde(default)` so every existing `project.json`
    still loads. Fully unit-tested; no click-through possible. Landed matching the brief, with two
    executor additions that were right: `#[serde(transparent)]` on `ArtId` (mirroring `TrackId`,
    which the brief's reference code had dropped) and `pub mod art;` **after** `albums`, which is
    where alphabetical order actually puts it — the brief said "before". Three defects fixed in
    review: an invented `pub use albums::AlbumSet;` for a type that does not exist and two
    test-only imports left at module scope (**both compile errors — the gate had not been run**),
    and a vacuous test, a fully-populated `Artwork` serialised and read straight back, which cannot
    fail unless a derive is removed; it now strips `title`/`width`/`height` from the JSON so the
    `serde(default)`s are what is under test. Eight mutations, eight killed. create-core 184→191,
    library 109→124; gate green.
  - **T-506b — `generate_image` + art ingest. ✅ LANDED 2026-09-03** ([t-506b-brief.md](t-506b-brief.md))
    (`src-tauri`). `PendingTrack` becomes `PendingOutput` carrying a `ModelKind` -- **the record
    decides, not the file extension** -- `ingest_outputs` returns a `Saved` per asset so the pump
    emits `track://saved` or `art://saved`, and the download directory is chosen by kind. A pure
    `kind_error` guard refuses a profile queued by the wrong command, before `ensure_connected`, in
    a sentence naming where it does belong. `build_and_submit` is untouched. Checked while briefing
    rather than assumed: `audit_slots` over the frozen Klein graph and the emitted profile's five
    addresses reports **nothing unchecked and nothing inert**, so the inert-slot refusal does not
    fire for an image profile. Fixture frozen ahead of the lane:
    `testdata/profiles/flux2-klein-9b-image.json`, the profile the app itself emitted, with only
    its adopt-time absolute workflow path rewritten to the repo's capture
    (`testdata/profiles/README.md`). **No click-through** -- tests only; the first sight of a cover
    is T-506d. Landed matching the brief; ARCHITECTURE §7 gained the shared-pipeline step in the
    same commit. Four defects fixed in review: **the brief's own `queue_generation` signature**
    (`&ComfyState`/`&ConfigDir` where `ensure_connected` wants the `State` wrappers -- a compile
    error the executor transcribed faithfully), two kind-guard tests whose **names were swapped
    against what they assert** and one expecting the fixture's *filename* where the profile's id
    belongs, a pipeline test using the input name `prompt` where the profile declares `tags`, and a
    title test that ingested the same file twice -- ingest **moves** the output, so the second run
    read a path that was no longer there. Also consolidated: `audio_extension`/`image_extension`
    differed only in the constant they consulted. **Seven mutations, seven killed** -- one only
    after rewriting the art counter test, which minted and wrote by hand and so survived moving the
    counter save *after* the file write; it is now the art mirror of the track burn test, driven
    through `ingest_outputs`. The repo's own comment on that track test already said a hand-ordered
    test cannot see a reordering. **Two production lines are not unit-testable** and waited for
    T-506d's click-through: the download-directory choice and the two-event dispatch both live in
    `ingest_if_pending`, which needs an `AppHandle` no test in the crate builds. **Both discharged
    2026-09-03** by T-506d's click-through steps 5 and 6. src-tauri 121→133.
  - **T-506c — the art generation store. SPLIT INTO TWO 2026-09-03**, because the stores need a
    config field and two read commands that do not exist, and mixing Rust into the store lane is
    what the T-504 -> T-505a cadence exists to avoid.
    - **T-506c-a — the backend seam. ✅ LANDED 2026-09-03 architect-direct**
      ([t-506c-a-brief.md](t-506c-a-brief.md)). `Config.default_image_profile_id` (its own field:
      one field would make picking an image model in Cover Art change the Audio Studio's model, and
      **there is no shipped image profile to fall back to**, so `None` stays `None` and the view
      says so), plus `src-tauri/src/art.rs` with `library_art` and `art_image_path` mirroring their
      track twins. Checked rather than assumed: the asset-protocol scope is already
      `$APPCONFIG/projects/**`, which covers `projects/<slug>/art/`. **The brief's file list was
      short by five** -- adding a required field to a mirrored type touches every test that builds
      a `Config` literal, which the wire-fixture tripwire found immediately and correctly. library
      124->126. `art.rs` has no tests, the same rule `tracks.rs` follows: both commands take Tauri
      `State`, which no test in the crate builds.
    - **T-506c-b — the panel factory and the art submit store. ✅ LANDED 2026-09-03**
      ([t-506c-b-brief.md](t-506c-b-brief.md)) (no UI). `paramPanel.ts` becomes
      `createParamPanelStore()` with two instances -- it is a module-level singleton today, so a
      CoverArt view loading an image profile into it would reset the Audio Studio's values on every
      view switch and re-roll a seed the user had already seen (the T-505d-d singleton lesson, one
      store over); the store body is unchanged and every existing importer keeps working. Plus a
      `generateImage` bridge, `state/artGenerate.ts` reusing `specsFor` with an **empty LoRA stack
      and no lyric document** (an adopted image profile declares neither, so nothing is invented),
      and `effectiveImageProfileId` / `selectedImageProfile` returning **`string | null`** with no
      fallback -- there is no shipped image profile to fall back to. Landed matching the brief. Two
      defects fixed in review: the executor **overwrote `pickable`** -- an existing export, not in
      the brief, and the very function whose doc comment says CoverArt will want it -- replacing it
      in place with `effectiveImageProfileId` and breaking `AudioStudio.tsx`; and a seed test stubbed
      `global.crypto`, which is getter-only here, when it needed no stub at all (every assertion is
      about whether the seed *changed*, which the real generator answers). **Nine mutations, nine
      killed**, including one re-run honestly: the first parallel-submit mutation did not actually
      parallelise, so the sequential-await invariant was re-checked with a real `Promise.all` over
      `generateImage`. frontend 459->476.
    - **T-506c-c — the artwork listing store. ✅ LANDED 2026-09-03**
      ([t-506c-c-brief.md](t-506c-c-brief.md)) (no UI). `bridge/art.ts` (reusing `Provenance` from
      `bridge/library.ts`, because the Rust `Artwork` embeds it verbatim) and `state/art.ts`: rows
      over `library_art`, asset URLs over `art_image_path`, and the `art://saved` subscription that
      makes a finished cover appear without a reload, mirroring what `track://saved` does for the
      Library. Three decisions the brief carries a reason for: the URLs resolve with `Promise.all`
      and a **per-artwork catch** (the sequential-await rule is about the one stdio transport to
      comfy-mcp and about `submittedAt` ordering the queue -- neither applies to a path lookup, and
      one unreadable sidecar must not blank the gallery, the same rule `list_art` follows on the
      Rust side); the URL map is **not cached across loads**, because art ids are per-project and a
      map keyed by id would show the previous project's cover under the new project's artwork; and
      `select`/`deleteProject` in `state/projects.ts` reload the artwork beside the tracks for that
      same reason. The lane also **extracts `modelLabel`/`seedText`/`createdDate`/`provenanceView`
      into `state/provenance.ts`** -- `library.ts`'s own comment says `queue.ts`'s `modelName` is a
      deliberate twin, and a third copy is where the fallback chain drifts, in the record a user
      reads to prove where a file came from. `provenanceView` takes a `Provenance` after the move,
      which is also how T-506d gets an artwork inspector for free. Landed matching the brief, and
      the extraction was faithful -- every moved test kept its assertions. **The gate was not run**:
      two tests failed and four `tsc` errors stood. One of the failures is worth keeping, because it
      is a test that would have passed for the wrong reason if the default had been `null`:
      `makeArtwork` built its overrides with `??`, so `width: null` was swallowed and re-defaulted
      to 768 and the missing-size case was never constructed. The other was a `startListening` test
      asserting one subscription under a fixed `isTauri: () => false`, which the guard refuses --
      fixed by making the flag togglable, the idiom `projects.test.ts` already uses. **Fourteen
      mutations, fourteen killed**, one after closing a gap it found: `modelLabel` dropping its
      `.trim()` survived, a hole carried in unchanged from `trackModel` and now shared by two
      stores, so a whitespace-only display name (an emitted profile takes its name from a workflow
      filename) is pinned. frontend 476->497.
  - **T-506d — the CoverArt view. ✅ LANDED 2026-09-03; CLICK-THROUGH PASSED (all 11 steps)**
    ([t-506d-brief.md](t-506d-brief.md))
    (frontend; **this lane has the click-through**). Image profile picker, param panel, Generate,
    the shared job queue, and a grid of what has been made. Two extractions keep it from becoming a
    second Audio Studio: `ParamPanel` takes its store as a prop (`ParamPanelStore =
    ReturnType<typeof createParamPanelStore>`), which is the last thing pinning the T-506c-b factory
    to one instance; and `ProfilePickerRow` moves to its own file, because it is the component that
    renders the **licence** and a second copy is where CONVENTIONS' commercial-use rule stops being
    true in one of the two studios. `GenerateArtBar` is deliberately **not** a prop on `GenerateBar`
    -- that bar reads a lyric document Cover Art has none of, and every rule the bar enforces
    (`blockers`, `canBatch`, `effectiveCount`, `queueingLabel`, `notesFor`) is already a shared pure
    function with a test, so the duplication is confined to the layer this repo cannot test anyway.
    Cover Art has more empty states than the Audio Studio because **the app ships no image
    profile** -- a first visit can have nothing chosen *and* nothing to choose -- so
    `imageStudioState` names five (`loading`/`no-profiles`/`none-chosen`/`missing`/`ready`) and
    `imageStudioNote` holds their sentences, as values rather than JSX. **The click-through is
    eleven steps and reads the files, not only the screen**: steps 5 and 6 are the whole point,
    proving the two lines in `ingest_if_pending` no `src-tauri` test can reach -- the `Saved::Art`
    arm emitting `art://saved` (a tile appears with no reload) and `ModelKind::Image => art_dir`
    (the PNG lands in `art/`, not `tracks/`). Landed matching the brief; both extractions are
    verbatim. **The gate was not run** -- one unused-variable error stood. Two review fixes, both
    about a stale value outliving what produced it: `selectedImageProfile` resolved a configured id
    against **every** profile, so a *music* id in `default_image_profile_id` -- two independent
    fields, and `config.json` is editable -- read as `ready` and would have put a music param panel
    in Cover Art with only `generate_image`'s kind guard behind it at submit; it now resolves
    against the image list, and a test pins it. And `ArtTile`'s `broken` flag never cleared, so a
    tile that failed to load once stayed "not found" for the life of the mount, including after the
    file came back -- reset on `row` identity, which `artRows` changes on exactly a reload and never
    on an unrelated re-render. **Nine mutations, nine killed.** frontend 497->508. **Click-through: all
    eleven steps passed 2026-09-03** -- the tile appears with no reload (the `Saved::Art` arm emits
    `art://saved`), the PNG lands in `art/` rather than `tracks/` (`ModelKind::Image => art_dir`),
    the sidecar's resolved slots and its 768x768 match the executed graph, the two panels stay
    independent across a view switch, a batch of two gives two seeds and two tiles, a project switch
    changes the gallery, a renamed file reads as missing and keeps its facts, and the Library is
    undisturbed by `provenanceView`'s new signature. **Cover art now works end to end**, and the
    two `ingest_if_pending` lines named as untested at T-506b are discharged.
  - **T-506e — attach artwork to a track or an album, and delete one. SPLIT INTO TWO 2026-09-03**,
    for the reason T-506c was: the stores need a schema change and three commands that do not exist,
    and mixing Rust into a store lane is what the T-504 -> T-505a cadence exists to avoid. A track's
    cover belongs in the **track sidecar** (`Track.cover: Option<ArtId>`) and an album's in its
    `AlbumList`, per ARCHITECTURE §8's one-source-of-truth rule; the artwork sidecar stays a pure
    provenance record. **`delete_art` lands here, not in a**, because deleting an artwork has to
    decide what a cover reference means. Artwork is the first new *kind of created content* since
    T-408, so the delete rule follows it.
    - **T-506e-a — the cover backend. ✅ LANDED 2026-09-03** ([t-506e-a-brief.md](t-506e-a-brief.md))
      (no frontend). The two fields, `set_track_cover` / `set_album_cover` / `delete_art`, and the
      three commands. **Correcting this entry's earlier guess:** a cover reference does **not** block
      a delete, and there is no `tracks_referencing`-style check. The repo has two precedents and
      they differ -- `lyrics::delete_doc` *refuses* because a track's `LyricRef` is part of the
      recipe and deleting the document would strand it, while `delete_track` *clears* the id from
      every album because an album is the user's current arrangement, not a record of how anything
      was made. A cover is the second kind: an editable pointer beside `title`, on which nothing
      reproducible depends. Refusing would make a user detach a bad cover from every track before
      deleting it -- friction bought with no protection. The T-408 shape still holds for the file
      half: to OS trash, trasher injected so `cargo test` never fills a Recycle Bin, files first and
      record last. One thing the brief says plainly rather than hides: clearing covers is **N atomic
      writes, not one transaction**, so a crash part-way leaves some tracks with no cover and some
      naming a deleted one -- which is why e-b must render a dangling cover as missing, the way
      T-403 renders a missing track. Landed matching the brief. **The gate was not run**: the crate
      did not compile (`&proj.slug` and `&mut proj` in one call), and behind that four tests failed
      on the same fixture bug -- `add_artwork_to_project` minted an id and wrote a sidecar but never
      pushed the id onto `Project::art`, which is the only thing the cover setters check, so every
      test that set a cover hit `NotFound` and the album test asserted the wrong error kind. Replaced
      with a four-line `register_art_id`: the setters never load the artwork, so a fixture that built
      one was testing something the code does not read. Also folded a third hand-built `Artwork`
      literal in `art.rs` back onto its own `sample_artwork`. **Fifteen mutations, fifteen killed,**
      two of them only after the count was made honest: `#[serde(default)]` on `Option<ArtId>` is an
      **equivalent mutant** (serde already defaults a missing `Option`) -- the same fact T-506c-a
      logged, so the attribute stays for consistency with every other optional field and the survivor
      is noted rather than churned; and the files-first ordering survived until a test was added for
      the half of it that is reachable -- a trasher that fails on the sidecar, asserting the project
      still lists the artwork so the delete can be retried. create-core 191->193, library 126->139,
      src-tauri 133->134.
    - **T-506e-b — the cover stores. ✅ LANDED 2026-09-03** ([t-506e-b-brief.md](t-506e-b-brief.md))
      (no views). The frontend half is **split in two**, the c/d rhythm this phase has used twice
      already: e-b is every rule as a pure function or a tested store action, e-c is the layer this
      repo cannot test in `node`. Three bridge calls, `Track.cover` / `AlbumList.cover` on the wire
      types, and `state/covers.ts` -- its own module because these selectors describe a cover **on
      something else**, and both the Library and the album panel need them without pulling in the
      gallery store. `coverView` returns `none` / `missing` / `shown`: **`missing` is not `none`**,
      because `none` would claim the row never had a cover, and a dangling reference is genuinely
      reachable -- e-a clears covers in N atomic writes, not one transaction. `deleteArtPrompt`
      states the rule unconditionally and appends the usage counts only when they are known, so a
      view that has not loaded the tracks says less rather than something false. `art.remove`
      reloads **three** stores, because `delete_art` rewrites track sidecars and album lists the
      frontend is already holding. Landed matching the brief. **The gate was not run** -- two
      `tsc` errors (a missing `afterEach` import, an unused `beforeEach`). Two review fixes, both
      about a test that passes for the wrong reason. The new delete-flow suite reset only its own
      mock, so `listArt` still carried whatever the *previous* describe's last test had armed on it
      -- its `error === null` assertion was decided by test order; it now resets and arms every mock
      it touches, and asserts the gallery reload it was named for. And the `albumRows` fixture gave
      every album `cover: null`, so `cover: album.cover` and `cover: null` were indistinguishable --
      the survivor a mutation found, and the same shape as T-506c-c's `??`-swallowed default. Also
      restored the house confirm wording (curly quotes around the name, as `ProjectDelete` has).
      **Seventeen mutations, seventeen killed** after that fix. frontend 508->525.
    - **T-506e-c — the cover views. ✅ LANDED 2026-09-03; CLICK-THROUGH PASSED (all 12 steps)**
      ([t-506e-c-brief.md](t-506e-c-brief.md)).
      `CoverPicker` (presentational, reads **no store** -- its two callers write through different
      ones, and a store read inside would make the component pick a side), the control on a track
      card and an album row, Delete with its confirm on a gallery tile, and the CSS. **No new
      logic**: every decision is already a tested function in `state/covers.ts`, and the brief makes
      "if something seems to need a new one, stop and ask" an acceptance criterion. Cover Art loads
      the library and albums stores on mount, which is not incidental -- `deleteArtPrompt` can only
      name what a delete will detach if they are loaded, and the alternative is a confirm that
      understates what it is about to do. **Twelve-step click-through**, reading the files rather
      than the screen: the track sidecar's `provenance` unchanged by attaching a cover (step 2, the
      rule the field is placed to keep), three views settling with no manual reload after a delete
      (step 6, what `remove`'s three store reloads buy), the trashed files still in the Recycle Bin
      (7), and a hand-made dangling cover rendering as missing and repairable (9 -- the state e-a's
      non-atomic clearing can leave, named there so it would be designed for rather than
      discovered). Landed matching the brief -- both acceptance criteria hold: the only new export
      is the component itself, and `CoverPicker` reads no store. **The gate was not run**: `art` was
      subscribed in `Library()` but used in `TrackCard`, a separate component, so it did not
      compile. The other three fixes were **deleted comments** -- the run stripped the T-409
      sanitise-before-the-dialog note, the whole `TrackDetails` doc comment, and the `broken`-reset
      comment written during T-506d's own review, none of them near anything it was asked to
      change. All restored. No mutations: this lane adds no decidable logic, which is the claim the
      brief makes and the reason the twelve-step click-through is its acceptance. frontend 525
      (unchanged -- no new tests, by design). **Click-through: all twelve steps passed
      2026-09-03** -- a cover attaches and clears on a track and an album with the track sidecar's
      `provenance` untouched, deleting an artwork in use names both counts in its confirm and then
      settles three views with no manual reload, the trashed files are in the Recycle Bin rather
      than erased, a hand-edited dangling cover renders as missing and is repairable from the
      picker, ids are still not reused after a delete, and covers follow their project across a
      switch. **T-506 is complete.**

### Packaging & public-repo readiness (original Phase 5 scope)
- **T-507 — First-run polish + empty/degraded-states audit.** Sweep every view for the cold-start
  and offline states; consistent status pills, no modal walls (ARCHITECTURE section 10 rule).
  **Carries one named item from T-505d-d's click-through:** an imported/adopted profile declares no
  model files (`emit` sets `models: []`, unchanged since T-313), so the Models step reads
  `Undeclared` — "Cannot check" + "This profile does not list the model files it needs." Correct for
  a user's own graph, but poor for an adopted gallery row the app *knows* is runnable. Nothing gates
  on readiness (verified: `generate.ts`, `generatePanel.ts`, `GenerateBar.tsx`, `generate.rs`,
  `profiles.ts` all ignore it), so the profile is fully usable and this is presentation, not a
  blocker. The fix available: populate `comfy.models` in `emit` from the graph's loader COMBO slots —
  Klein names all three (`75/70.unet_name`, `75/71.clip_name`, `75/72.vae_name`) — resolving each
  file's folder via `search_models`. `source_url`/`size_bytes` are both `Option` and stay `None`, so
  the row would read **Ready** without ever claiming the app can fetch someone else's weights.
- **T-508 — Installer builds.** Windows first (NSIS/MSIX), then macOS `.dmg` + Linux AppImage via
  CI. The milestone gate for the phase.
- **T-509 — THIRD-PARTY-LICENSES generation** (Rust + npm dependency licenses; the ported-viz
  bookkeeping ARCHITECTURE §9 notes).
- **T-510 — Public-repo readiness** — CONTRIBUTING, issue/PR templates, a README pass for a
  stranger cloning cold.

### Catalog pivot (owner decision 2026-09-05 — see PROJECT.md decisions log)

The catalog is refocused on **what the app can actually install**. comfy-mcp cannot auto-install an
arbitrary gallery model (no URL from a template; `download_model` is URL-only; Manager's model DB is
not exposed — re-verified live, MCP-SURFACE §36). So the browse-the-whole-gallery design is dropped
and the catalog becomes the **curated installable set**, surfaced through the Setup Models step that
already renders shipped profiles with one-click Install.

- **T-511 — Curate the image model profiles.** Ship `profiles/*.json` (kind `image`), all
  **commercial-safe** (the 2026-09-05 licence decision — cover art is for music people may release):
  **Flux.1 Schnell fp8** (Apache-2.0), **Chroma** (Apache-2.0), **SDXL** (CreativeML OpenRAIL-M),
  **SD 3.5** (Stability Community Licence), **Qwen-Image** (Apache-2.0). Each in the ACE-Step/MiniMax
  shape: `comfy.models` with per-file `folder`/`source_url`/`size_bytes`, a `template` ref, the input
  controls, the image save node, and an honest `vram_gb_min` (Qwen-Image is 20B — a higher-VRAM
  option that will not fit the 16 GB dev card). Each URL + file list is **verified live**
  (`fetch_template` → `list_workflow_slots` for the exact filenames/folders/addresses; the download
  URL confirmed to resolve on Hugging Face) before it ships. This is architect-authored, not an Aider
  lane — it is verified content. Klein 9B and Flux.1 Dev were dropped as non-commercial.
- **T-512 — Strip the catalog to the installable list.** Remove `components/ModelCatalog.tsx`,
  `components/RoleMapping.tsx`, `state/catalog.ts` (+ test), `bridge/catalog.ts`, the T-504
  `search_templates` catalog backend, and the T-505d gallery-adopt path; drop `<ModelCatalog />`
  from Setup. **Keep** `ImportWorkflow` (T-313, the bring-your-own valve) and everything the Models
  step and generation depend on. The Models step becomes the whole catalog; give it an `Audio |
  Image` split or headings so the two kinds read clearly.

**Build order, as it actually ran:** the three polish items first (T-501/502/503, independent and
cheap), then the catalog (T-504/505, which gated cover art), then cover art (T-506), then T-507
(first-run + empty-state polish). **All landed by 2026-09-05.** The 2026-09-05 catalog pivot adds
**T-511** (curate image profiles) and **T-512** (strip the gallery); do T-511 first so there is
something installable before the gallery is removed. What remains after: **T-508** installers (the
closing milestone), **T-509** THIRD-PARTY-LICENSES (now covering the shipped image-model licences
too), **T-510** public-repo readiness.

---

## Milestone check (live)
A person on a machine that never had the dev toolchain installs the build, opens Setup, **installs a
curated image model** from the Models step (one click, a real download), generates **cover art** over
it with a provenance sidecar, sees the **player docked on screen** with a **sharp** visualizer, and
the lyrics document picker reads as a card. Installable build produced by CI for at least Windows.

**The image-install half is what the 2026-09-05 pivot exists to deliver.** Before it, images had
**no** installable path at all (zero shipped image profiles), so this line could not be met for
images even in a dev build. After **T-511** (curated image profiles) a one-click install works
through the existing Models-step machinery; **T-512** removes the gallery that could not honour it.
Cover art with its sidecar, the docked player and sharp visualizer, and the lyrics card are already
discharged (T-506d/T-506e-c, T-501/502/503). **What the milestone still needs is T-511 + the
installer (T-508)** -- then the whole sequence run once on a machine that never had the toolchain,
the only part a dev-machine click-through cannot stand in for.
