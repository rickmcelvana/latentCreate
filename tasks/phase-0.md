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

# T-002: Nav rail + five placeholder views — ✅ **DONE 2026-08-23** (needed T-002b)
**Depends:** T-001 (done) | **Dir:** `app/` | **Executor:** Aider

The full brief lives in **[t-002-brief.md](t-002-brief.md)** -- it is long enough
(exact SVG primitives, verbatim UX copy, CSS requirements) that keeping it inline
would bury the rest of the phase. Paste that file into Aider after launching.

Summary: nav rail with five buttons driven by a Zustand `useNavStore`, five
placeholder views with titled empty states, `App.tsx` reduced to a composition
root with an exhaustive `switch` over `ViewId`, and the T-001 version pill moved
into the rail footer so the Tauri-bridge proof survives.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file app/src/App.tsx --file app/src/theme.css --file app/src/state/nav.ts --file app/src/state/nav.test.ts --file app/src/components/NavRail.tsx --file app/src/components/NavIcon.tsx --file app/src/views/Setup.tsx --file app/src/views/LyricsStudio.tsx --file app/src/views/AudioStudio.tsx --file app/src/views/Library.tsx --file app/src/views/CoverArt.tsx
```

## Outcome (2026-08-23)
**Aider result: FAIL on first pass, repaired in T-002b (commit `6e20244`).**

What Aider got right: the store shape, all five views with verbatim copy, icons
faithful to the brief's coordinates, correct CSS rules using only custom
properties, no new dependencies, no files touched outside the list.

What failed:
1. `App.tsx` used `JSX.Element` — React 19 removed the global `JSX` namespace (TS2503).
2. `NavIcon`'s shared props typed `aria-hidden`/`focusable` as strings, which do not
   satisfy React's `Booleanish` (TS2322 × 5).
3. It **committed twice anyway**, with `npm run build` exiting 2 and messages that
   ignored the `T-0XX:` convention.
4. It rewrote all 11 files as CRLF, so an 11-line CSS addition diffed as 336 lines.

Also repaired in T-002b (not Aider's fault — brief-level gaps): a failed
`appVersion()` rendered as `v<error text>`; both components subscribed to the whole
Zustand store; `.nav-item`'s `transition: var(--transition)` expanded to `all`.

**Process changes:** all launch commands now pass `--no-auto-commits`;
`.gitattributes` pins `eol=lf`; WORKFLOW §2/§4 record both plus the React-19 and
Zustand-selector traps for future reviews.

**Verified** in the browser pane with transitions disabled (they never advance
there): active item alone has accent border + bright text + accent icon, rail 208px,
no overflow, click switches heading and moves `aria-current`, fresh-tab console clean.

---

# T-003: create-core profile schema — ✅ **DONE 2026-08-23**
**Depends:** T-001 | **Crate:** `crates/create-core` | **Executor:** Aider

Full brief: **[t-003-brief.md](t-003-brief.md)**. Paste it into Aider after launching.

**Split from the original T-003**, which covered every domain type at once. This task is
the **model profile schema only** (`ModelProfile`, `InputSpec`, `LoraSupport`,
`SlotAddress`, `ComfySpec`) plus `profiles/ace-step-1.5-turbo.json` built from the verified
slot data, and round-trip tests. `Project`/`LyricDoc`/`Track`/`GenerationSpec`/`Provenance`
move to T-003b so each run stays under review size -- T-002 showed that a large executor
diff is where mistakes hide.

Two schema decisions were made in ARCHITECTURE §5 while writing this brief, both forced by
Rust rather than taste: `int`/`float` are separate input types because ACE-Step's seed
range reaches `u64::MAX` and `f64` cannot hold it exactly, and unsupported inputs are
**declared** (`"type": "unsupported"` with a reason) rather than omitted, so verified
absence is distinguishable from oversight.

## Outcome (2026-08-23) — commit `f3ea89a`
**Aider result: PASS**, the cleanest executor run so far. Types matched the brief, and the
fixture was **byte-identical** to the verified values — nothing rounded or "improved".
Eight tests green, clippy clean. `--no-auto-commits` worked: changes arrived in the working
tree for review rather than as commits, fixing T-002's failure mode at the source.

Fixed in review:
1. **`test_seed_max_roundtrips_exactly` was vacuous** — it round-tripped a bare `u64`
   through `serde_json` without touching our types, so it would have passed unchanged even
   if `Seed` became a float, which is the exact regression it exists to catch. **The brief's
   wording caused this** ("re-parse `u64::MAX` as a seed value through `serde_json`"), so it
   counts as a brief defect. Lesson for future briefs: *describe the invariant, not the
   mechanics* — say "prove a seed cannot be float-backed", not "round-trip a u64".
2. `cargo fmt`: import order, one over-long assert.
3. Crate docs still said "Populated by T-003" and listed types that do not exist yet.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file crates/create-core/Cargo.toml --file crates/create-core/src/lib.rs --file crates/create-core/src/profile.rs --file profiles/ace-step-1.5-turbo.json
```

