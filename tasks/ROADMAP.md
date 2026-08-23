# ROADMAP.md — phase plan

Each phase gets a `tasks/phase-N.md` with T-numbered Aider briefs (template: WORKFLOW.md §3) written by the architect *at the start of that phase* — briefs are not pre-written phases ahead, because each phase's interfaces harden in review. Phase 0 briefs exist now.

## Phase 0 — Scaffold (T-001 … T-006) — briefs ready: [phase-0.md](phase-0.md)
Repo skeleton that compiles, lints, tests, and launches an empty themed shell.
Cargo workspace (`create-core`, `mcp-bridge`, `llm-bridge`, `library` stubs), Tauri 2 shell, Vite/React/TS app with nav rail + theme.css, CI (fmt/clippy/test/tsc/build on the three desktop OSes), config store + keychain plumbing, docs/license boilerplate.
**Milestone check:** `npm run tauri dev` shows the themed empty shell; CI green.

## Phase 1 — Connections & setup wizard (T-101 …)
`mcp-bridge` against real `comfy-mcp` (schema enumeration first — RESEARCH §1 **and §3a: how to enumerate installed LoRAs** — both verified before any typed wrapper is written), `ComfyBackend` trait + stdio & cloud transports, mock-transport test rig, job event pump. `llm-bridge` with `openai_compat` + `ollama_native` (+ streaming). Model profile loader (`profiles/` + user dir merge) and the seed profiles (docs/MODELS.md). Setup wizard UI (ARCHITECTURE §10): detect/install guidance, health pills, curated model install with progress, per-model license display, LLM config + test call with the recommended-for-lyrics chips.
**Milestone check (live):** fresh machine → wizard → ACE-Step installed via app → server info visible.

## Phase 2 — Lyrics Studio (T-201 …)
Brief form with prefills, system-prompt assembly from profile (ARCHITECTURE §6), streaming generation UI, versioned editor with structure-tag validation, approve → handoff store action, consent-gated prompt optimizer with diff view (shared component — reused for audio tags in Phase 3).
**Milestone check (live):** brief → lyrics stream in → edit → approve; optimizer diff accept/revert round-trips.

## Phase 3 — Audio Studio & pipeline (T-301 …)
Profile-driven param panel, **LoRA stack panel** (picker + strength + reorder/bypass, ARCHITECTURE §5a), GenerationSpec build, run_template/submit_workflow paths, queue panel with progress/cancel, output ingestion → library + provenance sidecar, batch-by-seeds, **custom workflow import + input mapping** (§5b). Decide OQ-3 (raw-API fallback) here on evidence.
**Milestone check (live):** tags+lyrics → queued job → track appears in library with complete sidecar; a two-LoRA ACE-Step run reproduces from its sidecar alone; an imported user workflow generates successfully; kill ComfyUI mid-job → clean failed state + retry.

## Phase 4 — Library & Player (T-401 …)
Library views (project/track lists, album lists), player with AnalyserNode spectrum+waveform visualizer, track actions (trash-delete, rename, export/reveal), Send-to links (v1: open app.latentmixer.com / app.latentmastering.com + reveal file), provenance inspector panel ("re-use these settings" action).
**Phase-start check:** re-read the mixing/mastering repos — if their file-handoff protocol has landed by then, implement against it instead of the v1 link-out.
**Milestone check (live):** generate → play with visualizer → album list → send-to opens site with file revealed.

## Phase 5 — Cover art, polish, packaging (T-501 …)
Cover-art view over the image profile, first-run polish pass, empty/degraded states audit, installer builds (Windows first: NSIS/MSIX; then macOS dmg + Linux AppImage via CI), THIRD-PARTY-LICENSES generation, public-repo readiness (CONTRIBUTING, issue templates).
**Milestone check:** installable build on a machine that never had the dev toolchain.

## Future (backlog, PROJECT.md)
Bulk send-to-mastering (needs mastering bulk import) · latentPlayer hand-off · audio-to-audio (extend/cover/remix) profiles · community profile sharing · model-news surface.
