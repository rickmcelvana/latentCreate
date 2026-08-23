# CONVENTIONS.md — latentCreate code standards

*Feed to Aider with `--read` on every run (alongside WORKFLOW.md and ARCHITECTURE.md). Adapted from the sibling repos' standards; this repo has no realtime/DSP paths, so those rules are replaced by network/process rules.*

## Rust
- Edition 2021+; `cargo fmt` mandatory; `cargo clippy --all-targets -- -D warnings` clean.
- Errors: `thiserror` enum per crate (`ComfyError`, `LlmError`, `LibraryError`); no `unwrap()`/`expect()` outside tests and `main.rs`.
- Async: tokio. Any spawned task that outlives a command must be cancellable and owned by a managed state struct — no detached fire-and-forget loops.
- Child processes (`comfy-mcp`): always killed on drop/app-exit; stderr captured to the session log.
- Serde types in `create-core` are the single source of truth for anything crossing the Tauri boundary; frontend types are generated or hand-mirrored *with a test that round-trips fixtures*.
- Secrets only via `keyring`; never in `config.json`, logs, sidecars, or error strings.
- Third-party API surfaces (rmcp, comfy-mcp tools, Tauri plugins, provider REST APIs) verified against actual docs before landing in a brief — never from memory. Pin versions.
- Public items documented with `///` including units (`duration_s`, `vram_gb`); every Rust module with logic gets `#[cfg(test)] mod tests`.

## TypeScript / React
- Strict TS, no `any` (use `unknown` + narrowing).
- Functional components only. State via Zustand stores in `app/src/state/` — no component-local copies of config/library/job state.
- Tauri access (`invoke`, `listen`) **only** inside `app/src/bridge/` typed wrappers. Components import bridge functions, never `@tauri-apps/*`.
- Long-running work: Rust pushes Tauri events; the frontend never polls in a loop.
- Styling: plain CSS in `theme.css`, dark professional theme, violet accent — match the Latent suite's visual language. No UI framework (no MUI/AntD/Tailwind). Every className used in TSX has a rule in `theme.css`.
- `import type` for type-only imports; unused params `_`-prefixed (tsconfig has `noUnusedLocals`/`noUnusedParameters`/`verbatimModuleSyntax`).

## Product rules (enforced in review, not just UX docs)
- User text (prompts, lyrics) is never modified without an explicit accept step (ARCHITECTURE §6).
- Every generated asset gets a provenance sidecar before it appears in the UI (ARCHITECTURE §8), complete enough to reproduce the result — **including the full LoRA stack** (file identity, strength, order).
- Per-model license terms are shown wherever a model is chosen or installed. Some models are open-weights-with-conditions (attribution, revenue thresholds), not OSI-open; users ship these tracks commercially and must be able to see the terms without leaving the app.
- Delete moves to OS trash; no hard deletes anywhere.
- Degraded services (ComfyUI down, LLM unreachable) degrade the relevant view with a status pill + retry — never a blocking modal, never a crash.
- All user-facing errors say what to do next ("Start ComfyUI, then Retry"), not just what failed.

## Testing & verification
- `cargo test --workspace`, `npx tsc -b`, `npm test` green before diff review; `npm run build` when `app/` touched.
- Network crates test against mock transports/fixtures (WORKFLOW §5); no test may require a live ComfyUI or LLM.
- Test names: `test_<behavior>_<condition>`.

## General
- No new dependencies unless the brief lists them, with license noted (permissive only — this repo is open source; copyleft deps need an explicit decisions-log entry).
- No TODO comments — unfinished work goes to PROJECT.md backlog.
- ASCII in code/comments (UI strings may use Unicode).
- If a brief conflicts with this file, stop and ask.