---

# T-003b: create-core project, generation and provenance types — ✅ **DONE 2026-08-23**
**Depends:** T-003 | **Crate:** `crates/create-core` | **Executor:** Aider

Full brief: **[t-003b-brief.md](t-003b-brief.md)**. Paste it into Aider after launching.

`GenerationSpec` (+ `InputValue`, `LoraRef`, seed/batch helpers), `Project`, `LyricDoc`
(versioned, with the consent-gated `prompt_optimized` flag), `Track` and `Provenance`.

Three decisions were settled in the brief rather than left open, two of them recorded in
ARCHITECTURE §8:
- **`InputValue` is adjacently tagged.** Untagged, a JSON `3` could deserialise as `Int`,
  `Float` *or* `Seed`, and serde takes the first match — a seed silently demoted to an
  `Int` is an unreproducible track, which is the one thing provenance must prevent.
- **One source of truth per track.** `project.json` stores track *ids* only; title, file,
  duration and provenance live solely in the sidecar, so a rename cannot leave two files
  disagreeing.
- **Provenance keeps both levels** — the semantic `GenerationSpec` *and* the resolved slot
  values actually submitted. The first powers "re-use these settings"; the second is the
  only record of what the graph really received, and makes the duration/seed fan-out
  testable.

## Outcome (2026-08-23) — commit `4ce24f0`
**Aider result: PASS**, cleanest run yet — the only fix needed was `cargo fmt`. 19 tests
across the crate, clippy clean. Types matched the brief; all eleven named tests present
and non-vacuous, confirmed by reading each one rather than trusting the green.

Side finding: CONVENTIONS' "public items documented with `///`" was an overclaim. Checked
with `RUSTFLAGS="-W missing_docs"`: every public type, enum and function *is* documented;
the 49 gaps are all self-evident struct fields (`pub name: String`). The rule now states
the bar actually wanted rather than one nobody intends to meet.

**Pattern worth keeping:** three runs in, the executor lane is reliable when the brief
carries full reference code and names the *invariant* each test must protect. T-002 (prose
spec, no reference code) failed; T-003 and T-003b (full reference code) needed only
formatting.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/create-core/src/profile.rs --file crates/create-core/src/generation.rs --file crates/create-core/src/project.rs --file crates/create-core/src/provenance.rs --file crates/create-core/src/lib.rs
```

---

# T-004: config store, OS-keychain secrets, and Tauri commands — ✅ **DONE 2026-08-23**
**Depends:** T-003b | **Crates:** `crates/library`, `src-tauri` | **Executor:** Aider

Full brief: **[t-004-brief.md](t-004-brief.md)**. Paste it into Aider after launching.

**Split:** the TypeScript bridge and Zustand store move to **T-004b**, so a Rust review is
not mixed with a TypeScript one.

Three things were verified on the machine before the brief was written, rather than
recalled (CONVENTIONS: never write a third-party surface from memory):
1. **`keyring` 4.1.6 API** — `Entry::new` / `set_password` / `get_password` /
   `delete_credential`, compiled **and executed** against the real Windows Credential
   Manager: set/get/delete round-trips.
2. **The feature-flag trap** — defaults are `v1` + `windows-native-keyring-store` +
   `zbus-secret-service-keyring-store`; **macOS is not a default**, so without
   `apple-native-keyring-store` a mac build compiles and then has no store at runtime.
   This would have failed as a mystery bug on someone else's machine, not in CI.
3. **Atomic replace on Windows** — `write tmp -> sync_all -> fs::rename` over an existing
   file replaces contents and consumes the temp.

Two security decisions baked into the brief: secret names from the frontend are checked
against a **closed whitelist** (otherwise a buggy webview could write arbitrary keychain
entries), and **there is no `get_secret` command** — secret values never cross into the
webview; Rust reads them when building an outbound request, and the UI only learns whether
one exists.

## Outcome (2026-08-23) — commit `71440c9`
**Aider result: PASS**, only `cargo fmt` needed — the fourth consecutive clean run under
the reference-code pattern. The security boundary came through intact, which was the point
of the review: no `get_secret` command exists anywhere, every secret command parses its
name through the whitelist first, and `library` still has no `tauri` dependency.

Verified beyond the gate:
- **The ignored keychain test was run manually** (`cargo test -p library -- --ignored`) and
  passes against the real Windows Credential Manager — set/get/delete works end to end on
  this platform, not merely compiles.
- The corrupt-config test asserts the **exact original bytes** survive in
  `config.json.corrupt-N` and that the unreadable file is gone from its original path.

Added in review: `has_secret` reads the secret to answer, since the backends expose no
cheaper existence check. On macOS that can raise the keychain-access prompt on first use.
Documented with the consequence — call it on screen load, **never per-render or in a
polling loop**; Phase 1's setup wizard is exactly the code that would otherwise get this
wrong.

**Still unverified:** the commands have never been called from a running app. T-004b wires
the frontend; the first real exercise is the Phase 1 setup wizard.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file crates/library/Cargo.toml --file crates/library/src/lib.rs --file crates/library/src/config.rs --file crates/library/src/secrets.rs --file src-tauri/src/lib.rs
```

