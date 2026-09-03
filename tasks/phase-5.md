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
    - **T-505d-d — the "Bring in" button + adopt mapping UI. ✅ LANDED 2026-09-03, click-through pending**
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
  **Curated set:** app-curated audio+image list with hand-verified URLs (the shipped-profile
  pattern). **Gallery rows:** installed → adopt (d); not-installed → show missing files verbatim (no
  auto-download, no URL-from-prose). An image curated entry also gives T-506 its profile.

### Cover art — generation over an image profile
- **T-506 — CoverArt generation.** Generate single/album artwork over the adopted image profile,
  reusing the Phase 3 pipeline shape (spec → per-job workflow copy → validate → run → ingest with a
  provenance sidecar). Attach artwork to a track/album. **Depends on** an image profile existing,
  i.e. T-504/T-505. **Will split** — this is the largest single feature.

### Packaging & public-repo readiness (original Phase 5 scope)
- **T-507 — First-run polish + empty/degraded-states audit.** Sweep every view for the cold-start
  and offline states; consistent status pills, no modal walls (ARCHITECTURE §10 rule).
- **T-508 — Installer builds.** Windows first (NSIS/MSIX), then macOS `.dmg` + Linux AppImage via
  CI. The milestone gate for the phase.
- **T-509 — THIRD-PARTY-LICENSES generation** (Rust + npm dependency licenses; the ported-viz
  bookkeeping ARCHITECTURE §9 notes).
- **T-510 — Public-repo readiness** — CONTRIBUTING, issue/PR templates, a README pass for a
  stranger cloning cold.

**Build order is the owner's next call** (the "docs first, then decide build order" decision). The
three polish items are independent and cheap; the catalog (T-504/505) gates cover art (T-506);
packaging (T-508) is the closing milestone by nature.

---

## Milestone check (live)
A person on a machine that never had the dev toolchain installs the build, opens Setup, **searches
the catalog and installs an image model**, generates **cover art** over it with a provenance
sidecar, sees the **player docked on screen** with a **sharp** visualizer, and the lyrics document
picker reads as a card. Installable build produced by CI for at least Windows.
