# PROJECT.md — latentCreate (living document)

> Load this file at the start of every session (Claude Code, Opencode, any agent). Update it at the end of every session. **Session-start rule: verify this file and ARCHITECTURE.md agree with `git log` since the last session entry; fix drift before new work.**

## Snapshot
- **Project:** latentCreate — open-source, desktop-only (Tauri 2) AI music creation front-end. Orchestrates user-provided ComfyUI (via Comfy MCP) for audio/image generation and a user-provided LLM for lyrics. **Ships no models.** Complements the closed-source siblings `../latent-mixing` and `../latent-mastering` (send-to targets) and the in-development latentPlayer.
- **Repo:** public, Apache-2.0, `github.com/rickmcelvana/latentCreate`. CI green on ubuntu/windows/macos.
- **Phase:** **0 complete, tagged `phase0-done`** (2026-08-23). The app builds, runs, has a nav shell over five placeholder views, a complete domain model, a config store with OS-keychain secrets, and CI. It does not talk to ComfyUI yet.
- **Next up:** **Phase 1** — [tasks/phase-1.md](tasks/phase-1.md). The blocking `rmcp` verification is **done** (docs/MCP-SURFACE.md §8) and **[T-101's brief](tasks/t-101-brief.md) is written and ready to run** — the next action is a producer Aider run against it.
- **Stack (as built):** Rust 1.97 workspace (`create-core`, `mcp-bridge`, `llm-bridge`, `library`, `src-tauri`) + Tauri 2.11; React 19.2 + TS 6 strict + Vite 8 + Zustand + vitest 3 + oxlint. Plain CSS, one `theme.css`. `app` is an **npm workspace** — one `npm install` at the root.

## Working commands
```bash
npm install     # root + app workspace, one step
npm run dev     # desktop app (Tauri); run from the repo ROOT, not app/
npm run gate    # everything CI runs, in CI's order -- the pre-commit check
cargo test -p library -- --ignored   # the live-keychain test, excluded from CI
```

## How work happens
- WORKFLOW.md defines the Claude(architect)/Aider(executor)/human(producer) loop, adapted from latent-mastering. This repo is almost entirely plumbing/UI → default executor `ollama_chat/kimi-k2.7-code:cloud`. No DSP lane exists here (the visualizer is AnalyserNode + canvas, not custom math).
- Tasks live in `tasks/phase-N.md`; anything non-trivial gets its own `tasks/t-NNN-brief.md` with a ready-to-paste Aider launch command. One brief per run, ≤ ~400-line diffs.
- **The loop, as it actually settled in Phase 0:** architect writes the brief with full reference code → producer runs Aider with `--no-auto-commits` → producer runs `npm run gate` → architect reviews the working tree against the brief → **architect commits** `T-NNN: title` → push. Executors never commit; the architect does, on a green gate, without waiting to be asked. Architect-only work (briefs, docs, verification) follows the same rule minus the Aider step.

## Key decisions log
- **2026-08-23 — MCP-first Comfy integration.** App embeds an MCP *client* (rmcp, stdio to local `comfy-mcp`; HTTP to Comfy Cloud) rather than ComfyUI's raw HTTP API. Rationale: model search/download, templates, validation, and job tools come free; local/cloud is one trait, two transports (ARCHITECTURE §1, §3). Raw API fallback deliberately deferred (OQ-3). ⚠ *The local/cloud half of this was disproved the same day — see the verification entry below; MCP-first itself stands and was strengthened (slots).*
- **2026-08-23 — Model capability profiles as JSON data** (ARCHITECTURE §5). Supporting a new music model = a profile file, not code. Default model: ACE-Step 1.5 (Apache-2.0, lyrics+vocals, consumer-GPU fast). Also profiled: Stable Audio Open, MusicGen, YuE (advanced), DiffRhythm.
- **2026-08-23 — Prompt optimization is consent-gated.** Optimizer output always shown as a diff; user accepts/edits/reverts; provenance records the flag. Never silently rewrite user text (owner requirement).
- **2026-08-23 — Provenance sidecars** for every generated asset (full inputs+seed+model). JSON store, no DB.
- **2026-08-23 — Clean-room visualizer.** Sibling repos are closed-source; default is reimplementing the small viz layer against AnalyserNode rather than porting, to keep this repo unencumbered (owner may explicitly relicense pieces they own — record it here if so).
- **2026-08-23 — Delete = OS trash**, never hard delete (safety rule, also matches suite behavior).
- **2026-08-23 — License: Apache-2.0** (owner decision, closes OQ-1). Verbatim `LICENSE` + `NOTICE` in place. Rationale: patent grant, matches the ACE-Step/permissive model ecosystem, compatible with the closed-source siblings consuming outputs. Consequence for briefs: **permissive-only dependencies**; any copyleft dep needs its own decisions-log entry, and no code may be copied in from `../latent-mixing` / `../latent-mastering` without a recorded relicensing note. NOTICE holder is currently "latentCreate contributors" — swap in a legal name/entity if the owner prefers.
- **2026-08-23 (later) — Live MCP verification changed several decisions.** Full evidence: [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md). Headlines:
  - **Local and cloud comfy-mcp are different tool surfaces**, not one surface behind two transports (the earlier claim was wrong). v1 targets **local stdio only**; a cloud backend must be verified against a live cloud endpoint before it is written.
  - **Slots (`list_workflow_slots`/`set_workflow_slot`) are the parameter mechanism** — stable `node.input` addresses with current values. The app never parses graph JSON to change a parameter, and profiles shrink to *semantic input → slot address(es)*.
  - **One UI control may drive several slots.** ACE-Step 1.5 turbo has duration in two places and two independent seeds (planner + sampler). Profiles map to a list of addresses.
  - **ACE-Step 1.5 has no negative prompt.** Profile sets `negative.supported: false`.
  - **The LoRA loader is core `LoraLoaderModelOnly`**, not a custom ACE node, and the real LoRA list is ~90% training-checkpoint noise → the picker needs filtering/grouping design, not a dropdown (ARCHITECTURE §5a).
  - **The shipped template saves lossy MP3 via a deprecated node.** For an app feeding a mastering chain that is unacceptable — the pipeline swaps in `SaveAudioAdvanced` lossless (open sub-question: its `format` is a V3 dynamic combo). **Owner confirmed he already swaps this node out of every workflow by habit**, which makes automating it a correctness requirement rather than a nicety: the app should never hand a lossy file to the mastering stage.
  - **Profile authoring rule:** only against a template verified `runnable: true` on a real install, with slots read live. Never from docs or model cards.
- **2026-08-23 — LoRA support is a v1 requirement, not a later feature.** The owner's own production workflow is ACE-Step 1.5 turbo + custom-trained LoRAs, so profiles gained a `loras` block (loader node class, dirs, strength range, max stack), AudioStudio gains a LoRA stack panel, and the provenance sidecar records file+strength+order (ARCHITECTURE §5a). Consequences: LoRA-bearing runs need graph editing rather than parameter-setting alone; LoRA *training* stays out of scope (ComfyUI node packs already do it well). *(Tool names in this entry were pre-verification; the later entry below and docs/MCP-SURFACE.md carry the corrected mechanism — loader is core `LoraLoaderModelOnly`, enumeration via `nodes(action="get")`, run via `run_workflow`.)*
- **2026-08-23 — Custom workflow import** (ARCHITECTURE §5b), added alongside LoRA support. Users import an API-format workflow, map its inputs once, and it becomes a user profile. Prevents the profile abstraction from locking out people who already have working graphs — which is most serious ComfyUI users.
- **2026-08-23 — MiniMax Music 3 added to the seed profiles.** Released 2026-08-13/14, currently the strongest open-weights option (5-minute songs, vocals that read as performed). ⚠ Its license is open-weights-*with-conditions* (attribution required; separate agreement above ~$20M revenue), unlike ACE-Step's Apache-2.0 — the UI must surface per-model license terms, since users ship these tracks commercially. Does not affect this repo's own Apache-2.0 licensing (we ship no weights). ComfyUI support level **since verified**: a native, free (non-API) template `audio_minimax_music_3` exists, but its weights are not on the dev box — profile blocked, see OQ-6.
- **2026-08-23 — Branding source of truth is `latentbeats.com`**, not the sibling apps. `../website/latentbeats.com/css/style.css` carries the suite's current tokens; the owner rebranded the umbrella site **violet → blue a few days before 2026-08-23**, so any doc or repo still saying "violet" is behind the brand, not wrong-at-the-time. latentCreate's `theme.css` mirrors the site (`--bg: #0a0e1a`, `--accent: #58a6ff`, `--radius: 12px`, card shadow, 180ms transitions). This means latentCreate is **intentionally bluer/deeper than Latent Mixing and Mastering**, which still run the older GitHub-dark ground (`#0d1117`, `#30363d`); the accent is identical in all three. If the siblings are later brought onto the site palette, latentCreate needs no change. Rule for agents: change the site first, then follow it — never fork token values in an app.
- **2026-08-23 — Lyric-LLM recommendations** (closes OQ-2). From the owner's hands-on use across many local models: **Gemma 4 12B is the standout at its size** for lyric writing, and **Gemma 4 26B–31B perform well for users with the VRAM to run them**. These become the app's *suggested* models in the setup wizard's LLM step and in docs — suggestions only, never a gate: any OpenAI-compatible endpoint remains first-class (ARCHITECTURE §4). Not benchmarked in-repo; recorded as owner experience so agents don't re-litigate it. Full list with sizing: docs/MODELS.md.
- **2026-08-23 — `rmcp` verified; three findings reshape every future tool wrapper.** Full evidence: [docs/MCP-SURFACE.md §8](docs/MCP-SURFACE.md). Pinned **rmcp 3.1.4 with `default-features = false`** (client + child-process transport only; the default set pulls rmcp's entire *server* half). Consequences that outlive T-101:
  - **comfy-mcp returns JSON inside a text block.** `structured_content` is always `None` and not one of its 39 tools publishes an `output_schema`. Every wrapper in T-103–T-106 is a **two-stage decode** — extract text, then `serde_json::from_str` into our own type. There is nothing to derive types from, so each one is hand-written against a payload observed live.
  - ⚠ **A failing tool call returns `Ok`, not `Err`** — bad arguments, missing files, *and unknown tool names* all arrive as `Ok(is_error: true)`. A wrapper matching only `Result::Err` reports every ComfyUI failure as success. This is the shape of bug that ships.
  - **`TokioChildProcess` already kills the child on drop**, so ARCHITECTURE §3's "child killed on drop" needs no code of ours — and hand-rolling it would fight the transport.
  - Smaller: `CallToolRequestParams` is `#[non_exhaustive]` (struct literals do not compile); `call_tool` has **no default timeout**; a missing binary is `io::ErrorKind::NotFound` (T-110's detection signal); rmcp raises the workspace MSRV to **1.88**; everything it pulls in is permissively licensed.
  - **The `ComfyBackend` trait is deferred from T-101 to T-104.** Async fns in traits are not object-safe, so dyn-vs-enum dispatch should be decided when a backend first enters Tauri managed state — not guessed now around a single impl.
- **2026-08-23 — Send-to sequencing** (closes OQ-4 for this repo): the real handoff mechanism is being built on the mixing/mastering side **before** latentCreate reaches Phase 4. latentCreate therefore builds only the v1 link-out + reveal-file behavior and adopts whatever protocol those repos define when it exists — this repo does not design the handshake. Re-check the mixing/mastering repos' state at the start of Phase 4.

## Open questions (owner to decide)
- ~~**OQ-6 MiniMax Music 3 profile**~~ — **RESOLVED 2026-08-23.** Owner installed the int8 weights (all three files). The template still fails `local_check` on one line because it hardcodes the **fp16** DiT filename; overriding `37/6.unet_name` makes `validate_workflow` return clean — verified end to end. The profile can be written in Phase 1 without further setup; the fp16 DiT is optional and only for a quality comparison. Superseded detail below kept for context: *(original)* The native template `audio_minimax_music_3` exists and is free/local, but the three model files are not on the main dev box (which has MiniMax **H3**, the video model, instead). **Owner confirmed 2026-08-23:** the Music 3 testing was done on the other PC, and this box is his model-testing machine where new models are installed to try and then removed — so absent weights here mean nothing about the model. Options: install the weights here when the profile is written (multi-GB, owner's call), author it on the other PC, or defer to Phase 3. Update ComfyUI first regardless — core is one release behind and the template threw V3 type warnings consistent with template-newer-than-install.
  - **Standing implication for agents:** never infer "model unsupported/unavailable" from this machine's installed-model list. It is a testing box whose model set churns. Ask, or check the template rather than the weights.
- **OQ-3 Raw ComfyUI API fallback.** Build a second `ComfyBackend` impl against `/prompt`+websocket if comfy-mcp proves limiting (e.g. arbitrary node-input introspection)? Deferred until Phase 3 evidence exists.
- **OQ-5 App identity — parked, do not force a decision.** `latentbeats.com` is the umbrella for the whole suite; "latentCreate" is the working name and is fine to ship in docs/UI for now. Final product name comes out of a dedicated brainstorming session the owner will schedule. **Agents: do not propose or apply branding changes unprompted**; keep the name in a small number of places (README title, `package.json`/`tauri.conf.json` product name, window title) so a later rename is cheap.

*Resolved: OQ-1 (Apache-2.0), OQ-2 (lyric-LLM guidance), OQ-4 (send-to owned by mixing/mastering) — all in the decisions log above.*

## Backlog (accepted, not yet scheduled)
- Album lists → bulk send-to-mastering once mastering's bulk import lands (owner-stated future feature).
- latentPlayer integration (library hand-off) once player matures.
- Audio-to-audio flows (cover/remix/extend) for models that support it — profiles already leave room via `inputs`.
- Community profile sharing (import a model profile JSON from URL).
- In-app "what's new in models" surface — periodic `search_models` diff against catalog. Graceful upgrade UX sketched in ARCHITECTURE §10 step 2.

## Session log

### 2026-08-23 — planning through Phase 0 (one long session, Claude)
Repo went from empty to `phase0-done` in a single session. Condensed, because the durable
decisions live in the log above and the per-task detail in `tasks/phase-0.md`:

1. **Planned** the app from the owner's brief: researched Comfy MCP and the 2026 open
   music-model landscape, wrote README/ARCHITECTURE/CONVENTIONS/WORKFLOW/AGENTS,
   docs/RESEARCH.md, docs/MODELS.md, the roadmap and Phase 0 briefs.
2. **Verified against the live install** rather than the docs, which rewrote parts of the
   plan: local and cloud comfy-mcp are different tool surfaces, slots are the parameter
   mechanism, ACE-Step has no negative prompt, the LoRA loader is core
   `LoraLoaderModelOnly`, and the shipped template writes lossy MP3. See
   docs/MCP-SURFACE.md — **the authority for anything MCP**.
3. **Built Phase 0**: scaffold, nav shell, `create-core` (profile schema + domain types),
   `library` (config + keychain), five Tauri commands, the frontend bridge and store, CI,
   and a cross-language wire fixture.
4. **Closed it** with the T-006 milestone, which caught a broken fresh clone that five
   green CI runs had missed.

**What the next session should carry forward** (each one cost something to learn):
- Briefs for the executor lane need **full reference code** plus, per test, the **invariant
  it protects**. The one prose-spec brief (T-002) came back not compiling; every brief with
  reference code needed only `cargo fmt`.
- A brief that names a *mechanism* produces a test blind to its purpose. Happened twice —
  T-003's seed test and T-004b's fixture import. Say what must fail, not what to write.
- **Verify third-party surfaces by compiling and running them**, in a throwaway crate
  outside the repo. That method caught keyring's non-default macOS backend and serde's
  `"open_ai_compat"` string — both invisible to review, both would have shipped.
- Aider runs with `--no-auto-commits` and never commits; it once pushed two commits past a
  failing build.
- CI must exercise the **documented** setup path (WORKFLOW §4b).

### 2026-08-23 (later) — Phase 1 opened: `rmcp` verification (Claude, architect)

Session ritual first: PROJECT.md and ARCHITECTURE.md checked against `git log` — no drift,
tree clean. Then the one blocking item at the top of `tasks/phase-1.md`.

1. **Verified `rmcp` the Phase 0 way** — two throwaway crates outside the repo, compiled and
   run against the owner's live `comfy-mcp`. All five questions answered; findings in
   docs/MCP-SURFACE.md §8 and summarised in the decisions log above.
2. **Prototyped `mcp-bridge` itself** rather than stopping at API notes: `ComfyError`,
   `LocalComfy::connect/call/health/stats/shutdown`, slot-address splitting, error-slug
   parsing. It passes `clippy -D warnings` under the workspace's edition 2021 and runs
   clean against the live server, so **T-101's brief carries code that is known to compile**
   — the Phase 0 lesson that prose-spec briefs come back broken.
3. **Wrote [tasks/t-101-brief.md](tasks/t-101-brief.md)**, ready to run.

**Carry forward:**
- The two throwaway crates are gone with the scratchpad; §8 is the durable record. Anything
  it does not say about rmcp is unverified, including the streamable-HTTP transport a cloud
  backend would need.
- `list_workflow_slots` on the frozen MiniMax fixture returns **24 of 25 addresses in
  subgraph form**. T-103's warning is now a measurement, and the rule is concrete: split a
  slot address on the **last** `.`, because node ids contain `/` but never `.`.
- The verification paid for itself twice over. "A failed tool call returns `Ok`" and "results
  are JSON-in-text" are both invisible in review and both would have produced a bridge that
  silently reports success — the same class of finding as Phase 0's keyring backend.
