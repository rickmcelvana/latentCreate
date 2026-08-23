# Phase 0 — Scaffold

Goal: a repo that compiles, lints, tests, and launches an empty themed Tauri shell. No ComfyUI/LLM code yet. Producer prereq: Rust stable, Node 20+, `npm i -g @tauri-apps/cli` not required (use npx).

> ⚠ T-001 is producer+architect work (project generators, version pinning) — **not an Aider task**. Aider starts at T-002.

---

# T-001: Repo scaffold (architect/producer, no Aider) — ✅ **DONE 2026-08-23**
**Files to create:** entire skeleton.

Steps (producer runs, architect drives):
1. `npm create tauri-app@latest` equivalent layout matching ARCHITECTURE §2: `app/` (Vite + React 19 + TS strict), `src-tauri/`, workspace `Cargo.toml` with member crates `crates/create-core`, `crates/mcp-bridge`, `crates/llm-bridge`, `crates/library` (each `lib.rs` with a doc comment + empty `#[cfg(test)]` module).
2. Pin versions in line with `../latent-mixing` (Tauri ~2.11, React ~19.2, Vite ~8, TS ~6, zustand, oxlint, vitest). tsconfig: strict + `noUnusedLocals` + `noUnusedParameters` + `verbatimModuleSyntax`.
3. `theme.css` seeded with the Latent palette variables (from `latentbeats.com`: `#0a0e1a` ground, `#58a6ff` accent, 12px radii) and the nav-rail layout classes.
4. LICENSE (Apache-2.0), NOTICE, and `.gitignore` already exist from the planning commit — extend `.gitignore` only if the generators add new build dirs. Add the Apache header boilerplate to `src-tauri/tauri.conf.json` metadata (license field) and each `Cargo.toml` (`license = "Apache-2.0"`), plus `"license": "Apache-2.0"` in `app/package.json`.
5. Commit `T-001: repo scaffold`.

**Acceptance:** `cargo check --workspace`, `npx tsc -b`, `npm run build`, `npm run tauri dev` opens a window.

## Outcome (2026-08-23)
Green: `cargo check --workspace`, `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test --workspace` (5 targets), `npx tsc -b`, `npm test` (vitest), `npm run build`, `oxlint` (0 findings, 5 files), and `tauri build --no-bundle` -> `target/release/app.exe`. The binary was launched and served its frontend (Tauri manager logged asset requests) before being terminated.

**Unverified — for the producer:** nobody has *seen* the window. Process start plus asset serving is strong evidence it opened, but visual confirmation of `npm run dev` is a manual check (WORKFLOW §5).

**Deviations from the brief, deliberate:**
1. **Root `package.json` added.** The Tauri CLI resolves its project by scanning *subfolders* of the cwd, so running it from `app/` cannot find `src-tauri/`. The CLI and the `dev`/`build` entry points now live at the repo root; `app/` keeps its own package.json for the frontend. Run the desktop app with `npm run dev` from the root.
2. **Crate stub tests assert the crate name** rather than `assert!(true)` — clippy's `assertions_on_constants` rejects the latter under `-D warnings`.
3. **Palette is blue, tracking `latentbeats.com`.** The plan said "violet", which *was* correct — the umbrella site was violet until the owner rebranded it to blue in Aug 2026 (`../website/latentbeats.com/css/style.css`). latentCreate now mirrors the site's tokens (`#0a0e1a` ground, `#58a6ff` accent, 12px radii, card shadow), which are a newer and bluer take than the two sibling apps' GitHub-dark values. Divergence from the siblings is intentional and noted for the owner.
4. **A placeholder app icon was generated** (dark rounded field, three accent bars) so bundling works. Branding pass is deferred with OQ-5; android/ios icon output was deleted (desktop only).
5. **`app/src/bridge/shell.ts` + a smoke test exist**, slightly ahead of the brief, so the scaffold proves the Tauri boundary round-trips instead of only compiling.

---

# T-002: Theme shell + nav rail
**Depends:** T-001 (done) | **Dir:** `app/`
**Files:** `app/src/App.tsx`, `app/src/theme.css`, `app/src/views/{Setup,LyricsStudio,AudioStudio,Library,CoverArt}.tsx` (placeholder views), `app/src/state/nav.ts`

> Starting point: T-001 left `App.tsx` as a one-panel placeholder that calls `bridge/shell.ts` to show the Rust version. Keep that status pill somewhere unobtrusive (or move it into Setup) rather than deleting the only proof the bridge works. `theme.css` already defines the palette, `.app-shell`, `.nav-rail`, `.nav-brand`, `.content-pane`, `.view-title`, `.view-subtitle`, `.panel`, `.muted` and `.status-pill*` — extend it, do not restyle from scratch.

## Goal
Left nav rail switching five placeholder views, styled per the Latent visual language; no routing lib (zustand `nav.ts` holds the active view).