---

# T-004b: config bridge + store (brief pending)
**Depends:** T-004 | **Dir:** `app/` | **Executor:** Aider

`app/src/bridge/config.ts` (typed wrappers over the five commands) and
`app/src/state/config.ts` (Zustand store: config, warnings, load/save). Vitest against a
mocked bridge — no jsdom. Briefed once T-004's command surface is reviewed.

---

# T-005: CI — ✅ **DONE 2026-08-23** (architect, not Aider)
**Depends:** T-001 | **Files:** `.github/workflows/ci.yml`, root + app `package.json`

## Goal
GitHub Actions on push/PR: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --workspace` across ubuntu + windows + macos, plus `tsc -b`, oxlint, vitest
and `vite build`. Caching for cargo and npm.

## Outcome (2026-08-23)
**Reclassified off the Aider lane.** CI config is not the grunt work the executor lane is
for: it needs facts verified against upstream docs (Tauri's exact Linux packages), an
empirical answer about build ordering, and it cannot be validated by the executor at all
since only a real push proves it. Writing it as a brief would have meant writing the file
anyway. Recorded here so the routing decision is visible rather than implied.

**Two things verified rather than assumed:**
1. **Tauri 2 needs `libwebkit2gtk-4.1-dev`**, not 4.0 — checked against
   https://v2.tauri.app/start/prerequisites/ , with the full package list taken from there.
2. **The Rust jobs do NOT need the frontend built first.** `app/dist` is gitignored and so
   absent on a fresh clone; the concern was that `tauri::generate_context!` would fail
   without it. Tested by moving `app/dist` away, running `cargo clean -p app`, then
   `cargo check -p app` — clean compile. Only `tauri build` (which CI does not run) needs
   the bundled assets. The workflow carries a comment saying so, so nobody re-adds a
   redundant build step.

**Shape:** `frontend` runs once on ubuntu (TypeScript is platform-independent, so a 3-OS
matrix would triple minutes for no signal); `rust` is a matrix with `fail-fast: false`,
since cross-platform risk lives on the Tauri side. Concurrency cancels superseded runs;
`permissions: contents: read`.

**Matrix history — was event-dependent, now full again.** Kept here because the reasoning
recurs whenever a repo's visibility changes: Discovered right after the
first push: `github.com/rickmcelvana/latentCreate` is private (its Actions page 404s
publicly), and GitHub bills private-repo minutes with OS multipliers -- **Linux 1x,
Windows 2x, macOS 10x**, against 2,000 free minutes a month. A three-OS matrix on every
push charges roughly 60+ minutes per run, exhausting the monthly allowance in about thirty
pushes. So a tiny `targets` job picks the OS list: everyday pushes to master check Linux
only; pull requests, tags and `workflow_dispatch` run all three, which is where
cross-platform breakage actually needs catching. **The owner made the repo public on
2026-08-23**, so minutes became free and unlimited and the `targets` job was deleted in
favour of the plain three-OS list on every event (T-005c). Restore the trick only if the
repo goes private again.

**Also added: `npm run gate`** — one root command chaining the same checks in the same
order CI uses (`gate:rust` / `gate:app` for halves). This is the direct answer to T-002's
red commit: the producer can now prove green in one command before committing.

**Third-party actions used** (pinned by major tag): `actions/checkout@v4`,
`actions/setup-node@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`. All
widely used and permissively licensed; `rust-cache` is what keeps Tauri's dependency tree
from being rebuilt every run. SHA-pinning them is the stricter option if this repo later
wants it.

**Push status: pushed and ✅ green.** Pushed 2026-08-23 (`0dfd5c2..3c8ca9b`, 74 files);
the owner confirmed the first run passed. The Tauri Linux package list and the
"no frontend build needed for the Rust jobs" finding are therefore both confirmed by a
real run, not just locally.

---

# T-006: Milestone check (producer, no Aider)
Fresh clone → install deps → `npm run tauri dev` shows themed shell with nav; CI green on all three OSes; tag `phase0-done`. Paste results into PROJECT.md session log. Then architect opens `tasks/phase-1.md`.
