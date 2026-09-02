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
- **T-504 — Gallery browse + bare-row readiness (backend seam). 📝 BRIEFED** ([t-504-brief.md](t-504-brief.md)).
  `browse_templates` (a `search_templates` variant: `type` + `exclude_api:true` + `offset`) and two
  Tauri commands — `catalog_browse(kind, query, offset)` and `catalog_readiness(name)` returning the
  raw `LocalCheck` tri-state. The Ready/Not-ready/Unknown **verdict is derived in the T-505 store
  (TS)**, matching the repo; `create-core` stays pure (it has no mcp-bridge dep). Three files:
  `mcp-bridge/templates.rs`, new `src-tauri/catalog.rs`, `lib.rs`. **Install is not here** — see below.
- **T-505 — Catalog UI, curated one-click install, adopt-to-profile.** The browse list on Setup
  (Music-models step filtered to audio, Cover-art step to image), one shared component; the store
  that derives the readiness verdict from `LocalCheck`. **Curated set:** an app-curated audio+image
  list with hand-verified URLs → one-click install reusing `install.rs` (`models_install`/
  `models_progress`). **Gallery rows:** installed → **adopt** via the T-313 import-to-profile path;
  not-installed → show the missing files verbatim (no auto-download, no URL-from-prose). **Likely
  splits** (list+store / curated install / adopt). An image curated entry also gives T-506 its
  profile.

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