## Spec
Nav rail: icon+label buttons for Setup, Lyrics, Audio, Library, Cover Art; active state uses the accent colour (`--accent`); views render a titled empty-state panel ("Nothing here yet — finish Setup" style copy). Window min size 1100×700. Every className styled in `theme.css`.

## Acceptance criteria
- [ ] `npx tsc -b`, `npm run build`, `npm test` green; oxlint clean
- [ ] vitest: nav store switches views; all five render without crash
- [ ] No changes outside listed files

## Out of scope
Any Tauri invoke; any real view content.

## If unclear
Do not guess. Output numbered questions and stop.

## Aider launch
```bash
aider --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file app/src/App.tsx --file app/src/theme.css --file app/src/state/nav.ts --file app/src/views/Setup.tsx --file app/src/views/LyricsStudio.tsx --file app/src/views/AudioStudio.tsx --file app/src/views/Library.tsx --file app/src/views/CoverArt.tsx
```

---

# T-003: create-core domain types
**Depends:** T-001 | **Crate:** `crates/create-core`
**Files:** `crates/create-core/src/{lib.rs,project.rs,generation.rs,profile.rs,provenance.rs}`

## Goal
Serde types for the whole domain per ARCHITECTURE §5/§5a/§7/§8: `ModelProfile` (+ `InputSpec` enum: Text/Lyrics/Number/Seed with the fields shown in §5, plus the optional `loras` block and a `license` field), `LoraRef` (identity + strength + order) and `LoraStack`, `Project`, `LyricDoc`, `Track`, `GenerationSpec` (includes the LoRA stack), `Provenance` (records it). No I/O.

## Spec
All types `Serialize + Deserialize + Clone + Debug + PartialEq`. `serde(deny_unknown_fields)` OFF for `ModelProfile` (forward-compat), ON for internal types. Include a `profiles/ace-step-1.5.json` fixture (copy the §5 example, adjusted to compile) and a round-trip test.

## Acceptance criteria
- [ ] `cargo test -p create-core` incl. `test_profile_roundtrip_ace_step_fixture` and `test_profile_without_loras_block_deserializes` (LoRA support is optional per model)
- [ ] clippy/fmt clean; docs on all public items
- [ ] No changes outside listed files + `profiles/ace-step-1.5.json`

## Out of scope
Profile *loading* from disk (that's `library`, Phase 1); any validation logic beyond serde.

## If unclear
Numbered questions, stop.

## Aider launch
```bash
aider --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file crates/create-core/src/lib.rs --file crates/create-core/src/project.rs --file crates/create-core/src/generation.rs --file crates/create-core/src/profile.rs --file crates/create-core/src/provenance.rs --file profiles/ace-step-1.5.json
```

---

# T-004: Config store + keychain
**Depends:** T-003 | **Crate:** `crates/library` (+ `src-tauri` wiring)
**Files:** `crates/library/src/{lib.rs,config.rs}`, `src-tauri/src/lib.rs`, `app/src/bridge/config.ts`, `app/src/state/config.ts`

## Goal
Load/save `config.json` in the app data dir (non-secret settings: comfy mode local/cloud, endpoints, chosen provider, chosen profiles); secrets via `keyring` behind `set_secret(name)/get_secret(name)` — with Tauri commands + typed bridge wrappers + zustand config store.

## Spec
Atomic writes (temp file + rename). Missing/corrupt config → defaults + non-fatal warning event, never a crash. Bridge exposes `loadConfig()`, `saveConfig(patch)`, `setSecret(name, value)`, `hasSecret(name)` — secret *values* never cross to the frontend after being set. Architect pre-derives the keyring + atomic-write reference code in this brief before launch (WORKFLOW §1).

## Acceptance criteria
- [ ] `cargo test -p library`: roundtrip, corrupt-file recovery, atomic write (tempdir tests)
- [ ] vitest: config store hydrates from mocked bridge
- [ ] Secrets absent from config.json in tests; clippy/fmt/tsc clean

## Out of scope
Any UI; any real service validation.

## If unclear
Numbered questions, stop.

## Aider launch
```bash
aider --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file crates/library/src/lib.rs --file crates/library/src/config.rs --file src-tauri/src/lib.rs --file app/src/bridge/config.ts --file app/src/state/config.ts
```

---

# T-005: CI
**Depends:** T-001 | **Files:** `.github/workflows/ci.yml`

## Goal
GitHub Actions: on push/PR — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `npx tsc -b`, `npm test`, `npm run build`, on ubuntu + windows + macos (Tauri system deps installed on ubuntu).

## Acceptance criteria
- [ ] Workflow passes on the repo's actual state
- [ ] Caching for cargo + npm

## Aider launch
```bash
aider --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --file .github/workflows/ci.yml
```

---

# T-006: Milestone check (producer, no Aider)
Fresh clone → install deps → `npm run tauri dev` shows themed shell with nav; CI green on all three OSes; tag `phase0-done`. Paste results into PROJECT.md session log. Then architect opens `tasks/phase-1.md`.
