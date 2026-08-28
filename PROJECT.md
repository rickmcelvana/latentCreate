# PROJECT.md — latentCreate (living document)

> Load this file at the start of every session (Claude Code, Opencode, any agent). Update it at the end of every session. **Session-start rule: verify this file and ARCHITECTURE.md agree with `git log` since the last session entry; fix drift before new work.**

## Snapshot
- **Project:** latentCreate — open-source, desktop-only (Tauri 2) AI music creation front-end. Orchestrates user-provided ComfyUI (via Comfy MCP) for audio/image generation and a user-provided LLM for lyrics. **Ships no models.** Complements the closed-source siblings `../latent-mixing` and `../latent-mastering` (send-to targets) and the in-development latentPlayer.
- **Repo:** public, Apache-2.0, `github.com/rickmcelvana/latentCreate`. CI green on ubuntu/windows/macos.
- **Phase:** **0, 1 and 2 complete**, tagged `phase0-done` (2026-08-23), **`phase1-done` (2026-08-25)** and **`phase2-done` (2026-08-26)**. Phase 2 (Lyrics Studio) shipped T-201 … T-210 plus the T-211 live milestone and its fix-ups (T-212 … T-214): a user fills in a brief, watches lyrics stream from their own LLM, edits across versions, runs an advisory structure lint, optionally accepts a consent-gated optimized prompt, and approves a version for audio. **Verified live on 2026-08-26** against a real Ollama -- a whole song in 6.9-9.2 s with reasoning suppressed (against a 44 s / 85-character baseline), the optimizer preserving its five fixed lines 5/5, and the approved lyric on disk with its `prompt_optimized` consent flag. `mcp-bridge` (88 offline tests) covers the verified comfy-mcp surface; `llm-bridge` (35 + 4 live) covers OpenAI-compatible streaming plus Ollama's native API. **Phase 3 is in progress** -- the pure half and the pipeline that runs it are both done (T-304 ... T-306b): the app can now turn a spec into a queued ComfyUI job. **No generation has been run through it yet** -- the four-call sequence is proven against the mock transport, and T-314 is the first live run.
- **Landed in Phase 1:** T-101 (stdio transport, `ComfyError`, health), T-102 (mock transport rig), T-102b (session log + redaction), T-102c (stderr capture + free-text redaction), T-103a (templates + `local_check` tri-state), T-103b (slots + self-verifying writes), T-103c (validation verdicts + untrusted notes), T-104a (job lifecycle wrappers), T-104b (Tauri managed state + job event pump), T-104c (frontend jobs bridge + store + queue panel), T-105a (model discovery), T-105b (model download), T-106 (node registry), T-106b (`minimax-music-3` profile + `slot_overrides`), T-107a (profile loader), T-107b (profile slot addresses), T-108a/b/c (`llm-bridge` `openai_compat`: SSE framing, wire types, streaming client), T-109a/b (`ollama_native`: model listing + pull with progress), T-110a/b/c (Setup wizard ComfyUI step: typed `server_info`, `ComfyStatus` tagged union, health pill with a next step per state), T-111a-e (models step: profiles declare their model files, readiness by exact match against `search_models`, per-file install with byte-weighted progress, licence on every row), **T-112a-d (LLM step: capability-filtered picker, remote-model privacy disclosure, suggestions as data, test call)**. The comfy-mcp surface these were built against is **verified live** and recorded in [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) — that file is the authority, not the tool docs.
- **Landed in Phase 3 so far** (all 2026-08-27): the phase-start surface re-verification (docs/MCP-SURFACE.md §16), then **T-301** (no lyric model is recommended), **T-301b** (endpoint + API-key fields, so any OpenAI-compatible provider works -- verified live against QwenCloud), **T-302** (measured the cost of the conservative `reasoning_effort` rule: 11.8x billed tokens), **T-302b** (the app discovers acceptance per endpoint instead of inferring it -- 33 s became 1-2 s), **T-303** (`default_profile_id` persists; profile picker), **T-304** (`resolve_slots`: semantic inputs fanned out to slot addresses), **T-305a** (`ensure_lossless_output`), **T-305b** (`splice_loras`), **T-306a** (`to_slot_value` + `audit_slots`, and the ACE-Step seed fix), **T-306b** (the pipeline command, and the `test-support` mock feature that lets its call sequence be asserted offline). `create-core` is 126 tests, `app` 52.
- **T-306b landed 2026-08-27** ([brief](tasks/t-306b-brief.md)): `generate_audio(spec)` does `fetch_template` to a per-job working copy -> audit the resolved addresses -> `set_slots` -> the T-305 graph edits -> `validate_workflow` -> submit into the **existing** `jobs::run_workflow` pump. `app` is 52 tests. **T-307 landed 2026-08-28** ([brief](tasks/t-307-brief.md)): the raw 53-entry LoRA list becomes 12 pickable entries in 6 groups, and the list itself is finally captured as a fixture (`a5424eb`). `create-core` is 137 tests. **T-308a landed 2026-08-28** ([brief](tasks/t-308a-brief.md)): the profile's declared inputs become ordered controls with defaults, bounds and a basic/advanced split, and a typed `GenerationSpec.inputs`. Frontend is 141 tests. **T-308b's data path and panel store landed 2026-08-28** ([brief](tasks/t-308b-brief.md)) at 151 frontend tests and 54 app tests; **`<ParamPanel>` landed 2026-08-28** at 154 frontend tests and **passed its producer click-through the same day, all seven rows** -- the first Phase 3 work a person has actually looked at, including a human confirming that a seed of `18446744073709551615` is refused rather than silently rounded. **T-308c landed 2026-08-28** and **passed its click-through**, which corrected it: the three enums fill from the node registry, and a cached answer says so -- while a live one now stays quiet. mcp-bridge 92, app 58, frontend 162 tests (2026-08-28). **T-309a landed 2026-08-28** ([brief](tasks/t-309a-brief.md)): the `lora_panel` command, `state/loras.ts` and its store -- the 53-entry list reaches the picker as 12 offers in 6 groups, with the 20 training checkpoints behind a disclosure and the 21 non-adapters counted rather than vanished. Sixteen mutations, sixteen killed. create-core 138, app 65, frontend **195** tests. **T-309b landed 2026-08-28** ([brief](tasks/t-309b-brief.md)) and **passed its click-through**: the stack panel is on screen, hidden entirely for a model with no `loras` block and visible with a Retry when ComfyUI cannot be read. Frontend **206** tests. The click-through also answered the label question T-307 and MCP-SURFACE 12.2 both deferred: **the labels stay mechanical**. That removes most of T-309c's premise -- see the decisions log. **T-309d landed 2026-08-28** ([brief](tasks/t-309d-brief.md)), click-through pending: the Audio view can now start a generation. Written because nothing could -- `generate_audio` (T-306b), `specInputs` (T-308a) and `specLoras` (T-309a) were three tested seams meeting nowhere, and the phase file had no task joining them, so T-310's queue panel would have shipped able to display jobs nothing could create. Briefing it found the defect that would have shipped with it: the pump's events had nowhere to land (decisions log). Frontend **239** tests. Briefing it live corrected a claim written into MCP-SURFACE 19.1 the day before: a `lora_name` the server does not know is rejected by `validate_workflow` as `unknown_enum_value`, so a **deleted** LoRA under a stale picker is a loud failure, and 17.6's silent no-op belongs only to the *non-adapter* case, which validates clean because it is a real member of the enum (19.3). Live enum choices are **T-308c**: the profile names a node *instance*, not a *class*, so key/scale, time signature and language have no options until `InputSpec::Enum` gains a `node` field. **Nothing has generated audio yet** -- the pipeline is proven against the mock, never against a GPU; T-314 is the first live run and also the first chance to settle `vram_gb_min: 8`.
- ⚠ **Three findings from Phase 3 constrain everything downstream**, all verified live and recorded in [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) §§17-18. Read them before touching the pipeline: **(1)** a LoRA splice that feeds nothing validates clean, runs and writes audio -- `validate_workflow` is a schema check, not a reachability check (§17.1). **(2)** `set_workflow_slot` reporting an address `applied` does not mean the value reaches the engine (§18.1). **(3)** ACE-Step is **not reproducible run-to-run** even with a fixed seed -- two identical runs differ in 98.1% of bytes -- so no test or check may rest on two runs matching, and provenance reproduces the *inputs*, not the waveform (§17.3). `GET /history/<prompt_id>` is the only surface that shows what actually ran (§17.2).
- **Stack (as built):** Rust 1.97 workspace (`create-core`, `mcp-bridge`, `llm-bridge`, `library`, `src-tauri`) + Tauri 2.11; React 19.2 + TS 6 strict + Vite 8 + Zustand + vitest 3 + oxlint. Plain CSS, one `theme.css`. `app` is an **npm workspace** — one `npm install` at the root.

## Working commands
```bash
npm install     # root + app workspace, one step
npm run dev     # desktop app (Tauri); run from the repo ROOT, not app/
npm run gate    # everything CI runs, in CI's order -- the pre-commit check
cargo test -p library -- --ignored   # the live-keychain test, excluded from CI
```

**13 `#[ignore]` tests** across the workspace are live harnesses -- they need a running Ollama,
a hosted endpoint with a stored key, the OS keychain, or gigabytes of download, so CI never runs
them. Each carries its reason in the attribute. `cargo test -p <crate> -- --ignored` runs a
crate's set; two of them (`src-tauri/src/lyrics.rs`) **spend API credits**, so read the reason
before running one.

**Where the app writes** (Tauri `app_config_dir()`, identifier `com.latentbeats.create`):

| | Windows | macOS | Linux |
|---|---|---|---|
| App data root | `%APPDATA%\com.latentbeats.create\` | `~/Library/Application Support/com.latentbeats.create/` | `~/.config/com.latentbeats.create/` |

Inside it: **`config.json`** (non-secret config -- endpoint, model, `default_profile_id`),
`session.log`, and `projects/<slug>/` holding each project's `project.json`, `lyrics/` and
later its `tracks/`. **`config.json` sits beside `projects/`, not inside it** -- the one
place people look first and it is not there. API keys are **not** in `config.json`; they are
in the OS keychain (T-004), and no Tauri command returns a secret value.

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
- **2026-08-24 — `ComfyBackend` trait re-deferred (from T-104 to "until cloud is verified").** The T-104 planning session confirmed holding `LocalComfy` concretely in Tauri managed state rather than introducing the trait now. Three reasons, concrete rather than speculative: (1) still a single impl; (2) the ARCHITECTURE §3 sketch has drifted from the landed methods (`search_templates(query, limit) -> TemplateSearch`, batch `set_slots`, `list_slots -> SlotList`, missing `get_template`/`notes`); (3) MCP-SURFACE §1 proves local/cloud are different surfaces, so the eventual seam is more likely `enum Backend { Local, Cloud }` than a 17-method trait — shaped when cloud is actually verified, not guessed. ARCHITECTURE §3 now marks the trait a sketch and records this.
- **2026-08-24 — The profile/template check is split across the crates that already own each half.** `ModelProfile::slot_addresses()` (create-core, pure) collects every address a profile names; `SlotList::missing` (mcp-bridge, landed T-103b) does the comparison against a fetched template; they meet in `src-tauri`. Rejected: putting the comparison in `library` (it would need a `mcp-bridge` dependency, against ARCHITECTURE §2's role for the on-disk store) and adding a second comparison function to `create-core` (two answers to one question). Consequence: nothing in the check needs a live ComfyUI to test, and the wiring is one call at the T-110 seam.
- **2026-08-24 — Streamed LLM text is typed, not concatenated.** `ChatDelta` is an enum (`Content` / `Reasoning` / `Refusal` / `Finished` / `Usage`) and **only `Content` may reach the user's document**. Forced by a live capture: `gemma4:12b-it-qat`, the model recommended for lyrics, answered a one-word prompt with 163 characters of `delta.reasoning` and 5 of `delta.content`. Two spellings exist (`reasoning`, Ollama/OpenRouter/current vLLM; `reasoning_content`, DeepSeek/older vLLM) and both are read, because clients that know one have shipped the bug of dropping the other. Full evidence: [docs/LLM-SURFACE.md](docs/LLM-SURFACE.md).
- **2026-08-24 — `LlmProvider` deferred to T-109**, the same rule already applied twice to `ComfyBackend`: a trait with one implementation is a guess about where the seam goes. T-108 ships `OpenAiCompat` concretely; T-109's `ollama_native` is what will show which methods actually differ.
- **2026-08-24 — TLS is `rustls` with the OS trust store**, via reqwest 0.13's `rustls` feature (which pulls `rustls-native-certs`). No OpenSSL enters the tree, so Linux CI needs no `libssl-dev`. Every added crate is permissive (MIT/Apache-2.0/ISC/BSD-3-Clause), verified with `cargo metadata` rather than assumed — LLM-SURFACE 7. Note reqwest 0.13 **renamed these features**: the 0.12 names `rustls-tls` and `rustls-tls-native-roots` do not exist and fail to resolve.

- **2026-08-24 — TLS provider: rustls with aws-lc-rs, accepting a C build step.** reqwest 0.13's `rustls` feature pulls `aws-lc-rs` (built via cmake) and `rustls-platform-verifier`, adding ~400 lines to Cargo.lock and a native compile to every clean build. The alternative, `rustls-no-provider` + `ring`, avoids the C toolchain but requires the process to call `CryptoProvider::install_default()` before any TLS — forget it and **every** request fails at runtime. A slower build beats a runtime footgun in a desktop app whose TLS path is exercised only when the user configures a cloud endpoint. CI already installs `build-essential` and the runners ship cmake. Revisit if CI build times become painful.

- **2026-08-24 — `LlmProvider` is not written, and T-109 is why.** The trait was deferred at T-108 for lack of a second implementation; T-109 supplied one and it turned out not to be an implementation of the same thing. **`ollama_native` does not chat** — Ollama's `/v1/chat/completions` already goes through `openai_compat`, so the native API is an *enrichment layer* (which models can chat, which think, which are remote), not a peer provider. Forcing it into the trait would mean a `stream_chat` returning an error, the shape of a wrong abstraction. `anthropic` will settle the question, because it genuinely chats with a different wire format. ARCHITECTURE 4 records it.
- **2026-08-24 — Remote models are a privacy disclosure, not a performance note.** Ollama lists cloud models beside local ones with a `remote_host` field; generating with one sends the user's unreleased lyrics to another party. This app's premise is local-first generation, so the UI must show that distinction wherever a model is chosen — the same rule already applied to per-model licence terms (CONVENTIONS).
- **2026-08-24 — The backend classifies service states; the frontend renders them.** `ComfyStatus` is a serde-tagged union (`not_installed` / `unreachable` / `server_down` / `ready`) and `comfy_status` **never returns `Err` for a service problem** — only for this app failing to open its own session log. Rationale: CONVENTIONS requires degraded services to become a status pill with a next step, which is only possible if the states are enumerable. A frontend deciding what to show by parsing error strings is the alternative, and it breaks the first time a message is reworded.
- **2026-08-24 — `[port_in_use]` is not a launch failure.** Verified live: launching while something already holds 8188 fails with that code, which means something is already serving. `comfy_launch` ignores it and reports whatever the following health check finds, rather than alarming a user whose ComfyUI is simply already up.

- **2026-08-25 — model readiness is decided from the profile, not from ComfyUI.** No comfy-mcp
  tool answers "which model files does this workflow need": `workflow_deps` maps node classes to
  node *packs*, `node_dependencies` checks a pack's Python requirements, and `local_check`'s
  errors are English prose. Each profile therefore declares `comfy.models` (file, folder,
  source_url, size), and readiness is exact string matching against `search_models(folder=)`.
  **`local_check.runnable` is explicitly not used** — it answers a different question, and
  MiniMax Music 3 proves the gap: fully installed, `runnable: false`, over a filename its own
  `slot_overrides` already corrects (MCP-SURFACE 14).
- **2026-08-25 — "update available" is dropped for models.** `search_models` returns filenames
  only, with no hash, version or timestamp. Nothing local can distinguish a stale checkpoint
  from a current one, so the badge would be invented. It stays on ComfyUI core, where
  `freshness` supplies real data. The advanced `search_models` browser is backlogged for the
  same kind of reason — it is a different feature from "can I use this profile".
- **2026-08-25 — licence text comes from the profile, never from the download host.**
  `Comfy-Org/MiniMax-Music-3` is tagged Apache-2.0 on Hugging Face; the upstream
  `MiniMaxAI/MiniMax-Music3` carries a bare LICENSE file with a custom community licence,
  an attribution obligation and a revenue threshold. The repackager's tag describes the
  repackaging, not the weights, and showing it would misstate the user's obligations.
- **2026-08-25 — model capabilities are `Option<bool>`, and unknown is never rendered as
  false.** The OpenAI-compatible `/v1/models` returns ids and nothing else, so against any
  endpoint that is not Ollama the app cannot tell an embedding model from a chat model, or a
  local model from one running on someone else's servers. Reporting `is_remote: false` there
  would tell a user their unreleased lyrics stay on their machine when nobody checked. The UI
  says "capabilities unknown" instead, and still lets the model be chosen -- hiding unchecked
  models would strand a user on LM Studio with an empty picker (LLM-SURFACE 11.1, 11.2).
- **2026-08-25 — the LLM test call succeeds on a well-formed response, not on non-empty
  content.** A reasoning model spends its token budget on chain-of-thought before writing a
  word: 20 tokens returned empty content and `finish_reason: length` from a healthy endpoint.
  Asserting text would report a broken setup to a user whose setup is fine (LLM-SURFACE 11.4).
- **2026-08-25 — lyric-model suggestions ship as `data/lyric-llms.json`, matched by id
  prefix.** docs/MODELS.md required the list be read as data rather than hardcoded; the table
  there is now the human-readable twin of that file. Prefix matching is not a convenience: the
  verification machine has `gemma4:12b-32k` and `gemma4:12b-it-qat` and **no endpoint reports a
  tag named plainly `gemma4:12b`**, so equality would recommend nothing while the recommended
  model sat installed. Because two variants can match, the preselect takes the lowest id, and a
  model the user already configured always wins over any suggestion.
- **2026-08-25 — lyric generation sends `reasoning_effort: "none"`, and only where the model
  is known to think.** Captured live: an assembled lyric prompt to `gemma4:12b-32k` with a
  2000-token budget returned **85 characters of lyrics and 7458 of reasoning**,
  `finish_reason: length`, first content delta **44.08 s into a 44.65 s stream**. With
  `reasoning_effort: "none"` the same brief is a complete song in 8.2 s and ~400 completion
  tokens. Ollama's own `think: false` is **accepted and silently ignored** over the
  OpenAI-compatible endpoint, and `"low"` is not honoured, so neither is a substitute. The
  field is verified against Ollama only; it is therefore sent **only when `thinks` is true**,
  a fact that exists only where the native enrichment layer answered (T-112). Any endpoint the
  app cannot enrich never sees the field, which keeps the unverified path untaken rather than
  defended. Full evidence: [docs/LLM-SURFACE.md §12](docs/LLM-SURFACE.md).
- **2026-08-25 — the streamed reasoning is rendered, not just filtered.** T-108 typed
  `ChatDelta::Reasoning` so it could be kept out of the user's document. The capture above
  shows it is also the **only proof of life for 44 of 45 seconds**, so the Lyrics Studio shows
  it as status text. A generation with nothing on screen is indistinguishable from a hang, and
  the fix is not a spinner — it is the content the model is already sending.
- **2026-08-25 — structure-tag validation is advisory, and matching ignores numbering.**
  `TextEncodeAceStepAudio1.5.lyrics` is a bare STRING: empty `choices`, empty description.
  **Nothing in the install publishes which tags ACE-Step accepts**, so a blocking rule would
  enforce a guess against the user's own words — which the never-modify-user-text rule already
  forbids. Matching normalises a trailing number because the shipped template writes
  `[Verse 1]` while the profile declares `[Verse]`; a literal test would reject the model's own
  example. The check is load-bearing all the same: with the contract stated plainly in the
  system prompt, the recommended model put production cues (`[Vocal style: ethereal, airy]`)
  inside the lyrics in **every** capture. Evidence: [docs/MCP-SURFACE.md §15](docs/MCP-SURFACE.md).
- **2026-08-25 — one JSON file per lyric document.** `lyrics/<doc-id>.json` holds a whole
  `LyricDoc`, versions inline; `project.json` holds the ordered ids only. ARCHITECTURE 8's
  `lyrics/<v>.md` sketch predates the type and would split a version's text from its `source`,
  `created_at` and approval — the two-files-disagreeing hazard the one-source-of-truth rule
  exists to prevent, for a few KB. ARCHITECTURE 8 updated.
- **2026-08-25 — the lyric brief's `language` is a writing instruction, not a slot value.**
  The profile's `inputs.language` is `from_node_choices` and is read live from the node schema
  by Phase 3's param panel. Keeping the brief's language a plain string is what lets the Lyrics
  Studio render its form with **no running ComfyUI** — conflating the two would make writing
  lyrics depend on the audio service being up.
- **2026-08-25 — a prompt rule against a behaviour does not stop the behaviour, and the
  lyric prompt carries none.** The obvious fix for the model writing production cues into
  the lyrics (LLM-SURFACE 12.4) is a hard rule forbidding it. It was written, then measured
  over **14 live generations**: the runs carrying the rule averaged **more** stray direction
  blocks than the runs without it. Per-run counts ranged 0 to 10 on identical prompts, so
  the exact ordering is inside the noise -- but the rule never helped in any grouping, and
  naming the forbidden thing appears to prime it. The assembled prompt therefore stays the
  shape that was captured working, the profile's own `lyrics_contract` note stays (those are
  the profile author's words, not an instruction this app invented), and
  `test_no_rule_against_production_directions` exists purely to stop the rule being re-added
  on intuition. **The general rule for this repo: a prompt change is a change to a
  third-party surface, and gets measured like one.** Evidence:
  [docs/LLM-SURFACE.md 12.5](docs/LLM-SURFACE.md).
- **2026-08-25 — the model follows the requested section order, and usually adds an
  `[Outro]`.** Counted over all 13 saved generations: the requested `V-C-V-C-B-C` was a
  subsequence of the returned tags in **13 of 13**, an extra song section appeared in **9 of
  13** and was **always `[Outro]`**, and **none of the 99 declared tags** came back in a form
  other than the one the profile lists. Consequence for T-203: "the requested sections
  appear, in order" is a check a lyric can pass, "and nothing else" is one most lyrics fail
  over an outro the user probably wants, and **numbering tolerance is for the user's own
  text rather than the model's** -- the shipped template writes `[Verse 1]`, the model does
  not. *(Corrects an earlier phrasing of this entry that said every run added 2 to 4
  sections; that counted `[inst]` markers and tag occurrences rather than song sections.)*
- **2026-08-25 — a lyric line counts as structure when it *opens* with a bracket, not when
  it is only brackets.** The stricter rule was written first and looked obviously right.
  Run over the 13 saved generations, it reported one of them -- a correctly structured song,
  one of only three with no stray bracketed directions -- as having **no structure at all**,
  because that generation wrote every direction as `[Verse] (dreamy female vocals)`. The
  scanner now reads leading tags and reports the trailing text as its own finding. The
  general point, and the reason the fixtures in `testdata/lyrics/` are unedited model
  output: **a rule about model output has to be run against model output.** Hand-written
  fixtures are written to agree with the code.
- **2026-08-26 -- the optimizer rewrites the assembled brief, and the diff is the only
  enforcement.** What the user accepts is the *user message* -- the labelled lines
  `assemble_user_message` produces -- not the lyrics and not the form fields. The optimizer
  prompt splits those lines in two: Theme, Genre and style tags, Mood and Era and references
  may be rewritten; Structure, Language, Point of view, Explicit content allowed and Target
  duration must come back word for word, because they are settings the form owns and two of
  them feed the lint and the token budget. **No backend check enforces that**, deliberately: a
  model that rewrites a settings line produces a highlighted change the user has to accept
  before it goes anywhere, and a second gate would be a second answer to a question the
  consent step already answers. A test does assert the two lists cover every line the brief
  can emit, so a new field cannot reach the model with no rule at all.
- **2026-08-26 -- an accepted prompt is dropped the moment the brief changes.** The override
  was written against the brief as it read then; keeping it would leave the form describing
  one song while Generate sent another, and the form is what the user believes they are
  sending. Same reasoning as the never-modify-user-text rule, one level up: the request must
  match the thing on screen. Consequence: `setBrief` clears the optimizer state, and the way
  to a second rewrite is through Revert -- one rewrite in play at a time, so the text being
  replaced is always visible.
- **2026-08-26 -- `prompt_optimized` records the accepted prompt, not the optimizer run.** A
  rewrite the user reverted never reached the model, and a sidecar saying otherwise is a false
  record of how the lyric was made. The flag is therefore `promptOverride !== null` at commit
  time, not "the optimizer was opened".
- **2026-08-26 -- the optimizer prompt is unmeasured, and is marked as such in the code.** The
  lyric prompt in the same module was captured working before it was written down, and this
  repo's rule is that a prompt change is a change to a third-party surface and gets measured
  like one (LLM-SURFACE 12.5). This one is a first draft that has never met a model. T-211
  carries the measurement, and `create-core::lyrics::optimize`'s module docs say plainly that
  it is not a verified surface -- so nobody reads the confident wording as evidence.

- **2026-08-26 -- the remote-model disclosure stays in the wizard only** (owner, closing the
  question raised at T-211). Generating from the Lyrics Studio on a cloud model shows nothing
  about where the lyrics go; the wizard's "remote" chip and "Your lyrics leave this machine"
  at the point of choosing are judged sufficient. Recorded so a later session does not
  re-raise it as an oversight.
- **2026-08-26 -- the Gemma lyric recommendation is under review, and the reason is a
  confound.** The 2026-08-23 suggestion list came from the owner's hands-on use, but that use
  is on his music machine **with a system prompt he has tuned over hundreds of iterations** --
  which this app does not reproduce. Against latentCreate's own assembled prompt, unaided:
  `gemma4:12b-32k` wrote 8 lint findings on a generation where `qwen3.5:397b-cloud` wrote 0,
  and the owner's local `qwen3.5:9b` also wrote 0. That is the metric that matters here, since
  the app's premise is that the user does not have to tune a prompt. **The list is not changed
  yet** -- the dev box had most models removed days ago, so the owner is pulling a set and the
  comparison happens on real coverage. What is settled is that "Gemma is best for lyrics" was
  measured in a setting this app does not offer, and `data/lyric-llms.json` should not be
  treated as verified until it is re-measured. Tracked as OQ-7.
- **2026-08-27 -- `SaveAudioAdvanced.format` is set by graph edit, and the lossless test is
  the format value.** Closes the Phase 1 open item that blocked the pipeline. The dynamic
  combo is **not a slot**: `list_workflow_slots` does not surface it and `set_workflow_slot`
  rejects it with `[workflow_slot_invalid]` (loudly -- not one of 9.1's silent traps). It is a
  positional `widgets_values` entry, and **the array length varies by format**, so the write
  truncates rather than overwrites. `flac` is the **only** lossless option (no WAV), and it
  writes **16-bit/48 kHz with no bit-depth control** -- the UI must not offer 24-bit.
  Proven end to end: the ACE-Step turbo template's `SaveAudioMP3` retyped to
  `SaveAudioAdvanced`/`flac`, validated clean, run, and the output parsed as real FLAC
  (48 kHz, 16-bit, 10.000 s, compression ratio 0.471). **The MiniMax template ships
  `SaveAudioAdvanced` already set to `mp3`/`V0`**, so ARCHITECTURE 7's node-class test would
  have passed it and shipped MP3 into the mastering chain; the condition is now the format
  value. Evidence: [docs/MCP-SURFACE.md 16.1-16.3](docs/MCP-SURFACE.md).
- **2026-08-27 -- the app recommends no lyric model** (owner, closes OQ-7 by removing its
  premise). The suggestion list is dropped rather than re-measured. Owner's reasoning, in his
  terms: he is not here to promote models; the people using this app are already making music
  and **already have a go-to for lyrics**; and **assuming everyone runs Ollama is not
  logical** -- a lot of users are on APIs and larger models. What the app owes them is that
  **they can connect to whatever they already use**, not an opinion about which model to use.
  This reverses the 2026-08-23 recommendation decision (OQ-2) on better evidence about what
  the feature was actually for. Consequence: `data/lyric-llms.json`, the `suggestions.rs` in
  both `create-core` and `library`, the picker's preselect in `app/src/state/llm.ts` and
  `Setup.tsx`, and the docs/MODELS.md table all come out or change meaning -- Phase 2 code
  changed by a Phase 3-era decision, so it gets its own T-number rather than riding along.
  The `pull_command: "ollama pull ..."` field in that JSON is the clearest artifact of the
  assumption being removed.
- **2026-08-27 -- breadth of endpoint support beats depth on any one of them.** The direct
  consequence of the decision above: the measure of the LLM step is how many things a user
  can point it at, not how well it steers them. **Testing another API is not a blocker or
  something to wait on** (owner) -- so where a question about a provider comes up, it gets
  answered by connecting to one rather than by reasoning about it. Same standing offer for
  ComfyUI's own HTTP API and a cloud endpoint, **neither of which this repo has ever tested**;
  only local stdio comfy-mcp has been exercised.
- **2026-08-27 -- first real evidence for OQ-3 (raw ComfyUI API), and it is a point in
  favour.** Resolving the format question needed the option tree for a
  `COMFY_DYNAMICCOMBO_V3`. comfy-mcp's `nodes(action="get")` returns `choices: []` for it --
  the node registry cannot see inside a dynamic combo. A plain GET to the local
  `/object_info/SaveAudioAdvanced` returned the full nested structure (three formats, each
  with its own sub-inputs). That is precisely the "arbitrary node-input introspection"
  limitation OQ-3 was deferred against, now observed rather than hypothesised. It does not
  by itself justify a second backend -- one read-only lookup during verification is not a
  runtime dependency -- but it is the first entry in the evidence column, and the pipeline
  should be written knowing `/object_info` is available when the MCP surface cannot answer.

- **2026-08-27 -- `reasoning_effort: "none"` is honoured by a second, hosted provider, and
  the current rule is now measurably expensive.** T-302's measurement against **QwenCloud**
  (DashScope international, `qwen3.8-flash`), an endpoint the app cannot enrich and therefore
  never sends the field to: **first content 33.12 s -> 1.13 s, total 35.03 s -> 4.37 s,
  completion tokens 2771 -> 235**, both runs a complete song stopping cleanly. **On a hosted
  endpoint the rule costs credits, not only patience** -- 11.8x the billed completion tokens
  per generation, for a song no better. That is a different argument from the one the rule was
  written against on Ollama. **The rule is not changed yet**: two providers both honouring the
  field is not evidence that a third will not reject it, and sending an unsupported parameter
  everywhere would break lyric generation outright for that user. What the evidence supports
  is *discovering* acceptance per endpoint rather than inferring it from enrichment --
  proposed as T-302b. Evidence: [docs/LLM-SURFACE.md 13.1](docs/LLM-SURFACE.md).
- **2026-08-27 -- the both-spellings rule earned its keep on the first hosted endpoint.**
  QwenCloud streams **`reasoning_content`**, never `reasoning`. Every capture before today
  used Ollama's `reasoning`, so the 2026-08-24 decision to read both was written from
  documentation with no provider in hand that needed it. A client reading only `reasoning`
  would have decoded **none** of that run's 9419 reasoning characters -- not as an error, but
  as 33 seconds of an apparently empty stream, which is exactly the hang-versus-thinking
  confusion the proof-of-life decision exists to prevent. Recorded because this repo's habit
  is to delete defensive code that never fires; this one fired the first time it could.

- **2026-08-27 -- a graph edit that silently does nothing is the failure mode this app has to design against, and validation will not catch it.** Building T-305b's brief produced a LoRA splice with one plausible mistake -- loader nodes inserted, consumer link left sourced at the anchor -- and ran it live. `validate_workflow` returned `valid: true` with zero errors, the converted node count was identical to the correct splice, the job reported `completed` with `error: null`, and audio was written. **The user would have got a track with none of their LoRAs applied and nothing anywhere saying so.** ComfyUI prunes unreachable nodes without complaint. The standing consequence: **`validate_workflow` is a schema and enum check, not a reachability check**, and no part of the pipeline may treat a clean validation as evidence that an edit took effect. Reachability is asserted in `create-core`'s own tests, by traversing the edges, before a graph is ever submitted. Evidence: [docs/MCP-SURFACE.md 17.1](docs/MCP-SURFACE.md).
- **2026-08-27 -- a seed does not reproduce a track, and the app must not imply it does.** Two runs of the unmodified ACE-Step template with the same seed and sampling forced greedy differ in **98.1% of their bytes**. GPU reduction order is not deterministic and 58 sampling steps amplify it. This was found while trying to prove the LoRA splice worked by comparing audio -- a method that had to be abandoned. It constrains two things: **no test or manual check may rest on two runs matching**, and **`provenance` reproduces the inputs, not the waveform**. Wherever the UI shows a seed, it is a recipe, not a guarantee. Evidence: [docs/MCP-SURFACE.md 17.3](docs/MCP-SURFACE.md).
- **2026-08-27 -- `GET /history/<prompt_id>` is the project's answer to "what did the engine actually run".** It returns the API-format prompt as executed and was the only surface found that could tell a correct splice from a dangling one. comfy-mcp exposes no equivalent. Second entry in OQ-3's evidence column after `/object_info`, and the same shape: a read-only HTTP lookup that answers a question the MCP surface cannot. Still not a runtime dependency -- both are verification tools -- but the pipeline should be written knowing they exist.

- **2026-08-27 -- `applied` is not `effective`, and the shipped ACE-Step profile was proof.**
  Its `seed` wrote `94.seed` and `3.seed`; both are link-fed from `PrimitiveInt` 109, so
  `set_workflow_slot` reported them applied and the sampler read node 109 anyway. **Every track
  would have rendered with the template's seed while provenance recorded the user's choice** --
  a lie the app would have told about its own output, with no error anywhere. Fixed by pointing
  the input at `109.value`. The standing rule: **a slot write is only real if the target input
  is not driven by a node that survives conversion**, and the app checks rather than assumes
  (`create-core::audit_slots`). Note the nuance that makes a naive guard wrong -- a link from a
  frontend-only `PrimitiveNode` **is** dropped at conversion, so those writes do land, and
  flagging them would be a false alarm that gets the guard turned off.
  Evidence: [docs/MCP-SURFACE.md 18.1](docs/MCP-SURFACE.md).
- **2026-08-27 -- what `validate_workflow` is worth, stated precisely.** Measured, not assumed:
  it catches `unknown_enum_value`, `above_max` and `required_input_missing` before any GPU time.
  It is blind to reachability -- a LoRA chain feeding nothing passes (17.1) -- and documented
  blind to `COMFY_DYNAMICCOMBO_V3` sub-inputs, which is the save format. **Keep the step, and
  say in the code what it does and does not prove.** The phase file's earlier claim that it
  "catches a bad splice" was wrong in exactly the direction that matters and has been corrected.

- **2026-08-27 -- `local_check` is not a gate on the generation path, and the phase file said it
  was.** Found while briefing T-306b. `fetch_template` evaluates `local_check` against the
  template **as fetched**, before the profile's `slot_overrides` are applied -- and MiniMax
  Music 3 reports `runnable: false` over the one filename its own override corrects, with all
  three model files installed (MCP-SURFACE 14.4). A pipeline gating on it would refuse to
  generate with a fully working model, which is the same mistake the models step already
  refuses at length. The pipeline reads it nowhere; `validate_workflow` on the **edited** copy
  is what replaces it, because that is the first artefact that resembles what will be
  submitted. Same shape as the phase's other findings: the convenient signal was available,
  early, and about a different question than the one being asked.
- **2026-08-27 -- the mock MCP transport moves behind a `test-support` feature, because
  sequencing is what T-306b can get wrong.** Every `src-tauri` command until now made exactly
  one MCP call; the pipeline makes four in order against one file, and the failure modes -- an
  edit before the write it invalidates, a validate on a different path, an audit after the
  write it exists to prevent, a graph never written back -- are all invisible to tests over
  pure functions. `mcp-bridge::mock` already records every call sent, so it is exposed with
  `#[cfg(any(test, feature = "test-support"))]` and taken as a **dev**-dependency: Cargo does
  not build dev-dependencies for `cargo build`, verified by watching `cargo build -p app --lib`
  recompile `mcp-bridge` without the feature. The rule this sets: **a command that makes more
  than one call gets its call sequence asserted offline**, not left to the live milestone.
- **2026-08-27 -- the pipeline submits the lyric text it is given and never opens the lyric
  document.** `GenerationSpec` carries both the text (in `inputs`) and a `LyricRef`; resolving
  the ref would need a project slug the spec does not carry, and the Library view that owns
  that question is Phase 4. The gap -- one version's ref beside another version's text -- is
  closed at T-311 by ARCHITECTURE 8's existing requirement that the sidecar record the resolved
  slot values **actually submitted**, rather than the UI's account of them.

- **2026-08-27 -- a positive test is not optional where the negative one is cheap.** T-306b's
  briefed suite proved *"a bypassed LoRA is not spliced"* and had nothing asserting that an
  enabled one **is**. Two mutations pass that suite: dropping the splice entirely, and splicing
  into a throwaway clone -- the second leaves `lora_nodes` correctly populated in the result,
  raises no compiler warning, keeps the gate green, and submits a graph with none of the user's
  LoRAs in it. That is MCP-SURFACE 17.1 reproduced one layer up, in the code written to respect
  17.1. **Sixth consecutive task where an assertion of the form "nothing bad was found" passed
  because nothing was looked at**, and the first where the vacuous test was one I wrote into the
  brief myself. The standing addition: **where a test asserts an absence, the brief must also
  name the presence** -- and for anything that edits a graph, the assertion is read back out of
  the file that was submitted, never off the return value.

- **2026-08-28 -- A fixture the docs claimed for five tasks did not exist.** `tasks/phase-3.md` specified T-307 against "today's 53-entry list, captured into `testdata/mcp/`" from the day the phase opened. Nothing was ever captured; MCP-SURFACE 4, 12.2 and 16.5 describe the list in prose and `testdata/` held none of it. Captured now, verbatim, including the `stale: true` and `object_info_stale` warning that record it came from comfy-cli's cache with ComfyUI down -- provenance is part of a fixture, not noise to tidy off it. **Standing rule:** before briefing a task "against the captured X", open X. A phase file saying a fixture exists is a plan, not a file. Same failure mode as the T-306b `local_check` gate: the phase file was confidently wrong and nothing checked it.
- **2026-08-28 -- Real data can hide the bug most likely to be written.** The captured LoRA list contains `loragoth/final/`, and a run's `final/` supersedes every epoch checkpoint -- so on the real fixture the epoch number is never compared to anything, and comparing `checkpoint-epoch-N` as text (where 90 is the largest of {15 ... 300}) passes every test while offering a two-thirds-trained adapter as somebody's finished LoRA. The fix is not a synthetic fixture: it is the same real paths with one condition removed -- `final/` filtered out, which is what that directory looked like while it was still training. **Rule:** when a fixture's own completeness makes a comparison unreachable, derive the incomplete case from it rather than trusting the capture to exercise everything.
- **2026-08-28 -- Favourites and user LoRA names belong to T-309, not T-307.** The phase file put them in the pure function. They are persisted user state keyed on the entry path, they belong with the library store and the panel, and either one would put a second argument on a function whose whole value is taking one list and returning one catalog. Cosmetic label rules (stripping `ACE-Step-v1.5-` prefixes) go with them -- MCP-SURFACE 12.2 says those need the owner looking at the panel, so they wait for the panel.

- **2026-08-28 (owner) -- Aider is a token-saving device and nothing else.** Its only job is to keep the architect's context free so a session runs longer; it is not a review gate and adds no correctness. So work that is already written and verified does not go through it. T-306b and T-307 both came back **byte for byte identical** to their brief's reference -- two round trips that could not change the outcome, where all the value came from the architect's review afterwards. **The lane is decided before the brief is written:** architect-direct when the architect would pre-write and verify the whole thing anyway (small pure functions, where writing the reference is writing the task), Aider when the architect deliberately does not pre-write it (broad mechanical work, UI wiring, anything whose transcription would burn review context). The brief, the named invariants and the review pass stay identical in both lanes. WORKFLOW 1.
- **2026-08-28 (owner) -- Doc freshness is a written rule, and the existing one had a hole.** The standing rule checks docs against `git log` since the last session entry, which catches *drift* and can never catch a doc that was **wrong when written**. Two tasks in a row were specified against one: T-306b's `local_check` gate (would have made an installed MiniMax ungenerable) and T-307's captured fixture (never captured). New rule: **when a doc's claim is load-bearing for what you are about to write, check the claim against the repo** -- it names a file, open it; a test or behaviour, grep for it; a count, read the most recently dated section. Two corollaries: a doc line saying "trust the other doc if we disagree" is a defect that pre-authorises its own staleness, and every count gets the date it was true. WORKFLOW 6.

- **2026-08-28 -- A `u64` seed cannot survive JavaScript, so the panel refuses rather than rounds.** ACE-Step's seed runs to `u64::MAX` and `InputValue::Seed` exists so a seed cannot be demoted to another number type -- `generation.rs` pins `Seed(u64::MAX)`. JS has no such integer: above 2^53-1 the value changes as soon as it is a `number`, and `invoke` serialises via JSON so a `BigInt` cannot cross either. `18446744073709551615` would reach Rust as `...616`, be generated with, and be written into the provenance sidecar. The panel caps seeds at `Number.MAX_SAFE_INTEGER` and **refuses** anything larger with a message naming the limit. A refused seed is on screen; a clamped one is a sidecar that lies. Consequence: the app is capped below the model's range and the UI has to say so. Third instance of the same shape this phase -- **a guard in one layer does not bind the layer above it** (MCP-SURFACE 17.1 in the pipeline, the LoRA splice in T-306b, this in the panel).
- **2026-08-28 -- Control ordering is the panel's property, not the profile's.** `inputs` is a `BTreeMap` and arrives alphabetically, which puts bpm above the style tags and buries lyrics. A `PRESENTATION_ORDER` constant in `state/params.ts` orders the known names and appends unknown ones alphabetically. Rejected: a `display_order` field on the profile schema -- ordering is not a fact about the model, and it would be one more thing a custom-imported workflow (ARCHITECTURE 5b) has to get right to look correct.
- **2026-08-28 -- "ComfyUI is off" and "this model has no such input" must not look alike.** ACE-Step's `keyscale`, `timesignature` and `language` are `from_node_choices` with an **empty** local list, so they have no options until something asks the node registry; `negative` is `unsupported` with a reason recorded from a live node schema. The panel model reports both, distinctly -- `fromNode: true` with empty choices, and an `omitted` entry carrying its reason. Collapsing them is how a user concludes the app cannot see their install, and it throws away the only evidence that anyone checked.

- **2026-08-28 -- The profile names a node instance, not a node class, so live enum choices need a schema field.** `keyscale` declares `slots: ["94.keyscale"]` and `from_node_choices: true`. `94` is an instance id inside that profile's template; reading the live options means resolving 94 to `TextEncodeAceStepAudio1.5` first, and **the only place that hop exists is the workflow file**. Fetching a template to open a settings panel is an MCP round trip and a file write behind a UI affordance. Decision: **`InputSpec::Enum` gains an optional `node` field** carrying the class -- consistent with a schema that already names `save_node` and `loras.loader_node` -- and the input name keeps coming from the slot address's field part, so only the class was ever missing. Deferred to T-308c; T-308b ships the panel with those three controls in an honest "not loaded" state, which is what T-308a's `fromNode` flag was built for.
- **2026-08-28 -- A seed field must not be `<input type="number">`.** A number input coerces through a JS number, which is exactly the rounding T-308a refuses; the refusal is only real if the DOM never sees the value as a number. Text input, `inputMode="numeric"`, validated by `seedError`. Corroborated live the same day: `TextEncodeAceStepAudio1.5`'s `seed` reports `max: 18446744073709551615` from ComfyUI itself.

- **2026-08-28 -- A view type with nullable fields pushes dead branches into the view.** `Control` was one interface with `range: Range | null`, so `kind === 'int'` narrowed nothing and the panel could not write `min={control.range.min}` without a null check for a state that cannot occur. A discriminated union instead: "a numeric control always has bounds" becomes a fact the compiler knows rather than one every caller takes on trust, and the view gets exactly the fields its kind has. **Rule for the remaining panels (T-309, T-313):** if a view has to guard a field that is always present for the case it is rendering, the type is wrong, not the view.

- **2026-08-28 -- `nodes(action="get")` succeeds with ComfyUI down, and the wrapper was hiding it.** comfy-cli serves the class from its own `object_info` cache and flags it with `stale: true` plus an `object_info_stale` warning; `mcp-bridge` decoded neither, so every caller treated a cache as the installed truth. **A live read carries neither signal -- there is no `stale: false`** (observed by running the panel both ways). So `is_cached()` is `stale == Some(true) || the warning is present`, and absence is evidence rather than an assumption. The first cut read absence as "did not say, therefore not fresh" and **warned on every healthy install**, which is worse than not warning: a caution that is always on is one nobody reads by the time it matters. **Why any of it matters:** the same call enumerates LoRAs, where a cached list is a picker missing the LoRA the user finished training an hour ago. MCP-SURFACE 19.1. *(This entry originally also claimed a cached list offers deleted files and that picking one fails silently. Measured 2026-08-28 and wrong -- see the next entry.)*

- **2026-08-28 -- `validate_workflow` does catch a LoRA path the server does not know, and conflating that with 17.6 was my error.** Two spliced copies of the turbo template, one `LoraLoaderModelOnly` each, differing only in `lora_name`: a path no longer installed comes back **`valid: false`** with `code: unknown_enum_value` on the `lora_name` field, while `loragoth/final/training_state.pt` -- a genuine member of the 53-value enum -- comes back **`valid: true`** and applies nothing at run time. So the two failure modes are not one. **A deleted LoRA is loud and early**, caught by the pipeline's existing validation step before any GPU time; **a non-adapter is silent**, and validation cannot help because the value is legal. `create-core::loras` excluding non-adapters is therefore load-bearing in a way the stale-cache warning is not. Consequences: a stale list is a **short** list rather than a wrong one, so the LoRA panel's cache note says what is *missing* instead of cautioning about what is shown; and this is the first measured limit on 17.1's blindness -- validation ignores reachability but does check enum membership on the nodes a splice inserts, which is exactly where the LoRA path sits. Evidence: [docs/MCP-SURFACE.md 19.3](docs/MCP-SURFACE.md).

- **2026-08-28 -- LoRA labels stay mechanical** (owner, closes what T-307 deferred and MCP-SURFACE 12.2 asked for). Both documents declined to invent a cosmetic renaming rule on the grounds that it needed the owner looking at a real panel rather than at a list of strings. The panel now exists, and he looked: the labels are long but read fine in the menu and do not overflow. So no `ACE-Step-v1.5-` stripping and no trailing `-LoRA` trim, even though both are one line -- a rule that improves these twelve could mangle a naming scheme nobody has seen. **Consequence for T-309c:** user-defined display names were the escape hatch for labels the owner would find unusable, and he does not, so half that task's premise is gone; favourites over twelve entries is thin on its own. Recommend it goes to the backlog rather than the phase. The general point, third time this phase: *the question a doc defers to a person has to be asked against the thing, not against a description of it.*

- **2026-08-28 -- a submitter must register its job with the frontend store, and two correct layers were deaf together.** `generate_audio` starts the job pump itself rather than going through `run_workflow`; the frontend jobs store learns of a job only in `useJobsStore.run()`, which that path does not call; and `applyJobEvent` ignores events for ids it does not know -- which it must, or a foreign job would invent an entry. Each half is right on its own. Together they meant Generate would run a full generation on the GPU while **every progress, done and failed event was discarded**: an empty queue panel, no error, nothing in the log. Fixed with `useJobsStore.register(id)`, and the test drives a real event through the reducer afterwards rather than only asserting the id is in the map -- those are different claims. Fifth instance this phase of *a guard in one layer does not bind the layer above it*, and the first where neither layer is individually wrong; the general lesson is that **the dangerous seam is where two correct components meet**, which is exactly where unit tests do not look. Accepted gap: the pump can emit before `register` runs, so an early status can be missed -- terminal events always follow, so nothing hangs, and closing it means a new `job://queued` from the backend for a cosmetic gain.
- **2026-08-28 -- the `LyricRef` is attached only when the submitted text *is* the approved version's, byte for byte.** `GenerationSpec` carries the lyric text in `inputs` and a ref beside it and nothing downstream reconciles the two; PROJECT.md deferred that gap to T-311's sidecar on 2026-08-27. It closes cheaply at assembly instead, because a ref naming v2 next to v3's words is wrong in the one way provenance must never be wrong, and T-311's acceptance bar is that a run **reproduces from the sidecar alone**. Someone who pastes the approved lyric and changes a word has a different lyric and gets no ref. The lyrics and seed controls are found by **kind**, not by the names `lyrics` and `seed`, so a custom-imported workflow (ARCHITECTURE 5b) does not silently stop recording lyric provenance. Related: **ComfyUI being disconnected is deliberately not a blocker** -- `generate_audio` calls `ensure_connected`, which starts comfy-mcp itself, so gating on connection state would leave Generate dead on every cold start; a test pins that by arity so nobody adds the argument back.

## Open questions (owner to decide)
- ~~**OQ-7 -- which model should `data/lyric-llms.json` suggest for lyrics?**~~ **RESOLVED
  2026-08-27 -- the question is withdrawn, not answered.** The owner's decision is that the
  app should suggest **no** lyric model: users bring their own, many are on APIs rather than
  Ollama, and the app's job is connecting to whatever they already use. The comparison that
  was pending (gemma vs qwen against this app's unaided prompt) is therefore moot for
  shipping purposes -- though it stands as the reason the old recommendation was not
  trustworthy. For the record, the owner's finding: **local qwen is the best he has tested
  out of the box**, with no prompt adjustment. See the decisions-log entry above for the
  files this touches.

- ~~**OQ-6 MiniMax Music 3 profile**~~ — **RESOLVED 2026-08-23.** Owner installed the int8 weights (all three files). The template still fails `local_check` on one line because it hardcodes the **fp16** DiT filename; overriding `37/6.unet_name` makes `validate_workflow` return clean — verified end to end. The profile can be written in Phase 1 without further setup; the fp16 DiT is optional and only for a quality comparison. Superseded detail below kept for context: *(original)* The native template `audio_minimax_music_3` exists and is free/local, but the three model files are not on the main dev box (which has MiniMax **H3**, the video model, instead). **Owner confirmed 2026-08-23:** the Music 3 testing was done on the other PC, and this box is his model-testing machine where new models are installed to try and then removed — so absent weights here mean nothing about the model. Options: install the weights here when the profile is written (multi-GB, owner's call), author it on the other PC, or defer to Phase 3. Update ComfyUI first regardless — core is one release behind and the template threw V3 type warnings consistent with template-newer-than-install.
  - **Standing implication for agents:** never infer "model unsupported/unavailable" from this machine's installed-model list. It is a testing box whose model set churns. Ask, or check the template rather than the weights.
- **OQ-3 Raw ComfyUI API fallback.** Build a second `ComfyBackend` impl against `/prompt`+websocket if comfy-mcp proves limiting (e.g. arbitrary node-input introspection)? Deferred until Phase 3 evidence exists.
- **OQ-5 App identity — parked, do not force a decision.** `latentbeats.com` is the umbrella for the whole suite; "latentCreate" is the working name and is fine to ship in docs/UI for now. Final product name comes out of a dedicated brainstorming session the owner will schedule. **Agents: do not propose or apply branding changes unprompted**; keep the name in a small number of places (README title, `package.json`/`tauri.conf.json` product name, window title) so a later rename is cheap.

*Resolved: OQ-1 (Apache-2.0), OQ-2 (lyric-LLM guidance), OQ-4 (send-to owned by mixing/mastering) — all in the decisions log above.*

- **OQ: is ACE-Step 1.5 XL Turbo's `vram_gb_min: 8` right?** **Still open, and now the oldest
  unanswered question in the repo.** The profile says 8 GiB; the XL turbo DiT alone is 9.3 GiB
  and the full set is 18.5 GiB, so the figure looks wrong. T-113 did not settle it: the
  milestone never required a *generation*, only that the wizard reach "ready". It cannot be
  settled by argument — it needs one real run on the 15.9 GiB card, which is Phase 3's first
  chance. Until then the number stays as written rather than swapping one guess for another.
- ~~**OQ: `download` status `"completed"` is still inferred.**~~ **Settled 2026-08-25** by a real
  18.5 GiB ACE-Step install: `starting` -> `downloading` -> `completed`, and nothing else across
  four concurrent downloads. `isTerminal` is correct as written. Also settled: freshly downloaded
  files appear to `search_models` with **no ComfyUI restart**, so the post-install re-check is
  enough.
## Backlog (accepted, not yet scheduled)

- **Verify MiniMax's seed mapping the way ACE-Step's was verified.** All three addresses its
  profile names are link-fed (MCP-SURFACE 18.5); `audit_slots` reports them `unchecked` because
  they are subgraph interiors. Needs one MiniMax generation and a read of
  `GET /history/<prompt_id>`. Until then MiniMax's seed is unverified, not working.
- **Give `InputSpec::Seed` a declared range.** The ACE-Step template's seed goes through a
  `PrimitiveInt` capped at `i64::MAX`, while `KSampler.seed` accepts the full `u64`
  (MCP-SURFACE 18.4). Nothing in the profile schema can express that today; validation catches
  an over-range seed, so this is a UI-quality issue rather than a correctness one.

- **Click the Install button once, on a machine missing a model.** `models_install` and
  `models_progress` are the only Tauri commands in the wizard never exercised through the UI:
  the 18.5 GiB install ran the same `download_model` calls they wrap, but the click-through
  happened afterwards, with the models already present, so the button was never offered. The
  wrappers are covered by unit tests and by construction, not by a click.
- **Enforce the ASCII rule in `npm run gate` and CI**, rather than in prose. It has cost a
  review round on five consecutive tasks; executors have been right about it every single time,
  and a sweep at T-111 found five pre-existing violations that earlier reviews had missed. The
  rule is "ASCII in code and comments; UI strings may use Unicode", so the check must exempt
  rendered strings — `app/src/views/CoverArt.tsx` holds a legitimate one.
- **Styling debt lives in [docs/CSS-TODO.md](docs/CSS-TODO.md)**, not here -- presentation
  gaps found while clicking through a feature, written down at the moment they are noticed
  so the Phase 5 polish pass is not a rediscovery exercise. Currently: the model list's
  unstyled scrollbar, and the streamed-reasoning panel.
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

### 2026-08-23 (later still) — T-101 landed

Aider transcribed the brief faithfully: the diff touched only the listed files, and the one
gate failure was `cargo fmt` on my hand-formatted reference code — the same outcome Phase 0
recorded for every reference-code brief. Review then found **three things the brief got
wrong or left unowned**, none of them the executor's doing:

1. **`parse_error_code` scanned for the first `[`, but comfy-mcp puts the workflow path
   ahead of the slug.** A file under `my [demo] songs/` parsed as the code `demo`;
   `My [Demo] Songs/` swallowed the slug entirely. Fixed post-Aider by anchoring on the
   literal ` failed [`, with a test carrying that path. Nothing branches on `code` yet, but
   T-110 will map it to a remedy, so a wrong slug would have sent users to the wrong fix.
2. **T-101's flagship invariant shipped untested.** `call()`'s `Ok(is_error: true)` branch
   needs a transport to exercise, so it could not be covered in T-101 — it is now the first
   thing T-102's mock rig owes, written into that task.
3. **Nothing in any phase file owned the session log.** ARCHITECTURE §3 requires redacted
   tool-call logging and CONVENTIONS requires the child's stderr in it, but no T-number
   claimed either, and `TokioChildProcess::new` silently discards stderr. Now **T-102b**.

**Carry forward:** reference code in a brief must be run through `cargo fmt` before it ships,
not just clippy. And the review question that earned its keep here was not "does this match
the brief" — it did — but *"what did the brief fail to ask for?"* All three findings were
mine, upstream of the executor.

### 2026-08-24 — T-102 landed; the rig can now see requests

Aider transcribed the brief exactly; `cargo fmt` was again the only gate failure, now the
third run in a row, so WORKFLOW §1 gained a hard rule rather than another note: **reference
code goes through `cargo fmt` before it ships in a brief.** Compiling it is not enough — the
executor copies hand-formatting faithfully.

Review found one gap, again in the brief rather than the run. **The rig could verify
responses but not requests.** Every test served canned data and checked the decode; nothing
observed what the bridge *sent*. comfy-mcp rejects a misnamed argument outright — `path`
where it wants `workflow_path` (docs/MCP-SURFACE.md §8.7, proven live) — so a T-103 wrapper
misspelling one would pass the entire offline suite and fail only against a real server,
which is the failure this rig exists to prevent. `spawn_mock` now returns a `RecordedCalls`
log, with a test asserting the tool name and argument names go out verbatim; mutating `call`
to rename `workflow_path` → `path` fails that test and no other.

**Carry forward:** mutation-testing the two or three tests a task actually turns on has now
paid three times — it caught a test that passed for the wrong reason in T-102's own brief,
and confirmed both `is_error` guards and this one. It costs one edit and one `cargo test`.

### 2026-08-24 — T-103 split and T-103a landed

T-103's six tools were captured live in one pass (**MCP-SURFACE §9**) and the task split in
two, since six wrappers exceeds the ~400-line rule. T-103a (templates) is in; T-103b (slots,
writes, validation, notes) is fully researched and briefable without further live work.

**The fmt rule from T-102 held.** The reference code I shipped was `cargo fmt`-clean and the
run's only formatting failure was in a test body Aider composed itself — a much smaller
surface than the three prior runs.

Review found two gaps, both mine:

1. **`LocalCheck` did not survive its own round trip.** It reads comfy-mcp's
   `checked`/`runnable` but serialises a `state` tag for the frontend, so
   `Some(true)` → serialise → deserialise gave **`None`, silently** — the tri-state
   misreporting its own output, which is the exact failure it exists to prevent. Latent
   today, but CONVENTIONS requires boundary types to round-trip their fixtures (the T-004b
   pattern) and T-110 will mirror this one in TypeScript. `RawLocalCheck` now accepts both
   shapes, with a test across all three arms.
2. **`get_template` and `TemplateDetail` shipped untested** — my brief listed eight tests and
   none touched them. Covered now, including that a `get_template` row has no `api` field and
   must default rather than fail the decode.

**Carry forward:** when a type deserialises from one shape and serialises to another — which
`#[serde(from = ...)]` plus `#[serde(tag = ...)]` guarantees — assume it does not round-trip
until a test says otherwise. And check the brief's own test list against the public surface
it added: both times now, the untested thing was the one the test list simply forgot.

### 2026-08-24 — T-103b landed; two process failures of mine, both fixed

T-103 split again — T-103b is slots and writes, T-103c is validation and notes, still
unbriefed but fully researched (§9.2, §9.3, §9.6 need no more live capture).

`set_slots` sends `stdout: false` and structured overrides, and **verifies its own write**,
because both ways it can fail look like success in the payload. All three guards were
mutation-tested: flipping `stdout` to `true`, dropping the verification, and swapping in the
coercing string override form each fail exactly their own test and nothing else. Coverage
was complete first time — the test-list-versus-public-surface check from T-103a worked.

One design decision got checked against reality rather than assumed: `set_slots` errors when
an address is missing from `applied`, which would be wrong if comfy-mcp reported only
*changed* values. It does not — re-sending two addresses at the values they already held
returned both in `applied` (§9.1). That matters because the app sends the whole parameter set
whenever the user edits one field, so most addresses in a real write are no-ops.

**Two mistakes of mine this round, both worth not repeating:**

1. **The T-103b launch command omitted `--read` for `error.rs` and `local.rs`**, though the
   reference code constructs `ComfyError` variants and `impl`s on `LocalComfy`. Aider stopped
   and asked for one — the footer rule working. Accepting that prompt would have added the
   file as **editable**, widening the diff past the brief; the fix belonged in the launch
   command. Now a rule in WORKFLOW §3.
2. **I committed the aborted run's partial `slots.rs`**, non-compiling, under a docs message —
   `git add -A` swept it in. Worse, the commit was chained as
   `npm run gate | head -4 && git add -A && git commit`, and `head` exits 0 whatever the gate
   did, so the `&&` was gating on `head` rather than on the build. Undone with a soft reset
   (it was unpushed). **Gate runs now capture the exit code explicitly**
   (`npm run gate > log 2>&1; echo $?`) instead of being piped into anything.

**Carry forward:** never pipe the gate into `head`/`grep` in the same chain as a commit — the
pipeline's exit status stops being the build's. And `git add -A` is wrong whenever an
executor run was interrupted; stage the intended paths.

### 2026-08-24 — T-103c briefed; handoff point

[T-103c's brief](tasks/t-103c-brief.md) is written and ready to run. Its reference code
compiles, is `cargo fmt`- and clippy-clean, and its verdict logic was exercised across all
four cases first. It closes the T-103 split.

**Where `mcp-bridge` stands.** It can connect to `comfy-mcp` over stdio, call any tool, and
decode templates and slots into typed results — 33 tests, none needing a live server. Nothing
is wired to Tauri or the UI yet; that starts at T-104.

**What a new session most needs to know**, beyond the read order in AGENTS.md:

- **docs/MCP-SURFACE.md is the authority for anything comfy-mcp.** §8 is the Rust client
  (rmcp), §9 the template/slot surface. Both were captured by running the real server. The
  cloud documentation names different tools and is not a guide. Do not brief against memory
  or model cards — the standing rule that has now paid off in every single task.
- **Three traps on this surface are silent**, and each is guarded in code with a test that was
  mutation-checked: a failing tool call returns `Ok` with `is_error: true` (§8.3);
  `set_workflow_slot` does not write unless told to (§9.1); `validate_workflow` can report
  `valid: true` having examined nothing (§9.3). Assume the next tool has one too.
- **The review question that keeps finding things is not "does this match the brief"** — it
  has matched every time — but *"what did the brief fail to ask for?"* Four of the five
  review findings so far were defects in the brief, upstream of the executor. Check the
  brief's test list against the public surface it added.
- **Mutation-test the two or three tests a task turns on.** One edit, one `cargo test`. It has
  caught a vacuous test inside a brief, and confirmed six guards since.
- **Unbriefed and unowned:** T-102b (session log + child stderr — ARCHITECTURE §3 requires it,
  no task claimed it until the T-101 review) and everything from T-104 on.

### 2026-08-24 — T-103c landed; the template/slot surface is complete

Aider transcribed the brief faithfully and the diff touched only the two listed files
(`preflight.rs` new, `lib.rs` +2). The run's single gate failure was **two unused imports**
(`Value`, `NoteList`) in the test module — the executor imported types the tests never
reference, since `json!(...)` and `serde_json::from_value` infer them. Fixed directly; this is
the executor-imports-it-doesn't-use variant of the recurring formatting defect, smaller than
the prior three runs' single-issue failures.

Review did its own work this round rather than the brief's: **mutation-tested three guards**,
each failing exactly its own test and nothing else —
1. `Verdict::Vacuous` requires **both** `converted_from_ui.is_none()` and a `non_node_key`
   warning. Dropping the second condition fails `test_verdict_is_valid_for_an_api_format_graph`
   (the false-positive guard), confirming the brief's explicit warning that requiring only the
   missing field would condemn every API-format workflow — which is the format the app's own
   pipeline produces.
2. `node_id_to_instance` translating `:`→`/`. A no-op translation fails
   `test_node_id_translates_to_a_slot_instance` only.
3. `Validation::spends_credits` field presence. Removing the field is a **compile error** at
   the test's assertion site — stronger than a runtime failure: the product rule (T-104 gates
   running on this) cannot be silently dropped.

The untrusted-note boundary needs no mutation: both wrappers are pure `self.call(...)`, so
there is no text processing to break; the byte-identical `text == text` assertion guards the
verbatim-relay contract by construction. 43 tests in `mcp-bridge` now, none needing a live
server.

**Where `mcp-bridge` stands.** The template/slot surface is complete: connect, call any tool,
and decode templates, slots, validation verdicts, and notes into typed results. Nothing is
wired to Tauri or the UI yet; that starts at T-104.

**Carry forward:**
- The T-103 split paid off — three sub-tasks each landed with a single small, fixable defect
  rather than one ~1200-line diff. Keep splitting any task over the ~400-line rule.
- **T-102b and T-104 are the next two briefs to write.** T-104 is where the deferred
  `ComfyBackend` trait decision comes due (async fns in traits aren't object-safe; decide
  dyn-vs-enum when a backend first enters Tauri managed state, not around a single impl).
  T-102b is smaller and unblocks packaged-build diagnostics: `LocalComfy::connect` still
  inherits stderr, so comfy-mcp's output vanishes in a shipped app.

### 2026-08-24 — T-102b briefed (session log + redaction), split from stderr capture

Wrote [tasks/t-102b-brief.md](tasks/t-102b-brief.md). The original T-102b ("session log +
child stderr") came to ~565 lines of diff once the full reference code was written, so it is
split the T-103 way: **T-102b** delivers the `SessionLog` (rotating NDJSON) and structural
`redact`, wired into `LocalComfy::call`; **T-102c** delivers stderr capture and free-text
redaction (`redact_line`). `tasks/phase-1.md` records both.

**Verified the way the brief's rule requires, not from memory.** The whole thing was built in a
throwaway crate outside the repo and compiled against the *actual* rmcp 3.1.4 source: 20 tests
pass, `cargo fmt`-clean, `clippy -D warnings`-clean. Two facts worth carrying forward:

1. **`TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()` returns
   `(TokioChildProcess, Option<ChildStderr>)`** — confirmed against `child_process.rs`, not the
   MCP-SURFACE note alone. The stderr half (T-102c) is now safe to brief without re-verifying.
2. **Redaction is two layers, and only one ships here.** `redact` is *structural*: it walks JSON
   and replaces values under secret-*named* keys (`api_key`, `token`, …), matched on whole words
   so a real control name like `keyscale` is never hit. That is the layer with a crisp,
   testable invariant ("a secret argument never reaches the file"). Free-text redaction for
   stderr is T-102c. **Documented residual:** a secret embedded in free text or split across an
   array (ComfyUI's `system_stats` `argv`) is not caught by structural redaction — surfaced in
   the brief as a known limitation, mitigation upstream (T-110 launches ComfyUI without secrets
   on argv).

**Rotation pitfall caught by compiling on Windows, not by reading:** `fs::rename` does not
overwrite an existing destination on Windows, so a second rollover would silently keep
appending to the current file while resetting the byte counter. The log removes the previous
`.1` before renaming — the one previous generation is the whole rotation scheme, documented in
the brief.

### 2026-08-24 — T-102b landed

Aider transcribed the brief faithfully; the diff touched exactly the four listed files and the
executor's one hand-written test was *stronger* than my reference (it asserts both the `call`
and `result` entries plus the `ok` flag, not just the secret's absence). `mcp-bridge` is now 49
tests, all offline.

**The single gate failure was mine, not the executor's.** `cargo fmt --check` rejected
`from_transport_with_log`'s `().serve(transport).await.map_err(...)` chain, which I had written
multi-line in the brief's reference code. The scratch crate was fmt-clean; I re-typed the brief
from an earlier draft instead of copying the formatted file, so the rule "reference code goes
through `cargo fmt` before it ships in a brief" was followed for the crate and violated for the
brief itself. Fixed with a one-line collapse. The rule's real lesson: **copy the reference code
out of the fmt-clean scratch file verbatim — do not re-type it.**

**Mutation-tested the guard that matters:** deleting `redact(...)` from `log_call` fails
`test_call_logs_call_and_result_to_the_session_log` on `!raw.contains("sk-secret")` and nothing
else — the "a secret never reaches the file" invariant is genuinely enforced, not asserted.

**One branch remains untested, noted for later:** `call`'s transport-error path
(`call_tool` returning `Err`) logs `log_result(tool, false, …)` but no test triggers a transport
error — the mock always answers. It is a best-effort log line (the error still propagates
correctly via `Err(e.into())`), so it is low-risk, but it is the only part of the new surface
without a test. Fold a transport-abort mock case into T-102c if convenient, or leave it — it
would only lose a diagnostic line, never change behavior.

### 2026-08-24 — T-102c briefed (stderr capture + free-text redaction)

Wrote [tasks/t-102c-brief.md](tasks/t-102c-brief.md), closing the T-102b split. It captures
`comfy-mcp`'s stderr via `TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()`, drains
it on a cancellable task, and adds `redact_line` for free text. It also **folds in the
transport-abort mock case** flagged at the T-102b review.

**The new mock case was built and run, not assumed — and it was the one thing I had not verified
before this session.** `Reply::Hangup` closes the duplex without answering; I confirmed in the
throwaway crate that this makes rmcp's `call_tool` return `Err` (a `ServiceError` →
`ComfyError::Transport`) rather than hang, and that `call` logs it as `ok: false`. So the one
untested branch from T-102b is now covered by a test that fails if the `log_result(false, …)`
line in `call` is deleted. 26 tests in the scratch crate, `cargo fmt`- and clippy-clean.

**Carry forward:** the T-102b fmt lesson held — this brief's reference code was copied verbatim
from the fmt-clean scratch file (the `spawn_stderr_drain` signature is wrapped the way `cargo fmt`
leaves it), so the recurring "reference code wasn't fmt-clean" defect should not recur here.

### 2026-08-24 — T-102c landed; and I broke my own fmt rule, again

Aider transcribed the brief faithfully — the diff touched exactly the four listed files and its
hand-written `redact_line`/drain tests were again stronger than my reference (they assert the full
redacted output, not just the secret's absence). `mcp-bridge` is now 55 tests, all offline.

**The same fmt defect I recorded one task ago came back, and this time it was three places in my
own reference code, not one.** `cargo fmt --check` rejected the `connect` builder chain, the
`().serve(...)` chain in `from_transport_with_log`, and the mock's `let reply = …` line — all of
which I had re-typed multi-line in the brief after running `cargo fmt` on the scratch. The scratch
was fmt-clean; the *brief* was not. The prior session's lesson ("copy from the fmt-clean scratch
file verbatim") was written down and I still didn't follow it: I reconstruct reference code from
my `write`-tool drafts, which are pre-`cargo fmt`. The fix is procedural, not a reminder: after
`cargo fmt` in the scratch, **re-read the files and paste from that**, never from memory or the
draft. A fourth fmt defect in the executor's *own* test body (`test_log_stderr…`'s long
`assert_eq!`) is the smaller, already-seen variant — the executor composing test code.

**Mutation-tested the flagship guard, now in the landed tree:** deleting the
`log_result(false, …)` line in `call`'s transport-error branch fails
`test_transport_error_is_logged_as_a_failed_result` on `entries.len() == 2` and nothing else — the
T-102b gap is genuinely closed, not just covered by a test that happens to pass.

**Two things the brief correctly left untested, both by design:** `connect`'s real child spawn
(the no-live-process rule; `drain_stderr` itself is exercised via a `duplex`), and `shutdown`'s
abort of the drain task (needs a real child). Both are live-only; the drain also self-terminates
via EOF on drop, so the abort is belt-and-suspenders, not the only safety net.

### 2026-08-24 — T-104 planned: trait re-deferred, job surface half-captured

Planned T-104 (job lifecycle + event pump) and surfaced the two decisions it hinges on. Owner
confirmed both: **defer the `ComfyBackend` trait again**, and **capture the run/job/fetch shapes
via fast-fail** — which turned out to be only half-possible, so the rest is deferred to a
"Before T-104a" step.

**Trait re-deferred.** Recorded in the decisions log and ARCHITECTURE §3 (now marked a sketch).
The concrete reasons, in order of weight: still one impl; the ARCHITECTURE sketch has drifted from
landed methods; MCP-SURFACE §1 proves local/cloud are different surfaces, so the eventual seam is
more likely `enum Backend { Local, Cloud }` than a 17-method trait. Tauri managed state will hold
`Arc<LocalComfy>` concretely.

**Live capture, the error half.** Zero-cost against the running server (comfy-cli 1.16.0, ComfyUI
v0.33.3): argument names (`workflow_path`/`wait`, `prompt_id`, `out_dir`) and error slugs
(`workflow_not_found`, `workflow_unknown_nodes`, `prompt_not_found`, `download_job_not_found`) plus
`job(action="queue")`'s `{host,port,where,scope,count,jobs[]}`. Recorded in **MCP-SURFACE §10**.

**The one finding that reshapes T-104a: `run_workflow` pre-validates.** A workflow with a missing
checkpoint and no output node was rejected with `[workflow_unknown_nodes]` *before* queueing — so
a fast-fail never yields a `prompt_id`, and the run wrapper's error granularity comes from
comfy-cli, not `/prompt`. (This also makes ARCHITECTURE §7 step 4's pre-submit `validate_workflow`
partly redundant for the run path.)

**The success half is genuinely blocked here:** this box has no image checkpoint (`checkpoints`
empty — only ACE-Step/MiniMax music/video models), so capturing `run_workflow`'s `prompt_id`
envelope, `job(status)` running/success, and `fetch_outputs` with files needs a real music
generation or a small image checkpoint install. Deferred to "Before T-104a" in phase-1.md, with
the two ways to satisfy it named.

**Carry forward:** "fast-fail capture" presumed the failure happened at execution, where a
`prompt_id` would still exist. It happened at validation instead — a reminder that the shape of a
tool's failure is itself part of the surface to verify, not assume.

### 2026-08-24 — success shapes captured; T-104a briefed

Owner green-lit the capture; I ran a real short ACE-Step 1.5 turbo generation (duration set to
10 s via `set_workflow_slot`) and captured the full run→poll→cancel→fetch path with an actual MP3.
Recorded in **MCP-SURFACE §10.3–10.6**, and [tasks/t-104a-brief.md](tasks/t-104a-brief.md) written
with the shapes folded in. Verified the reference code in the throwaway crate (35 tests, 9 new,
`cargo fmt`/clippy-clean) — and this time copied it verbatim from the post-`cargo fmt` file, so the
recurring fmt defect should not recur.

Three shapes worth carrying forward, each the kind of thing a model card would get wrong:

1. **Terminal status is `"completed"`, not `"success"`.** Also, the status shape carries **no
   `progress`/`total` number** on comfy-cli 1.16.0 — T-104b's pump polls `status` + `outputs`, not
   a percentage.
2. **`run_workflow`'s result is an envelope**, not a bare id: `{workflow, status:"queued",
   prompt_id, client_id, outputs, elapsed_seconds, host, port, state_file, watcher_spawned}`.
   `prompt_id` is the handle; `state_file` is what `fetch_outputs` reads back.
3. **`job(action="cancel")` is racy** — with the model cached, a second run completed before the
   cancel landed, so there is no `"cancelled"` status to rely on; the app reads `found`/
   `queue_delete_ok`/`interrupt_ok`. And the *failure* shape (`error` non-null) was not reproduced
   — `JobStatus.error` is `Option<Value>` and `is_terminal` marks `"error"`/`"failed"` as inferred.

`mcp-bridge` now spans the whole comfy-mcp surface from health through run/job/fetch; T-104b (the
Tauri pump) is the last wiring step before generation reaches the UI.

### 2026-08-24 — T-104a landed

Aider transcribed the brief exactly; the diff touched only the two listed files and the fmt rule
finally held — the reference code was copied from the post-`cargo fmt` scratch file, so this was
the first run in four with **no** fmt defect. `mcp-bridge` is now 64 tests, all offline.

The one transcription difference is an improvement, not a defect: the executor dropped the
non-ASCII `§` from my doc comments (`"ARCHITECTURE §3"` → `"ARCHITECTURE 3"`, `"MCP-SURFACE §10.4"`
→ `"MCP-SURFACE 10.4"`), which matches the crate's existing comment style and the ASCII-in-comments
rule better than my reference did.

**Mutation-tested the headline guard:** rewriting `is_success` to key on `"success"` instead of
`"completed"` fails `test_completed_is_terminal_and_success` on `assertion failed:
status.is_success()` and nothing else — the `"completed"`-not-`"success"` finding is enforced in
the landed tree, not just asserted. This matters precisely because the live capture is the only
place that fact is recorded; a pump written later from memory would key on `"success"`.

**No new "what did the brief fail to ask for" findings.** The two honest gaps are the ones the
brief already carried and encoded: the failure shape (`error` non-null) is `Option<Value>` with
`"error"`/`"failed"` inferred, and `job(action="wait"|"watch")` is deliberately unwrapped (T-104b
decides how the pump blocks). `mcp-bridge` is feature-complete for the whole comfy-mcp surface;
T-104b wires it to Tauri.

### 2026-08-24 — T-104b briefed (Tauri managed state + event pump)

Wrote [tasks/t-104b-brief.md](tasks/t-104b-brief.md). `src-tauri` gains a `ComfyState` managed
state (`Arc<LocalComfy>` + a map of active pump abort-handles), `connect_comfy` / `run_workflow` /
`cancel_job` commands, and a `poll_until_terminal` pump that re-emits `job://progress|done|failed`.
Frontend bridge + jobs store + queue panel is a follow-up, not this brief.

**Verified the tauri 2.11 surface from source, not memory** (the CONVENTIONS rule, first time it
applies to tauri rather than rmcp/comfy-mcp): `Emitter::emit<S: Serialize + Clone>`,
`async_runtime::spawn -> JoinHandle` (with `.inner().abort_handle()` for the stored
`tokio::task::AbortHandle` and `.abort()`), and `#[tauri::command]`'s injected `AppHandle`/`State`
params. The reference code compiles against the real `tauri` + `mcp-bridge` in a throwaway crate,
6 tests pass, `cargo fmt`/clippy-clean.

**Two design points worth carrying forward:**

1. **The pump is a pure function, the Tauri glue is thin.** `poll_until_terminal` takes the
   status source and the emit sink as closures, so the meaningful logic (loop, terminal-vs-progress
   split, error propagation) is unit-tested with a canned status sequence — no live ComfyUI, no
   window. `terminal_outcome` maps the terminal result to done/failed and is pure too. The
   `monitor_job`/command layer is just `app.emit` + `spawn` + a registry, verified by compilation.
2. **`poll` takes `String`, not `&str`.** An owned id lets the `async move` closure be `'static`,
   which `async_runtime::spawn` requires — the borrow version would not compile. This is the kind
   of thing that reads as a nitpick in a brief and is a hard compile error in the run.

### 2026-08-24 — T-104b landed

Aider transcribed the brief exactly; the diff touched only the three listed files (plus the
`Cargo.lock` tokio entry). `src-tauri` now has 7 tests (6 new), fmt clean — the fourth consecutive
run with no fmt defect.

**Mutation-tested the pump guard that matters:** swapping `on_update` ahead of the terminal check
makes `test_poll_emits_non_terminal_and_returns_terminal` fail — the terminal status is genuinely
*not* emitted as progress, so a job can't be double-reported (once as `job://progress`, once as
`job://done`). The `terminal_outcome` mapping (completed→done, error→failed, poll-error→failed) is
pure and its three tests cover every arm.

**No new "what did the brief fail to ask for" findings.** The one thing the brief scoped out — the
frontend bridge + jobs store + queue panel that consumes these `job://*` events — is now flagged in
the snapshot as **unassigned and in need of a T-number** (the T-102b pattern: a requirement no task
owns). The `outputs` method (T-104a) is also not yet called by anything; the §7 pipeline downloads
outputs at completion, which is T-107+ territory.

**A process note on my own review:** I used a PowerShell `-replace` with `\n` inside single quotes
to revert a mutation, which does not interpret `\n`, and briefly dropped the terminal check before
the `edit` tool fixed it. The tree was re-verified green before commit — but the lesson is to use
the `edit` tool for reverts, never ad-hoc regex on multi-line Rust.

### 2026-08-24 — T-104c briefed (frontend jobs bridge + store + queue panel)

Closed the gap T-104b left, as [tasks/t-104c-brief.md](tasks/t-104c-brief.md): the typed
`bridge/jobs.ts` wrappers (invoke + `listen`), a `useJobsStore` queue with a pure `applyJobEvent`
fold, and a `JobQueue` component in AudioStudio.

**Verified the frontend the same way the Rust has been — by running it, not recalling it.** The
`@tauri-apps/api` v2 `listen<T>`/`UnlistenFn` signatures were read from `node_modules` (not memory),
and the reference code was written into the repo, gated (`tsc -b` clean, `oxlint` 0 warnings, 21
vitest tests / 9 new, `vite build`), then reverted so the brief is the only artifact. Two facts
worth recording:

1. **The `job://` event name is legal on both sides.** Tauri validates event names to
   `alphanumeric + - / : _` — the same charset in the Rust `is_event_name_valid` and the frontend
   `listen` docs. This was worth checking explicitly: had `://` been rejected, T-104b's `emit`
   calls would have silently no-op'd and every later job would have streamed into the void.
2. **The frontend test seam is `vi.mock` at the module boundary** — the store test mocks
   `../bridge/jobs` (its own module), the bridge test mocks `@tauri-apps/api/event` to pin the
   three event-name strings. The `mock`-prefixed hoisted-variable convention is the one
   `state/config.test.ts` already uses, so the mock factory can see its variables.

**The one risk the brief can't remove:** the event-name spelling is the only place the frontend and
Rust must agree by string, and it is only pinned by a mock, not a shared fixture (events are
one-way; there is no round-trip). A producer live smoke check at T-113 will confirm end to end.

### 2026-08-24 — T-104c landed

Aider transcribed the brief exactly — all seven files match the reference, `tsc`/`oxlint` clean,
21 vitest tests (9 new), `vite build` green. The generation path is now wired end to end at the
plumbing level: connect → run → poll → events → store → queue panel. What is still unwired is the
*pipeline* that produces a workflow to run.

**One finding, from asking "what did the brief fail to ask for":** a submission/event race. `run`
adds the job to the store as `"queued"` only after `run_workflow` resolves, but the backend spawns
its monitor before returning and the monitor's first poll is immediate — so a `job://done`/`failed`
could in principle arrive before the store has the id, be dropped by `applyJobEvent`'s ignore-unknown
rule, and leave the job stuck at `"queued"` forever. **Practically benign:** the window is the ~ms
IPC round-trip, and a real music job cannot go terminal in it (model load alone is seconds;
`run_workflow` pre-validation already rejects the common instant failures before any monitor is
spawned). The `"ignores unknown id"` rule itself is the right call — a stray event must not fabricate
a phantom job. Recorded rather than fixed because the robust fix (upsert on terminal events + a
non-regressing `run` set) changes a deliberate semantic, and the app never submits sub-10 ms jobs.
Revisit only if a fast terminal path ever appears.

**Carry forward:** this closes the "unassigned frontend jobs" gap T-104b flagged. The
bridge/store/queue seam (T-104c) is now the template for every later frontend surface: invoke/listen
wrapped in `bridge/`, state in a Zustand store with a pure fold, `vi.mock` at the module boundary in
tests. Next is **T-105** (models) back on the phase order.

### 2026-08-24 — T-105 planned and split; models surface captured

Captured the whole models surface live for T-105 and wrote [t-105a-brief.md](tasks/t-105a-brief.md)
+ [t-105b-brief.md](tasks/t-105b-brief.md) (split for the ~400-line rule, the T-103 pattern).
Recorded in **MCP-SURFACE §11**.

**The headline finding: `search_models` has THREE shapes, not two.** The same tool returns
`folders: [{name, subfolders}]` with no args, `files: [{name, pathIndex}]` with `folder=`, and
`rows: [{name, type, tags, …}]` with `query=`. The folder/query distinction was already flagged in
the phase file; the third (list-folders) mode and the **camelCase `pathIndex`** are new, and the
query rows' registry fields (`base_model`, `trained_words`, `source_url`, `preview_url`, `size`,
`id`) are **always null on the local surface** — so `ModelHit` models only `name`/`type`/`tags`.

**Two download facts worth carrying forward:** `download_model`'s `filename` is *effectively*
required when the URL does not end in the file name (`[missing_argument]`), and `download` returns
one shape for `status`/`wait`/`cancel` with terminal `"failed"` verified, `"completed"` inferred
(the bogus URL failed and comfy-cli cleaned up its own partial — no junk left, but also no
completed-shape capture). The download progress UI that will stream this is T-111, not here.

**Carry forward:** the "verify the exact shape, not just the tool" discipline caught the
`pathIndex` camelCase and the third search shape — both invisible to a review that trusts the
`search_models` name alone. This is the MCP-SURFACE §8.5 "24 of 25 subgraph addresses" lesson again:
the payload's real shape is the contract, and it is only learned by running the server.

### 2026-08-24 — T-105a landed; and the fmt defect came back, a fifth time

Aider transcribed the brief exactly; the diff touched only the two listed files and the executor
again dropped the non-ASCII `…`/`⚠` from doc comments (the ASCII-in-comments improvement, consistent
with T-104a). `mcp-bridge` is now 70 tests, all offline.

**The one gate failure was mine, and it is the recurring defect's clearest instance yet.** My
brief's `lib.rs` reference wrote the `pub use models::{…}` re-export multi-line, but `cargo fmt`
collapses it to one line (it fits under 100 chars). I had copied the new `models.rs` verbatim from
the post-`cargo fmt` scratch — and re-typed the `lib.rs` re-export from the pre-`cargo fmt` draft.
So the rule narrows from "copy from the fmt-clean scratch file" to **"run `cargo fmt` in the
scratch, then copy EVERY touched file — including the `lib.rs` re-export — from the post-fmt
state; `use`/`pub use` lines are code too." Five times now; the fix is to treat the scratch's
post-fmt state as the single source, not my write-tool drafts.

**The decode guards are genuinely armed** (reasoned, not re-run): `test_folder_decodes_files_with_path_index`
feeds the real `"pathIndex": 2` JSON, so a wrong `#[serde(rename)]` makes `path_index` default to
`0` and fails the `== 2` assertion; `test_search_decodes_rows_with_type` does the same for the
`type` → `ty` rename. Neither could pass for the wrong reason.

### 2026-08-24 — T-105b landed; the fmt rule finally held for a full run

Aider transcribed the brief exactly; `download.rs` is byte-identical to the reference and
`mcp-bridge` is now 74 tests. **The first run in six with no fmt defect** — my T-105b `lib.rs`
reference had the re-export on one line, so the tightened rule (copy every touched file from the
post-`cargo fmt` scratch) worked when actually followed.

The one fix-up was cosmetic, and mine in origin: I wrote "alphabetical, between `error` and `jobs`"
in the brief, but `download` sorts *before* `error`, so the executor's `mod download;` landed
correctly at the top while `pub use download` landed after `pub use error`. Moved it for
alphabetical consistency. Zero functional impact — a reminder that `mod`/`pub use` ordering is a
convention the brief should pin precisely, not hand-wave at.

**The terminal-semantics guard is genuine:** `test_download_state_is_terminal_and_success`
constructs `DownloadState` literals directly (no transport), so `"failed"` → terminal-but-not-
success and `"completed"` → terminal-and-success are asserted exactly; a regression in either arm
fails the test. `mcp-bridge` now covers the full comfy-mcp surface except `nodes` (T-106).

### 2026-08-24 — session closed; handoff to the next one

**State:** Phase 1 is a little over half done. Landed: T-101, T-102, T-102b, T-102c, T-103a/b/c,
T-104a/b/c, T-105a/b. `mcp-bridge` wraps the entire comfy-mcp surface **except `nodes`** (74
offline tests); the backend + job pump + frontend queue are wired end to end, but nothing connects
to a *model pipeline* yet (that is T-107's profile loader feeding T-110's wizard). Tree is clean,
pushed, `npm run gate` green.

**Next session, first action:** pick up **T-106** (node registry) per [tasks/phase-1.md](tasks/phase-1.md).
It has a **"Before T-106" step**: capture the `nodes(action="get")` full response shape live —
MCP-SURFACE §4 verifies the LoRA-enumeration fact (`LoraLoaderModelOnly` → `lora_name` COMBO whose
`choices` are the installed LoRA paths) but not the full node schema. Then T-106b (MiniMax profile),
T-107 (profile loader), T-108/T-109 (`llm-bridge`), T-110–T-112 (wizard), T-113 (milestone).

**The one process rule that has cost the most:** reference code goes into a brief only after
`cargo fmt` in the scratch crate, and **every touched file — including `lib.rs` re-exports — is
copied from the post-fmt state, never re-typed.** Six occurrences and counting; it is the single
most common gate failure in this project.
### 2026-08-24 (later) — T-106 landed; the node registry closes the comfy-mcp surface

`nodes.rs` (318 lines, +5 tests -> `mcp-bridge` at 79) wraps `nodes(action="get")`: `NodeSchema`
with metadata + `inputs[]` + `outputs[]`, `input(name)`, and the `choices_for(name)` primitive that
both `from_node_choices` enums and the Phase 3 LoRA picker read. The diff touched only the two
listed files. **`mcp-bridge` now covers every comfy-mcp tool the app needs** — the surface is closed.

**The two live-captured traps both landed in the type, not in prose.** `NodeOptions` keeps
`min`/`max`/`step`/`default` as `Option<Value>` rather than numbers, because `default` is variously
a string, a bool, a number or null, and the `INT` seed's `max` is `u64::MAX` — which does not fit
`i64`. That is the same precision argument that made `Seed` its own `InputSpec` variant back in
T-003; it has now bitten twice, in unrelated modules, from opposite directions.

**Guards reviewed (reasoned, not re-run):** `test_options_default_is_polymorphic` feeds the real
`u64::MAX` max and a string default, so narrowing `NodeOptions` to numeric types fails it;
`test_unknown_node_is_a_tool_error` pins the `Ok(is_error: true)` decode that MCP-SURFACE §8 calls
the shape of bug that ships. Neither can pass for the wrong reason.

### 2026-08-24 (later) — T-106b landed; the second profile, and the schema grew one field

`profiles/minimax-music-3.json` plus `ComfySpec.slot_overrides`
(`BTreeMap<SlotAddress, InputValue>`) and 4 tests (`create-core` at 24). ARCHITECTURE §5 documents
the field in the same commit as the schema change, per the docs rule.

**Writing a second profile is what proved the schema, and the schema moved.** ACE-Step alone had
never exercised subgraph addressing (`37/13.caption`), a three-way seed fan-out, a `caption` input
where ACE-Step has `tags`, or a template whose save node is *already* `SaveAudioAdvanced`. The last
one matters most: it proves the save-node swap is **conditional per profile**, not a universal
pipeline step — a pipeline that always substituted would have corrupted this template.

**`slot_overrides` is the generalisation of a one-off fix.** MiniMax's template hardcodes the fp16
DiT filename while the installed weights are int8, so the profile pins `37/6.unet_name` rather than
the app special-casing one model. Any profile can now target a checkpoint variant its template gets
wrong, applied to the fetched template before the user's inputs.

**The license test is doing real work.** `test_minimax_fixture_surfaces_the_conditional_license`
asserts the notes carry both "attribution" and the revenue threshold, because this is the first
shipped profile whose weights are open-*with-conditions*: users ship these tracks commercially, and
T-111 must show those terms wherever the model is chosen.
### 2026-08-24 — session opened on drift; T-107 briefed as a split

**Session ritual caught real drift.** T-106 and T-106b had landed with no PROJECT.md entry:
the Snapshot still read "next up: T-106", the landed list stopped at T-105b, the mcp-bridge
test count said 74 (79), and tasks/phase-1.md carried no LANDED markers for either. Written
up and committed before touching T-107. Gate was green on arrival.

**T-107 is two briefs, and the reason is worth recording.** I wrote the whole reference
implementation first — loader plus address collector — compiled it, ran `cargo fmt`,
clippy and the full gate against the real crates, and only then measured: **529 lines**
against the ~400 rule. Splitting after verification rather than guessing beforehand cost
nothing and made the split obvious, because the two halves turned out to live in different
crates anyway: [t-107a-brief.md](tasks/t-107a-brief.md) is `library::profiles` (339 lines,
7 tests), [t-107b-brief.md](tasks/t-107b-brief.md) is `ModelProfile::slot_addresses()` (180
lines, 4 tests).

**The design question this task actually posed was "where does the check live".** The phase
file's sentence — "validates that a profile's slot addresses exist in its template" —
reads like one function, but the fetch is `mcp-bridge`, the profile is `create-core`, and
`library` is the on-disk store. Making `library` do it would have dragged an MCP dependency
into the crate that ARCHITECTURE §2 defines as files-on-disk. The answer that needed no new
code at all: `mcp-bridge`'s `SlotList::missing` has done the comparison since T-103b, so
T-107b adds only the collector and the two compose at the `src-tauri` seam. Recorded in the
decisions log and ARCHITECTURE §5.

**A profile with a wrong slot address fails silently, which is why this exists.** ComfyUI
does not reject an address it does not have — the app simply never writes that input, and
the template generates from its own default prompt. The user gets a plausible track that
ignores what they typed. `test_shipped_ace_step_addresses_all_exist_in_the_verified_template`
turns that into a build failure by checking the shipped profile against the 24 addresses
MCP-SURFACE §3 captured live.

**The fmt rule held again.** Reference code was written into the real tree, compiled,
`cargo fmt`-ed, gate-run, then copied out post-fmt and reverted — so both briefs carry
text `cargo fmt` is a no-op on. Second run in a row without the recurring defect.
### 2026-08-24 (later) — T-107a and T-107b landed; both transcribed byte-identically

Producer ran both briefs (T-104a through T-106b having been run in opencode during a
weekly-refresh gap here, which is where the doc drift this session opened on came from).
Gate green: `library` 11 -> 18 tests, `create-core` 24 -> 28, `mcp-bridge` unchanged at 79.

**Both files came back byte-identical to the brief's reference code.** Diffing the landed
`profiles.rs` against the verified scratch copy showed zero lines of difference, and the
`impl`/test blocks in `profile.rs` matched exactly. That is the strongest evidence yet for
WORKFLOW —1's claim: a brief carrying compiled, `cargo fmt`-clean reference code gets
transcribed rather than reinterpreted. No fmt defect for the third run running.

**Two placement defects, both fixed directly rather than by a fix-up brief** (WORKFLOW —2):

1. The executor put T-107b's two `impl` blocks immediately after `InputSpec` instead of
   before `mod tests` as the brief said. Rust does not care, but it left `impl ModelProfile`
   about 120 lines *above* `struct ModelProfile` and rewrote every untouched type in
   between — a 3x larger diff for no reason. Moved to the specified position.
2. The executor ASCII-ified `ARCHITECTURE.md —8` to `ARCHITECTURE.md 8` in `library/src/lib.rs`'s
   module doc — a line the brief did not touch. CONVENTIONS does say ASCII in comments, and
   past runs' `...`/`!` strippings were accepted, but eight other section signs remain across
   the workspace, so this one made the codebase *less* consistent. Restored.

   Worth carrying: the executor edits ASCII-adjacent lines it happens to be transcribing past.
   When a brief pastes a "complete file after the change", it is also handing over every
   unrelated line in that file.

**The guards were armed by mutation, not by reasoning.** Injecting `94.tag` for `94.tags`
into the shipped ACE-Step profile failed
`test_shipped_ace_step_addresses_all_exist_in_the_verified_template` with the offending
address named, and flipping the merge to `entry().or_insert()` failed
`test_user_profile_replaces_shipped_and_reports_it`. Both restored. This is the T-003 lesson
applied ahead of time rather than after: a test that has never been seen to fail is a claim,
not a guard.

**What this unblocks:** the wizard now has a model list to show (T-111) and a way to tell a
user *why* a model is missing. The slot check still needs its one call at the `src-tauri`
seam, which belongs to T-110's wiring, not to either of these tasks.
### 2026-08-24 (later) — T-108 verified live and briefed in three

Ollama was running on this box, so the OpenAI-compatible surface was captured live rather
than recalled: model list, streaming frames, three error shapes, the usage frame. Recorded
in **[docs/LLM-SURFACE.md](docs/LLM-SURFACE.md)** with the raw capture committed to
`testdata/llm/`, and referenced from AGENTS.md beside MCP-SURFACE. Then the whole
implementation was written, compiled, `cargo fmt`-ed, gate-run **and run against the live
endpoint** before any of it entered a brief. 1093 lines — three briefs.

**The finding that shaped the design.** Prompted "Reply with exactly: tulip",
`gemma4:12b-it-qat` — the model docs/MODELS.md recommends for lyric writing — produced
**163 characters of `delta.reasoning` and 5 of `delta.content`**. Not a reasoning-branded
model; an ordinary instruct model, on the app's own recommended path. Three ways to get
this wrong, all of which look reasonable while writing the code:

1. concatenate every text field — 163 characters of the model's deliberation land in the
   user's song;
2. read only `content` — the UI sits frozen through 40 frames of a healthy stream;
3. treat presence as text — `"content":""` ships on nearly every frame, so
   `is_some()` is true throughout a stream carrying no content at all.

`ChatDelta` is therefore an enum, and only `Content` may reach the document. The ecosystem
also spells the field two ways (`reasoning` vs `reasoning_content`) and real clients have
shipped the bug of handling one; both are read.

**Second trap, same class as MCP-SURFACE's:** the usage frame carries `"choices":[]`, so
`chunk.choices[0]` — the obvious line to write — fails on the last frame of every metered
stream. And an error body is not necessarily JSON: a base URL missing `/v1` answers
`404 page not found` in plain text, so the error path decodes the envelope and falls back
to the raw body.

**Verifying by compiling paid for itself immediately.** reqwest 0.13 renamed its TLS
features; `rustls-tls-native-roots` does not exist and the build failed outright. Written
from memory into a brief, that would have cost an executor round trip. The working feature
is plain `rustls`, which pulls `rustls-native-certs` and uses the OS trust store — no
OpenSSL, so Linux CI needs nothing extra. All added crates permissive, checked with
`cargo metadata`.

**A live test is kept, not thrown away.** `test_live_stream_returns_content_separated_from_reasoning`
is `#[ignore]`d like `library`'s keychain test and passed against Ollama in 5.75 s. It is
what proves reqwest streams incrementally instead of buffering the response, which no
offline test can show, and it is now on the T-113 checklist.

**One brief is knowingly over the limit.** T-108c is ~435 lines against the ~400 guide;
splitting one stream state machine across two runs would cost more than it saves. Said in
the brief rather than quietly ignored.
### 2026-08-24 (later) — T-108a/b/c landed; four files byte-identical, one predicted defect

Producer ran all three briefs. Gate green, `llm-bridge` 1 -> **22 tests plus 1 ignored**,
and the ignored live test passed against Ollama in 5.18 s. Landed as one commit because
`Cargo.toml` and `lib.rs` each carry changes from all three briefs; splitting the history
would have meant reconstructing and gating two intermediate states for cosmetic reasons.

**All four new files came back byte-identical to the briefs' reference code** — error.rs,
sse.rs, wire.rs and openai.rs, 1000+ lines, zero differences. Fourth consecutive run
without a formatting defect.

**The one fix was the defect I predicted last session and did nothing about.** The executor
ASCII-ified `ARCHITECTURE.md §4` to `ARCHITECTURE.md 4` in `lib.rs`'s module doc — a
pre-existing line, outside the brief's change — exactly as it did in T-107a. The T-107
session log even names the mechanism: *when a brief pastes a "complete file after the
change", it hands over every unrelated line in that file.* I then pasted a complete
`lib.rs` in T-108a anyway. **Rule going forward: a brief modifying an existing file gives
the changed lines and their anchors, never the whole file, unless the whole file is new.**
T-108b and T-108c did it that way and neither drifted.

**Guards armed by mutation, not by reasoning.** Two mattered enough to break on purpose:
a derived `Debug` on `OpenAiCompat` fails `test_debug_never_prints_the_api_key` with
`api_key: Some("sk-secret-123")` printed in the failure output, and routing `reasoning`
into `Content` fails three tests including the real-stream replay. Both restored.

**One thing to watch.** The `rustls` feature grew Cargo.lock by 396 lines and pulls
`aws-lc-rs`, which compiles C via cmake. Recorded as a decision above with its rationale
rather than absorbed silently — the first CI run on this commit is what confirms all three
runners build it.
### 2026-08-24 (later) — T-109 verified live and briefed in two; the trait question answered

Captured Ollama's own API live — `/api/tags`, `/api/show`, `/api/version`, `/api/ps`, and a
real 46 MB `/api/pull` — recorded as **LLM-SURFACE 8-9** with both captures committed to
`testdata/llm/`. Implementation written, compiled, gate-run and run against the live server
before briefing. 745 lines, so two briefs.

**T-109 was supposed to settle the `LlmProvider` trait, and it did — by disproving it.**
The expected second implementation is not an implementation of the same thing:
**`ollama_native` does not chat.** Ollama's `/v1/chat/completions` already goes through
`openai_compat`, and a second path to the same tokens would be two things to keep correct.
What the native API adds is facts *about* models. Forcing it into the trait would have
meant a `stream_chat` that returns an error — the recognisable shape of a wrong
abstraction. So the trait stays unwritten, now for a stronger reason than "only one impl":
the obvious candidate turned out to be a different kind of thing. `anthropic` will settle
it properly.

**The single most useful field is `capabilities`.** `nomic-embed-text` reports
`["embedding"]` and nothing else — yet `/v1/models` lists it identically to a chat model.
Without the native call, an embedding model sits in the lyric picker and fails only after
the user has chosen it and written a brief. The same field carries `thinking`, which is the
only advance warning that a model will spend budget on `delta.reasoning` (T-108's finding),
and it is set on every completion model on this box.

**`remote_host` turned out to be a privacy disclosure.** Cloud models are listed beside
local ones; generating with one sends unreleased lyrics to another party. Recorded as a
decision, because it is a UI requirement rather than a detail. Their `size` is a stub
manifest — 308 bytes for a 2.81T model — so it must never be shown as disk usage.

**Second sighting of the "success that isn't" bug, in a second protocol.** `/api/pull`
answers **HTTP 200** when the pull fails, putting the error in a frame in the body. That is
exactly comfy-mcp's `Ok(is_error: true)` (MCP-SURFACE 8), found independently in an
unrelated service. It now has its own error variant, `LlmError::Reported`. Worth treating as
a general expectation rather than a quirk: **a streaming API's HTTP status describes the
connection, not the operation.**

**Two decode traps that would have shipped.** `families` arrives as JSON null on cloud
entries, and `#[serde(default)]` on a `Vec<String>` rejects an explicit null — the whole
model list would fail to decode the moment a user signs in to Ollama's cloud. And
`completed` is absent on 12 of 23 pull frames, so a `u64` default would report "0 bytes
fetched" for a layer that has merely started, which looks exactly like a stall.

**Applied the T-108 lesson this time.** Both briefs give **changed lines and anchors** for
existing files instead of pasting complete files, and T-109a makes "the module doc comment
is unchanged, section sign included" an explicit acceptance criterion. The ASCII-ification
defect has appeared in two consecutive runs, both times on a line handed over needlessly.

**One note for the producer:** capturing the pull frames required a real download, so
`all-minilm` (46 MB) is now installed on the box. Remove it with `ollama rm all-minilm` if
unwanted — though `test_live_pull_of_an_installed_model_reaches_success` re-verifies
against it without downloading, so keeping it makes that live check free.
### 2026-08-24 (later) — T-109a/b landed; the executor was right and I was wrong, three times

Both briefs run. Gate green, `llm-bridge` 22 -> **34 tests plus 3 ignored**, and all three
live checks pass against Ollama 0.32.15. Landed as one commit because `error.rs`,
`ollama.rs` and `lib.rs` each carry changes from both briefs.

**The changed-lines-and-anchors fix worked.** `error.rs` and `lib.rs` came back
**byte-identical** — pure additions, no deletions, none of the drift the last two runs had.
Giving an executor a complete file hands it every unrelated line; giving it the changed
lines and their anchors does not. That is now settled by experiment rather than argument.

**But the same defect reappeared in the new file, and this time it was mine.** The executor
stripped the two warning-sign characters from `pull.rs` doc comments — my own reference
code. CONVENTIONS says *"ASCII in code/comments"*, and the warning sign is not ASCII. So the
executor was right. Checking properly showed it has been right every time: the section sign
I "restored" twice (T-107a, T-108) is **also** non-ASCII and **also** a violation; I argued
consistency with eight other violations rather than reading the rule.

Fixed by making the rule true instead of arguing with it: **all 9 section signs purged from
Rust comments repo-wide**, replaced with `section N`, and CONVENTIONS now spells out that
the rule covers every non-ASCII character with examples. Three consecutive runs spent
churning one character each is three too many, and two of my earlier "small review defect"
fixes were the defect.

**Guards armed by mutation, all three precise:** `failure()` returning `None` fails the
HTTP-200 test; treating a started-but-empty layer as "no progress" fails the
`completed`-absent test; a `can_chat()` that always returns true fails both embedding-filter
tests.

**Phase 1's bridges are done.** `mcp-bridge` (79 tests) and `llm-bridge` (34 + 3 live) both
cover their verified surfaces. What remains is the wizard (T-110-T-112), which is UI and
Tauri wiring over surfaces already proven, and T-113's live milestone.
### 2026-08-24 (later) — T-110 verified live and briefed in three; the first UI task

Captured `server_info` and `launch_comfyui` live from comfy-cli 1.16.0 (**MCP-SURFACE 13**,
payload committed to `testdata/mcp/`). Wrote the whole step — crate, Tauri commands, store,
view, CSS — compiled it, ran the gate, and drove all five rendered states through the real
store in a browser before briefing. 922 lines plus CSS, so three briefs.

**The `ServerInfo` written at T-101 was guesswork, and this is what that costs.** It modelled
three blocks as opaque `Value`s. The live payload carries **seven**, four of which the wizard
needs: `server.running`, `hardware.gpu.vram_bytes` (the number a profile's `vram_gb_min` is
checked against), `workspace.path`, and `freshness.core.outdated`. It was written before
anyone had seen the payload, which is exactly the practice the project banned after
2026-08-23 — and it survived nine tasks because nothing had needed it yet.

**A third polymorphic-shape trap, in a third service.** `freshness` is either
`{"core": {...}, "packs": [...]}` or `{"unsupported": true}` — "could not check", not "up to
date". Rendering it as an update badge gives the user a notice they can never clear. That is
now three services (comfy-mcp `search_models`, Ollama `families`, this) where the shape
depends on the answer, and the pattern is worth expecting rather than discovering.

**Absent is not zero, twice over.** No `server` block means ComfyUI is **down**, not unknown
— comfy-mcp answers happily while ComfyUI is dead, which is precisely the state the wizard
exists to show. No `hardware` block means VRAM is **unknown**, and rendering that as `0 GB`
puts a hardware warning on a working machine. Both are carried as `Option` all the way to the
UI, where `formatVram` returns null rather than a zero reading.

**The browser check found no bug, and that itself took work.** All five states rendered
correctly, but computed pill colours read as muted on every one of them. The cause was the
180 ms colour transition: **the review pane does not composite frames, so transitions never
advance**, and a mid-transition read returns the start value. WORKFLOW section 5 records this
exact limitation from the sibling repos, and I still spent four probes rediscovering it —
including one `requestAnimationFrame` call that hung the tool for 30 seconds. Reading a
`transition: none` clone gives the true value; the animation itself is listed in the brief as
unverified, for the producer's click-through.

**Process note:** all three briefs give changed lines and anchors for existing files, and
each carries "no non-ASCII characters anywhere in the diff" as an explicit acceptance
criterion. A separate task is queued to enforce that in the gate rather than in prose — it
has now cost a review round on four consecutive tasks, most recently my own `health.rs`.

### 2026-08-25 — T-110a/b/c landed; mutation testing found two guards I had written and never armed

All three runs came back **byte-identical to the verified reference** on every new file
(`health.rs`, `comfy.rs`, `bridge/comfy.ts`, `state/comfy.ts`, `comfy.test.ts`, `Setup.tsx`) and on the
`theme.css` and `types.rs` edits. One defect, and a trivial one: `mod jobs;` landed after
`mod local;` in `mcp-bridge/src/lib.rs`. Fixed directly. Test counts hit the briefed targets
exactly — mcp-bridge 79 to 86, app 7 to 12, vitest 21 to 28 across 6 files.

**The ASCII rule held for the first time in four tasks.** Not one non-ASCII character in the
diff. Making it an explicit acceptance criterion in each brief appears to have been enough;
the gate check is still worth having, because prose that works is still prose.

**Then mutation testing earned its place.** Four guards checked; two survived, and both were
guards I wrote myself in the reference implementation, with tests I also wrote that did not
arm them.

- Deleting the `unsupported` early return from `Freshness::update_available` changed nothing:
  all 86 tests passed. The unsupported payload carries no `core` block, so the function
  falls through to `false` anyway. The test asserted the *outcome* on the captured shape, not
  the *rule*. Only a payload carrying both `unsupported: true` and a stale `core` can tell a
  present guard from an absent one, and that is the test now added.
- Replacing `is_running`'s body with `self.server.is_some()` also passed all 86 — mcp-bridge
  never exercised `running: false`. It was caught, but one crate downstream, by
  `test_stopped_server_classifies_as_server_down` in `src-tauri`. A rule should be armed in
  the crate that owns it, not by a caller that happens to cover it.

**The lesson, stated plainly: a test written from a captured payload guards the payload, not
the rule.** Both of these read as thorough — they name the right rule in the doc comment and
assert the right answer — and both would have let the rule be deleted silently. The captured
shape made the guard look covered precisely because the captured shape is the easy case.
Worth applying to the other payload-derived tests as they are written, not retrofitted.

The two armed guards behaved: breaking `is_port_in_use` and dropping `server_down`'s next
step each failed exactly one test, the sweep naming the offending state in its message.

mcp-bridge is now 87 tests. Gate green; landed as `50186c2`. No browser re-verification — the
frontend is byte-identical to what was driven through all five states before briefing. The
pill's colour transition remains unverified by me, for the producer's click-through.

### 2026-08-25 (later) — T-111 verified live and briefed in five; and T-110's launch shape was wrong

Started ComfyUI to verify the models surface and the very first call disproved a line I had
written into MCP-SURFACE the same morning. **`launch_comfyui` success carries no `ok` key:**

```json
{"background": true, "listen": "127.0.0.1", "port": 8188, "url": "http://127.0.0.1:8188", "pid": 23404}
```

T-110 captured the *failure* path live (`[port_in_use]`) and took the *success* shape from the
tool's docstring, which promises `{"ok": true}`. `#[serde(default)]` meant it decoded fine and
read `false` from every real launch; nothing branched on it, so the shipped behaviour was
correct by luck. The test asserted `result.ok` against a mock returning a shape the server
never sends — a guard around a fiction. Fixed in `50186c2`... `5b04e42`, with a second test
pinning the actual rule: success is the `Ok` arm, never a field. `stop_comfyui` has no `ok`
key either. **The lesson is narrower than "verify live" and worth stating exactly: verifying
the error path is not verifying the success path.**

**Then T-111, where four separate findings changed the design.**

**`search_models` needs a running ComfyUI.** Its docstring says "re-read from disk every
call"; it fetches `http://127.0.0.1:8188/models` and fails `[server_not_running]` when the
server is down. So the models step cannot answer anything until the ComfyUI step is green, and
it needs an explicit "cannot check" state. This is the single most damaging confusion available
to this step: ACE-Step is 18.5 GiB, and reporting an empty install to a user whose server is
merely stopped would send them to re-download models they already have.

**Nothing answers "which model files does this workflow need".** `workflow_deps` maps node
classes to node *packs*; `node_dependencies` checks a pack's *Python* requirements. The only
signal is `local_check.errors`, which is English prose. Deciding on a multi-gigabyte download
by parsing that is not acceptable, so the profile declares its own file list.

**`local_check` is worse than merely unhelpful here.** `runnable: false` does not mean models
are missing — MiniMax has all three files and fails on the fp16/int8 filename its own
`slot_overrides` corrects. And `local_check.summary` renders *every* such problem as node-class
advice ("Update ComfyUI and its custom nodes, or pick another template"), which for a missing
model points at something that cannot fix it. It must never be shown to a user.

**A repackaged repo's licence tag is not the model's licence.** `Comfy-Org/MiniMax-Music-3` is
tagged Apache-2.0; the upstream carries a custom community licence with an attribution
obligation and a revenue threshold. Had the UI read the licence from the download host it would
have told users they had no obligations they in fact have.

**Two items in the original T-111 line were disproved and dropped**, rather than built as
written: the quiet "update available" badge (no version or hash data exists for model files,
so it would be invented) and the advanced `search_models` expander (a different feature,
backlogged). Both are recorded in the decisions log.

Verification went further than usual because the machine offered a free case: ACE-Step, the
app's *default* model, is not installed here, while MiniMax's three files are — one profile in
each state. The reference implementation was run against the real comfy-mcp and a real
ComfyUI via an `#[ignore]` test that passes, and all five rendered states were driven through
the real store in a browser. Byte-weighted progress was confirmed live at 8% where a
file-counted bar would have read 25%.

Two process notes. The pill-colour reading needed the `transition: none` clone again
(WORKFLOW section 5) and this time I went straight to it. And my first pass at adding
`comfy.models` ran the profiles through `json.dumps`, which reformatted both files and turned
an 11-line change into 171 lines of noise; reverted and inserted textually.

Also fills the MCP-SURFACE 9.4 gap: the `local_check: {"checked": false}` arm is now verified
live rather than quoted from documentation. It carries a `reason` and `summary` the tool does
not document, and **the template is still written to `out_path`** when the check cannot run.

~1659 lines, so five briefs rather than the usual three. `install.rs` was split out of
`models.rs` mid-way, which improved the module boundary as well as the split — reporting and
acting are different jobs. Gate green.

### 2026-08-25 (later still) — T-111a-e landed; the real install ran, and settled two questions

All five runs came back **byte-identical to the verified reference** on every new file. One
difference, and it was an improvement: `install.rs` gained a blank line before `#[cfg(test)]`.
Test counts hit the briefed targets — create-core 28 to 34, app 12 to 19 (+1 ignored), vitest 28
to 41. My brief said 41 across *8* files; it is 7. My miscount, not the executor's.

**The executor caught a bug I had missed, again.** It converted a pre-existing em dash on line 1
of `profile.rs` to `--`. Correct: CONVENTIONS names em dash explicitly. That is five runs in a
row where an executor was right about this rule. It prompted a proper sweep, which turned up
**four more** in `generation.rs`, `project.rs` and `provenance.rs`, plus one in a `config.test.ts`
comment — all from earlier tasks, all now purged. The em dash in `CoverArt.tsx` stays: it is a
rendered UI string, which the rule explicitly permits, and the distinction is the whole reason
the rule says "code/comments" rather than "files".

**Mutation testing: seven guards, seven caught.** A clean sweep, where T-110 had two survivors
and the fix was to arm them. The instructive one was byte-weighted progress: the file-counted
mutant reported **50%** where the real code says **3%**.

**Then the producer offered their bandwidth, and the real install answered two open questions.**

`"completed"` is no longer inferred. Four concurrent downloads reported `starting` at submit,
then `downloading`, then `completed`, and nothing else. `isTerminal` was right as written — but
it was right on an assumption, and the cost of being wrong was an install UI spinning forever on
a finished download.

And **freshly downloaded files appear with no ComfyUI restart.** The readiness check flipped from
four-missing to Ready the moment the transfer finished. That was the live risk in the design: had
ComfyUI cached its folder listing, a user would have downloaded 18.5 GiB and still been told
"Not installed" — the exact failure the whole step is built to avoid, arriving through a door I
had not checked.

**A number worth keeping: 821 seconds.** 18.5 GiB at roughly 23 MB/s aggregate, on a 2 Gbit
line. The host throttles, so this is a minutes-long operation no matter how fast the user's
connection is. The progress UI is not a nicety.

**Two new traps found while confirming the install, neither affecting T-111.** `search_models`
returns `name` as a **relative path with the OS-native separator** — this install has
`loragoth\checkpoint-epoch-105\adapter\adapter_model.safetensors`, three levels deep — so a
nested file cannot be named portably in a profile, and `ModelFileSpec` now says to declare
top-level files only. And `subfolders` comes back `[]` from list-folders **even for folders that
demonstrably have subfolders**, so it answers nothing. Both matter for the Phase 3 LoRA picker
and are recorded in MCP-SURFACE 11.1. The listing is also unfiltered: `loras` returns
`training_state.pt` files beside the adapters.

ACE-Step 1.5 XL Turbo is now installed on the producer's machine, which makes T-113's live
milestone reachable and leaves the remaining VRAM open question answerable by an actual run.
Gate green; landed as `ca610ad`, with the install findings following.

### 2026-08-25 (later still) — T-112 verified live and briefed in four; T-109 paid for itself

The LLM step was verified against the producer's real Ollama, 0.32.15 with 13 models. That
catalogue turned out to be an unusually good test case, and it settled the design in four
places.

**T-109's `ollama_native` work paid for itself, and the numbers are stark.** `/v1/models`
returns four keys per row — id, object, created, owned_by — and nothing else. Of the 13
models here, **2 cannot chat at all** (`all-minilm`, `nomic-embed-text`) and **8 run on
Ollama's servers**. The OpenAI-compatible list presents all 13 identically. Without enrichment
the wizard offers two models that fail later at lyric time, far from the screen where the
choice was made, and says nothing at all when a user picks one that ships their unreleased
lyrics to a third party. A live test now asserts exactly this: 13 ids, 13 enriched, 2 unusable
removed, 8 remote disclosed.

**The privacy half is the sharper one.** Over the OpenAI API there is no way to tell a remote
model from a local one; the only hint is the `:cloud` suffix, which is a naming convention, not
a contract. `remote_host` from `/api/tags` is the only reliable signal, and it comes in two
forms (`https://ollama.com` and `https://ollama.com:443`). That drove the rule that unknown is
never rendered as false: on a non-Ollama endpoint the app cannot check, and a silent absence of
disclosure reads to a user as "this is private".

**A thinking model spends the token budget on reasoning first.** Section 2 recorded this for
generation; it bites the *test call* just as hard, and now with numbers. Asking `gemma4:12b-32k`
to "Reply with exactly: ok" returned **empty content** with `finish_reason: length` at 20
tokens, and `"ok"` after 108 characters of reasoning at 400. A test call that asserts non-empty
content reports a broken endpoint to a user whose setup is fine. Success is now a well-formed
response, and an ignored live test pins it.

**Recommendation matching cannot be equality.** MODELS.md asks for a "Gemma 4 12B" chip. This
machine has two of them — `gemma4:12b-32k` and `gemma4:12b-it-qat` — and **neither is named
`gemma4:12b`**. Equality would have recommended nothing while the recommended model sat
installed. Prefix matching, deterministic preselect (lowest id), and a configured model always
wins over a suggestion.

**A doc correction found along the way.** LLM-SURFACE section 10 still listed "`/api/pull` of a
model already up to date" as unverified, but T-109 shipped
`test_live_pull_of_an_installed_model_reaches_success`, which runs green. The list of unknowns
had gone stale in the direction that matters least — claiming less coverage than exists — but
it is the same class of error as claiming more, so it is corrected.

Also settled a small interpretation: MODELS.md said the suggestion list "lives in this file and
the wizard reads it as data". Parsing a markdown table at runtime would be fragile, so the
machine-readable list is now `data/lyric-llms.json`, shipped as a bundle resource beside
`profiles/`, and MODELS.md's table is explicitly its human-readable twin. `data/` is kept out of
`profiles/`, which is scanned for model profiles and would report a stray file as malformed.

~1405 lines of code, so four briefs. Gate green; vitest 41 to 51, create-core 34 to 41, library
19 to 22, app 20 to 29.

### 2026-08-25 (later still) — T-112a-d landed; and a fixture that could not fail

All four runs came back **byte-identical to the verified reference** on every new file, and the
`create-core`/`library` wiring diff matched exactly. Two cosmetic differences: an extra blank
line in `theme.css`, and `Cargo.lock` arriving unbuilt, which the first gate run fixed. Nothing
to correct.

**Mutation testing: eight guards, seven caught, one survivor — and the survivor is the same
mistake as last time, wearing different clothes.** Replacing the deterministic `.min()` in
`preselect` with `.next()`, so the picker follows whatever order the endpoint returned, passed
all 41 tests. The reason: my `INSTALLED` fixture is written in sorted order, so "lowest id" and
"first one seen" give the same answer against it. The test asserted an outcome that **both**
the correct and the broken implementation produce.

T-110's lesson was "a test written from a captured payload guards the payload, not the rule".
This is that lesson again at a different angle: **a fixture in sorted order cannot prove
sorting.** The fix is one line of input — the same two variants listed worst-first — which is
the only shape that can tell the two implementations apart. Armed and re-mutated; it fails now.

The seven that were caught are the ones the step exists for: unknown capability rendered as
`false` (twice, once for `can_chat` and once for the privacy flag), prefix matching degraded to
equality, a suggestion overriding the user's own configured model, the missing-`/v1` hint going
silent, a reasoning-only test call reported as failure, and an unchecked model losing the chip
that says so.

**Two errors of mine in the briefs, both in predicted test counts.** I wrote "51 across 9 files"
(it is 8) and "20 -> 29" for the app crate (it is 21 -> 30; I forgot the second ignored test I
had added in the T-111 follow-up). I made the same file-count slip in T-111d, saying 8 where it
was 7. Predicted counts are supposed to be a cheap check that the executor landed everything,
and one that is wrong by default trains the reviewer to ignore it. **Derive them from a run
before writing the brief, never from arithmetic.**

Live tests re-run against the landed code: 13 ids, 13 enriched, 2 unusable removed, 8 remote
disclosed; the test call returned `ok=true saw_reasoning=true content="ok"`. Gate green.
Final counts: create-core 41, library 21 (+1 ignored), llm-bridge 34 (+3), mcp-bridge 88,
app 26 (+4), vitest 51 across 8 files.

### 2026-08-25 — T-113 done; Phase 1 complete, tagged `phase1-done`

**Producer, live on the real install:** the wizard opened from cold, **ComfyUI started from the
app's own button**, server info rendered, the models step read Ready, and the LLM test call
returned. `cargo test -p llm-bridge -- --ignored` and `cargo test -p app -- --ignored` both
green.

**Architect, same session:** the two checks T-113 asks for that are verifiable from here. A
freshly fetched `audio_ace_step1_5_xl_turbo` now reports **`local_check: runnable: true` with
zero errors** — it had four, one per missing model file, when T-111 was written. That is the
cleanest possible end-to-end confirmation that the models step's declared-file list was correct:
the app downloaded exactly what the template needed, nothing more, nothing missing. And **all 17
slot addresses** the `ace-step-1.5-turbo` profile declares still resolve against that template.
No gallery drift, which was the specific risk T-113 named (24 h TTL).

**One seam recorded rather than glossed.** `models_install` and `models_progress` have never run
through the Tauri boundary. The 18.5 GiB download used the same `download_model` calls they
wrap, but the click-through came afterwards, when the models were present and the Install button
was therefore never offered. Verified by construction and unit tests, not by a click —
backlogged, because "the code underneath is tested" is exactly the reasoning that let the
`launch_comfyui` payload be wrong for a day.

**Phase 1 in one line: the app can now talk to both of the user's services and get them from a
clean install to "ready to generate".** Three wizard steps, each of which turned out to be
mostly about telling the truth when a service could not answer — ComfyUI down is not "no
models", an unreadable capability is not "local", a stopped server is not an empty catalogue.

**What the phase actually cost, and where.** Thirteen tasks became **34 executor runs**. Every
run but a handful came back byte-identical to a reference implementation the architect had
already compiled, gate-run and driven live, which is the loop working as designed. The expensive
mistakes were never executor mistakes:

- **Guessing a payload shape from documentation.** `ServerInfo` at T-101 (three opaque `Value`s
  where the live payload has seven blocks) and `LaunchResult` at T-110 (`{"ok": true}`, which
  the server never sends). Both cost a rewrite. The narrower lesson from the second: **verifying
  the error path is not verifying the success path.**
- **Tests that could not fail.** Four guards across T-110 and T-112 passed against mutations
  because the fixture was already in the state that made the rule invisible — an unsupported
  freshness block with no `core` to outrank, a model list already in sorted order. Mutation
  testing caught all four; nothing else would have.
- **Prose where a check belonged.** The ASCII rule, five tasks running.

**Handoff state:** working tree clean, gate green, `phase1-done` tagged and pushed. ComfyUI is
left **running** (the producer started it from the app); Ollama is running with 13 models.
ACE-Step 1.5 XL Turbo and MiniMax Music 3 are both fully installed.

### 2026-08-25 (later) — Phase 2 opened; the lyric surface verified before a line of it was planned

**Drift check first, as the ritual requires.** One commit since the last entry
(`3f84b1d`, the phase-1.md header reconcile), tree clean, `phase1-done` tagged. Docs and git
agree; nothing to fix.

**Then the phase-start rule: read the surface.** Phase 2's surface is not a tool catalogue —
it is what a real model does when asked for a real song, which nobody here had measured. Four
captures against `gemma4:12b-32k`, the model this app recommends for lyrics, with a system
prompt assembled the way ARCHITECTURE 6 specifies.

**The headline is that the phase's obvious implementation would have shipped broken.** A full
brief with a 2000-token budget returned **85 characters of lyrics and 7458 characters of
reasoning**, `finish_reason: length` — the song cut off eight words in — and the **first
content delta arrived 44.08 s into a 44.65 s stream**. Written the obvious way, the Lyrics
Studio shows an empty document for 44 seconds and then saves a truncated fragment as version 1.
None of those three failures is visible in code review; all three are visible in one capture.

**And the fix is not the one that looks right.** Ollama's own `think: false` is **accepted and
silently ignored** over the OpenAI-compatible endpoint — byte-for-byte the baseline, no error,
no warning — as is `reasoning_effort: "low"`. Only `reasoning_effort: "none"` works, and when
it does the same request is 6.6x faster and actually answers: 8.2 s, `stop`, ~400 completion
tokens, a complete `V-C-V-C-B-C` song. A client that had set `think: false` and moved on would
have believed thinking was off while paying for every thinking token.

**The limit of that finding is recorded with it.** `reasoning_effort` is verified against
Ollama and nothing else. Rather than defend an unverified path, the field is sent **only when
`thinks` is true** — a fact only the native enrichment can supply, which means it is only ever
sent to the endpoint it was verified against. The Phase 1 capability flags turned out to have
a second job.

**One finding came from the other side of the app.** `TextEncodeAceStepAudio1.5.lyrics` is a
bare STRING: empty `choices`, empty description. **Nothing in ComfyUI publishes the structure
tags ACE-Step accepts**, and the shipped template's own example numbers its verses
(`[Verse 1]`) while the profile declares `[Verse]` — so a literal validator would reject the
model's own example, and any blocking validator would enforce a guess against the user's
lyrics. Advisory, numbering-tolerant. It is still load-bearing: with the profile's
"cues belong in tags, not lyrics" rule stated plainly in the system prompt, the model wrote
`[Vocal style: ethereal, airy]` into the lyrics in **every single capture**.

**Planned, not briefed.** [tasks/phase-2.md](tasks/phase-2.md) carries T-201 … T-211 with the
four findings folded into the tasks they change, plus two decisions taken at the boundary: one
JSON file per lyric document (ARCHITECTURE 8's `lyrics/<v>.md` sketch predates `LyricDoc`), and
the brief's `language` as a writing instruction rather than a slot value, so the Lyrics Studio
form needs no running ComfyUI. **T-201 is the store**, first because `LyricDoc` has existed
since T-003b with nothing to persist it and Phase 3's provenance points at a `LyricRef` that a
Zustand store cannot keep.

**Handoff state:** gate green, docs committed, no brief written yet. ComfyUI is up (core is one
release behind, v0.33.4 vs v0.34.0 — worth updating before Phase 3 touches the pipeline, it does
not affect Phase 2). Ollama is up with 13 models.

### 2026-08-25 (later still) — T-201 landed whole: the project and lyric store

**Briefed as three, landed as one.** T-201a's brief was written the usual way -- reference
code compiled, `cargo fmt` clean, clippy clean, gate green, guards mutation-tested -- and
by the time it was finished the same was true of T-201b's and T-201c's code, because
deriving the brief meant writing the store. The producer's call was to land T-201 whole as
architect work rather than spend three executor runs transcribing code that was already
verified. [tasks/t-201a-brief.md](tasks/t-201a-brief.md) stays as the design record for the
split.

**What landed.** `library::atomic` (one `write_json`, now shared with `config::save`, which
had the only other hand-rolled rename dance in the crate), `library::projects` (slug rules,
path safety, create/save/load/list) and `library::lyrics` (one JSON file per document).
Plus, in `create-core`: `Project::new`, `Project::next_lyric_seq`, and
`LyricDoc::push_version`/`approve`.

**Three decisions inside it worth naming, because each prevents a specific wrong answer:**

- **Lyric ids come from a counter on the project, never from the files present.** Deriving
  the next id from the surviving documents hands a deleted document's id to a later one,
  and Phase 3's provenance `LyricRef` then resolves to lyrics written for a different song.
  The counter is monotonic and `#[serde(default)]`s to 1, so older project files still load.
- **`list_docs` walks `Project::lyrics`, not the directory.** A stray file left by a failed
  write is therefore invisible rather than appearing as a document, and an id with no file
  behind it becomes a warning rather than a silent omission -- the user is told a document
  is missing instead of being shown a list that quietly lost one.
- **Slugs and doc ids are validated against a whitelist**, because both arrive from the
  frontend and `..` or a separator would otherwise reach outside the library. A project
  named `CON` still gets a creatable directory, which on Windows it would not otherwise
  have.

**Nine mutations, nine caught -- but only after one was rewritten.** `push_version`
numbering from `versions.len() + 1` survived the first pass: it is indistinguishable from
numbering off the highest version until a document has a gap, and the test had no gap in
it. That is the same shape as the four fixtures-that-could-not-fail from T-110 and T-112,
found the same way. The replacement test starts from versions numbered 1 and 5.

**Next:** T-202, the brief type and system-prompt assembly, which is where the captured
prompt shape from this session's verification turns into code.

### 2026-08-25 (later still) — T-202 briefed in two, after the prompt was measured rather than argued about

**The brief needed a prompt, and a prompt is a third-party surface.** ARCHITECTURE 6 says
what goes in the system prompt; it does not say whether any of it works. So the assembled
prompt was run against `gemma4:12b-32k` before the brief was written -- and the run that
mattered was the one testing the line I had added myself.

**The obvious fix made it worse.** The model writes production and vocal-style cues into
the lyrics, which the profile's own contract forbids, so the prompt gained a hard rule
against it. Over **14 live generations** across three prompt variants, the runs carrying
that rule averaged **more** stray direction blocks than the runs without it -- 5.7 against
3.4. Per-run counts swung 0 to 10 on identical prompts, so the ordering is inside the
noise; what is not inside the noise is that the rule never helped in any grouping. Naming
the forbidden thing appears to prime it. The rule is gone, and
`test_no_rule_against_production_directions` exists so it cannot come back on intuition.
That is now a repo rule: **a prompt change is a change to a third-party surface and gets
measured like one.**

**The same runs sized T-203.** No run ever broke the requested order, and the additions
were smaller and more uniform than first written: one extra `[Outro]` in 9 of the 13 saved
generations, nothing else. So the lint can check "the requested sections appear, in order"
and must not fail a lyric for "and nothing else".

**One mutation survived, for the third time in the same shape.** The test asserting that
structure tags come from the profile passed with the tag line deleted entirely, because
both shipped profiles carry a worked example that already contains their tags -- the
fixture was already in the state that made the rule invisible. Same failure as T-110's
freshness block, T-112's sorted model list and T-201's contiguous version numbers. The
test now pulls out the tags line and asserts on that.

**Briefed in two** because the file is 567 lines: [T-202a](tasks/t-202a-brief.md) is the
brief type, `expand_structure` and `token_budget`; [T-202b](tasks/t-202b-brief.md) is the
assembly. Seven mutations, seven caught after the fix. Reference code compiled, fmt and
clippy clean, gate green.

**Handoff state:** the reference implementation is **not** in the working tree -- it is
saved under the session scratchpad, and the briefs carry it verbatim. Say the word if you
would rather land T-202 directly the way T-201 went, and it comes back in one step.

### 2026-08-25 (later still) — T-202a/b reviewed and landed; the first run in this phase to go through the executor

**Both briefs came back faithful.** Against the reference the only content differences were
a reflowed module doc comment, `use crate::profile` sorted before `use serde`, and the two
tasks' tests sitting in a different order within `mod tests`. Nothing semantic. That is the
loop working exactly as WORKFLOW describes it: the expensive thinking happened while the
brief was being written, and the run was transcription.

**One defect, fixed directly rather than re-run.** Both files arrived **CRLF** while every
other source file in the repo is LF, which turned a faithful transcription into a 566-line
phantom rewrite in `diff`. This is the failure WORKFLOW section 2 already documents from
T-002, and the reason it says to check the line endings before reading a large diff as a
rewrite -- it cost a minute here because that note exists. Normalised both files; the
committed content is unchanged either way, since `.gitattributes` pins `eol=lf`.

**The ASCII rule held with no review round.** First task in five where it did not have to be
raised. Worth noting because the backlog item to enforce it in `npm run gate` was written
when it was failing every time; the case for it is now weaker, not stronger.

**Verified rather than assumed, on the two things that matter:**

- All 14 tests present and green, and the **eight mutations from the acceptance criteria
  re-run against the landed file** -- eight caught. Including the one added during review:
  making whitespace a separator again in `expand_structure`, which would silently split a
  section the user named "Spoken word" into two.
- The assembly functions are byte-identical to the reference, so the prompt this code
  builds is the same string that was measured over 14 live generations. No re-run needed to
  know that; the diff is the proof.

**Next:** T-203, the lint. It is the only defence against the stray production directions,
now that the prompt has been measured and cannot suppress them.

### 2026-08-25 (later still) — T-203 briefed in two; the corpus broke my own rule

**The lint had a corpus to be built against, and that changed it.** The 13 generations
saved from T-202's prompt runs are the only real evidence anywhere of what this model puts
in a lyric, so the lint was written, then run over all 13 before the brief was finished.

**It caught a defect I had reasoned my way into.** The scanner's first rule was "a line is
structure only if it is nothing but bracket tokens", which is tidy and wrong: one
generation wrote every direction as `[Verse] (dreamy female vocals, ethereal synth pads)`,
and that file -- a correctly structured song, one of only three in the corpus with no
bracketed strays -- came back reported as having **no structure at all**. The best-behaved
file in the corpus got the worst possible answer. The scanner now reads the leading tags
and reports the trailing text as its own finding, `TextAfterTag`, and that generation is
`testdata/lyrics/generated-parenthesised-directions.txt`.

**The corpus also corrected something I had already written down.** Three committed
documents said every run added 2 to 4 sections beyond the six requested. Counted properly,
excluding `[inst]` markers and per-occurrence double counting: an extra section appeared in
**9 of 13**, and it was **always an `[Outro]`**; the other 4 matched the brief exactly. The
design conclusion is unchanged -- an extra section is Info, not a warning -- but it now
rests on 9 of 13 rather than on "no lyric passes". Corrected in LLM-SURFACE 12.5, the
decisions log and the phase file.

**A fourth thing the corpus settled:** across 99 declared tags the model **never once
numbered one**. So the lint's numbering tolerance is not for the model at all -- it is for
the shipped ACE-Step template, which writes `[Verse 1]`, and for songwriters who number out
of habit. That is now written on the function, because otherwise it reads as dead code.

**Verification:** eight mutations, eight caught after one rewrite (the trailing-text rule
had an untested branch), and a sweep over all 13 files whose counts matched an independent
analysis exactly -- 46 unknown tags, order clean 13 of 13, extra section in exactly the 9
that added an outro, and no file misread as untagged.

**Briefed in two** because the file is 663 lines. Reference implementation is in the
session scratchpad, not the working tree; the three fixtures **are** committed, since they
are captured data rather than executor output.

### 2026-08-25 (later still) — T-203a/b landed directly; the executor produced an empty file

**Both Aider runs produced nothing.** The working tree came back with a single untracked,
**0-byte** `crates/create-core/src/lyrics/lint.rs` and no `pub mod lint;` line in `lyrics.rs`
-- the executor created the file and wrote nothing into it, so the crate never compiled it and
the gate passed vacuously. This is the first run to fail this way; every prior run at least
transcribed. The producer chose to land T-203a/b directly as architect work rather than re-run
(the same call made for T-201): the reference code in the two briefs was already compiled,
`cargo fmt`-clean, clippy-clean, mutation-tested and swept over all 13 saved generations, so a
retry would have been transcribing already-verified text.

**What landed.** `create-core/src/lyrics/lint.rs` (653 lines): the tag scanner
(`split_tag_line`/`scan_tag_lines`/`normalize_name`), the two direction rules
(`UnknownTag`/`TextAfterTag`), and the three structure rules (`MissingSection`/`OutOfOrder`/
`ExtraSection`) with severities set by the corpus -- extra section is `Info` because 9 of 13
generations added an `[Outro]`. `LintSeverity` has no `Error` variant and nothing here edits or
blocks, per the phase-boundary decision. `create-core` 41 -> **74 tests** (15 new, 12 + 3).

**Guards re-armed against the landed tree, not just re-asserted from the brief.** Because the
file was assembled by hand from two briefs rather than copied from one scratch file, two
mutations were re-run: `ExtraSection` -> `Warning` fails only
`test_an_added_outro_is_info_not_a_warning`, and gating the order check on `false` fails only
`test_out_of_order_is_reported_when_nothing_is_missing`. Both reverts done with the edit tool,
not regex (the T-104b lesson).

**Line endings were clean this time.** The new file is LF-only on first check -- the CRLF
phantom-rewrite that cost a round in T-202 did not recur, because the architect wrote it, not
the executor.

**Next:** T-204 (`llm-bridge::reasoning_effort`), the last piece before the Tauri streaming
command (T-205).

### 2026-08-25 (later still) — T-204 landed directly; the policy field, with its evidence kept live

**Producer's standing call applied:** a task this small, written and tested by the architect,
does not get an executor round trip. T-204 added one `Option<String>` field to `ChatRequest`
plus a live test, so it was landed directly (like T-201 and T-203 before it).

**What landed.** `ChatRequest.reasoning_effort: Option<String>`, `#[serde(default,
skip_serializing_if = "Option::is_none")]` so it is absent from the wire unless set. The field
carries the policy as a doc comment: the one value verified is `"none"`, against Ollama only
(LLM-SURFACE 12.2), and it must be set only when the model is known to think -- an endpoint the
app has not verified may reject an unknown field rather than ignore it. `stream_body` needed no
change: it serialises the request whole, so the field is included/omitted automatically.

**The type is `Option<String>`, not an enum, and that is deliberate.** The value is a
provider's vocabulary string forwarded verbatim (the same treatment `finish_reason` already
gets), not this app's own vocabulary, and the app only ever sends `"none"`. A one-variant enum
would model the single verified value but force a rename if a future provider surfaces a
different effort level; the policy that constrains *when* to send it lives in T-205's call site,
not in the wire type.

**Two tests, both aimed at the two halves of "omitted when unset":** `test_reasoning_effort_is_sent_when_set`
guards that `Some("none")` reaches the wire, and `test_unset_options_are_omitted_not_null` was
extended to guard that `None` does not (a mutation that dropped `skip_serializing_if` serialised
`"reasoning_effort": null` and failed exactly that test). The existing `llm_test` call site
deliberately leaves it `None` -- the wizard's test call *wants* to see reasoning, because
`saw_reasoning` is what proves the reasoning split works.

**The evidence is kept live, not just recorded.** `test_live_reasoning_effort_none_suppresses_reasoning`
(`#[ignore]`) asserts that `"none"` leaves a thinking `gemma4` model with empty reasoning and a
real answer, which is the whole reason the field exists and the one thing no offline test can
show. `llm-bridge` 34 + 3 -> **35 + 4 live**.

**Next:** T-205, the Tauri lyric streaming command and event pump, where the
`reasoning_effort` policy (send only when `thinks`) is actually applied.

### 2026-08-25 (later still) — T-205 landed directly; the streaming command applies the policy T-204 only added

**Landed directly, on the same call as T-204.** This is the larger half of the pair -- a new
`src-tauri/src/lyrics.rs` (381 lines) plus three small wiring edits -- but it is all internal
wiring over surfaces already verified: the pump is modelled on `jobs.rs`, the streaming and
enrichment on `llm.rs`, and the event names (`lyrics://*`) use the same charset T-104c verified.
No new third-party surface, so there was nothing to capture live.

**What landed.** `lyrics_generate` / `lyrics_cancel`, `LyricsState` (a single abort handle --
one generation at a time, a second generate aborts the first), and the four events
`lyrics://delta` (content only), `lyrics://thinking` (reasoning), `lyrics://done { finish_reason,
usage }`, `lyrics://failed`. `TokenUsage` gained a re-export in `llm-bridge` so the `done` payload
can carry it, and `llm::enrich` became `pub(crate)` so the command can read the `thinks` flag.

**The `reasoning_effort` policy is applied here, not in T-204.** `reasoning_effort_for(thinks)`
returns `Some("none")` only when `thinks == Some(true)`, and `thinks` is read fresh from the
Ollama enrichment (`model_thinks`) rather than trusted from the frontend -- a model set can
change between the wizard and a generation. Unknown (`None`) and non-thinking (`Some(false)`)
both leave the field unset, so the unverified path is never taken.

**A refusal is a failure, not silence and not content.** `ChatDelta::Refusal` was the one delta
the phase file did not name. Routing it to `content` would put the model's "I can't write that"
into the lyric; ignoring it would emit a clean `done` with an empty document. It becomes
`lyrics://failed` with the model's own wording, which is what the user needs to see.

**Guards armed by mutation, two of them:** widening `reasoning_effort_for` to always send fails
only `test_reasoning_effort_is_sent_only_when_the_model_known_to_think`, and dropping the
`finish_reason` assignment fails only `test_stream_emits_deltas_and_returns_done_with_reason_and_usage`
-- the finish reason reaching the frontend intact is the one thing the phase file calls out, and
it is enforced rather than asserted. `app` crate 26 -> **31 tests** (5 new).

**One type change worth recording:** `stream_lyrics`/`pump_lyrics` are generic over
`impl Stream<Item = Result<ChatDelta, LlmError>>` rather than naming `BoxStream`, because the
shell crate has no `futures-core` dependency (only `futures-util`). Naming the concrete type
would have added a dependency for no behaviour.

**Next:** T-206, the frontend bridge and lyrics store, which consumes these events and is where
the event-name spelling (`lyrics://*`) first has to agree across the Rust/frontend boundary.

### 2026-08-25 (later still) — T-206 landed directly; the store the events stream into

**Landed directly, as with T-204/T-205.** Four new files, no changes to existing code:
`bridge/lyrics.ts` (the typed wrappers + wire types), `state/lyrics.ts` (the store), and a test
for each. vitest 51 -> **64 tests** across 10 files.

**The two halves of the streaming state are split the way the events themselves are.** `Content`
folds into `draft`, `Reasoning` into a bounded `thinking` trace (last 50 deltas, so a model that
thinks for 44 seconds cannot grow memory without bound), and `done`/`failed` are terminal. The
`truncated` flag is set from `finish_reason === "length"` and nothing else -- the signal the
truncation banner (T-208) reads, so it is folded rather than swallowed. `applyLyricEvent` is a
pure function of `(snapshot, event)`, the same seam `state/jobs.ts` uses, so it is tested without
a store or a bridge.

**The event-name spelling is now pinned on the frontend side.** `bridge/lyrics.test.ts` asserts
`subscribeLyrics` registers exactly `lyrics://delta|thinking|done|failed` -- the same four names
T-205 emits. This is the cross-boundary string T-104c warned about: pinned by a mock, not a
shared fixture, and the only thing a producer live smoke check (T-211) can confirm end to end.

**"version list, approve" moved to T-209.** The phase file listed them under T-206, but a
version's `LyricSource::Llm { model }` needs the model name, which the backend reads from config
(not passed back on the events), and persisting a `LyricDoc` needs Tauri commands that wrap
`library::lyrics` -- neither exists yet. They belong with T-209's versioned editor, which now
explicitly carries them plus that wiring. Recorded in the phase file so there is no drift.

**Two test-infra facts worth keeping:** `vi.hoisted` is required for any non-`vi.fn()` value the
`vi.mock` factory references -- a plain `const` object hit "Cannot access before initialization"
because `vi.mock` hoists above it while `vi.fn()` calls are hoisted with it. And the `mock*`
prefix convention (from T-104c) applies only to `vi.fn()`s and `let` flags, not to data literals.

**Next:** T-207, the LyricsStudio brief form, which binds the store's `brief` to actual inputs.

### 2026-08-25 (later still) — T-207 landed directly; the brief form, and two gaps it exposed

**Landed directly, as with T-204-T-206.** The first UI of the phase: `LyricsStudio.tsx` is now a
brief form over the T-206 store (`setBrief` per field), with a structure picker, a plain-text
language field, and one primary action (Generate). vitest 64 -> **70 tests**; `app` crate 31 ->
**33 tests**.

**Two pre-existing gaps the form exposed, both fixed here rather than worked around:**

1. **`default_profile_id` was never read at runtime.** `load_config` existed but no production
   code called `load()` -- the config store was only ever exercised by tests and the wizard's
   `save_config`. The brief form needs a selected profile to prefill from, so `App.tsx` now
   loads config once on mount, and the form falls back to `DEFAULT_PROFILE_ID`
   (`ace-step-1.5-turbo`) when none is configured. This is the same class of gap T-006 caught in
   Phase 0 -- a command existed with no caller.
2. **The profile's `prompt_guide` reached the frontend nowhere.** `models_status` returns
   readiness rows, not the authoring guide, so a new `profile_guide` command returns
   `display_name` + `tag_style` + `examples`, and the form prefills `style_tags` from the first
   example. This matters because the two shipped profiles disagree about what a style tag is --
   ACE-Step wants comma-separated short tags, MiniMax wants a structured caption -- so the
   prefill must come from the profile, never a constant.

**The prefill respects "never modify the user's words."** `prefillFrom` replaces the `style_tags`
only when it still equals the built-in default; an edit the user already made is left alone. A
test pins it (and the `styleTagsFromGuide`/`structureOptions` helpers are pure and tested).

**The form is verified by `tsc`/`oxlint`/build and the store tests; its rendering is not.** The
component logic is thin (store projections), and per WORKFLOW 5 the visual claims are for the
producer's click-through, not something I can composite here. The Generate button is wired and
disabled while `generating`, but the streaming result is T-208.

**Next:** T-208, the generation UI -- streaming the draft, the thinking trace, cancel, and the
truncation banner.

### 2026-08-25 (later still) — T-208 landed directly; the generation UI renders what the model is already sending

**Landed directly, as with T-204-T-207.** Frontend-only: a `LyricOutput` section in the
LyricsStudio below the brief form, plus two pure helpers in the store. vitest 70 -> **76 tests**.

**The one idea this task exists for: reasoning is proof of life.** `generationPhase` derives
`starting -> thinking -> writing` from the snapshot, with `thinking` (reasoning flowing, no
content yet) taking precedence over `starting` because that window is the 44 seconds during
which an otherwise-healthy generation shows nothing (LLM-SURFACE 12.1). `thinkingTail` shows the
newest reasoning on the status line -- the fix is the content the model is already sending, not
a spinner. The `thinking`-beats-`starting` guard is armed by mutation (collapsing it fails
`test_starting_then_thinking_then_writing` and nothing else).

**The truncation banner is a suggestion, not a second action.** The frontend cannot raise
`max_tokens` (the backend computes it from the brief via `token_budget`), so the banner says
"try a longer length, then generate again" and the retry is the existing Generate button plus
the Length field. This is the honest shape of "retry with more budget" given where the budget
actually lives.

**Rendering is producer-click-through, as always.** The draft, status, cancel button and banner
are thin store projections verified by `tsc`/`oxlint`/build; the visual claims are WORKFLOW 5
territory. The cancel button reuses the existing `.job-cancel` style rather than a new token.

**Next:** T-209, the versioned editor -- which also wires `library::lyrics` to Tauri commands
(`create_doc`/`save_doc`/`list_docs` have no frontend path yet) and carries the version
list/approve deferred from T-206.

### 2026-08-25 (later still) — T-209 landed directly in three parts; the versioned editor and the lyric store wiring

**The largest phase-2 task, landed in three commits** (T-209a backend, T-209b store, T-209c
UI), each gate-green. `library::lyrics` finally has a frontend path, the version list/approve
deferred from T-206 landed, and the LyricsStudio draft became an editor.

**T-209a -- the seam.** `lyrics_open` / `lyrics_save` / `lyrics_lint`, plus a `default_project`
helper that creates "My First Song" on first use and reuses it after -- there is exactly one
project and one working document until Phase 4. `lyrics_save` validates the doc id against the
whitelist before it touches a path, so a bogus id from the frontend cannot write outside the
project. `lyrics_lint` runs `create-core`'s lint against the profile + brief and returns an empty
vec on a missing profile -- the lint is advisory, and nothing to check against is not a fault.

**T-209b -- the store.** `bridge/lyricdoc.ts` mirrors `LyricDoc`/`LyricVersion`/`LyricSource`/
`LintFinding` (severity re-derived frontend-side, since the backend's `severity()` is not on the
wire). The store gains `doc`, `findings`, and the version actions: `loadDoc`, `commit(source)`,
`commitGenerated` (auto on `done`, reading the model from config), `saveDraft` (human without
versions, edited with), `restore`, `approve`, `lint`. The handoff is `approvedText(doc)` -- a pure
selector Phase 3's AudioStudio reads, no navigation side effect.

**Two design decisions worth naming.** (1) **Generation auto-commits** on `done` with source
`Llm`, model read from `useConfigStore` -- the alternative (an explicit "save generation" step)
adds a click for the common case. (2) **`LyricSource::Edited` is chosen by `saveDraft`**, not by
the component: an edit of an existing version is `edited from vN`, typing into an empty document
is `human`, and the branch lives in the store where it is tested.

**T-209c -- the UI.** The draft is now a textarea; the version list shows number, source label,
a one-line preview, and per-version Restore/Approve; lint findings render as warning vs info
advisories; Save and Check buttons sit under the draft. The approve handoff is a store action
(`approve` sets `doc.approved`, `approvedText` exposes it). One layout note: `saveDraft`'s
"human vs edited" test is the one that pins the `LyricSource` rule.

**Test counts:** `app` crate 33 -> **35** (2 backend), vitest 76 -> **87** (11: version helpers,
the document-store actions, `saveDraft`). `tempfile` added to `src-tauri` dev-dependencies -- the
only new dependency, already in the workspace via `library`.

**Rendering is producer-click-through.** The editor is thin store projections, verified by
`tsc`/`oxlint`/build and store tests; the visual claims are WORKFLOW 5 territory. The
end-to-end flow (generate -> edit -> approve) is exactly what T-211's live milestone checks.

**Next:** T-210, the consent-gated prompt optimizer and the shared `<PromptDiff>` component.

### 2026-08-26 -- T-210 landed directly in two parts; the consent gate is the whole feature

**Landed directly, as with T-204-T-209**, in two gate-green commits: T-210a (backend),
T-210b (the diff, the component and the store). The last piece of Phase 2 before the live
milestone.

**What is optimized is the assembled brief, not the lyrics.** T-202 chose labelled lines for
the user message partly so this diff would read well, and this is the task that spends that:
the optimizer is handed the assembled brief, told which four lines it may rewrite (Theme,
Genre and style tags, Mood, Era and references) and which five it must reproduce word for
word (Structure, Language, Point of view, Explicit content allowed, Target duration), and its
answer comes back beside the original. `test_every_brief_label_is_classified_as_rewritable_or_fixed`
walks the real `assemble_user_message` output and fails if a brief field ever reaches the
model with no rule attached -- the mutation that matters, since adding a field is the way this
quietly breaks.

**No backend check enforces the fixed lines, and that is the design.** The obvious addition is
a validator that rejects a rewrite touching Target duration. It would be a second answer to a
question the consent step already answers: a model that rewrites a settings line produces a
highlighted change the user has to accept before anything is sent. The diff is the gate, so
the diff had better be honest -- which is what the one test the whole component rests on is
for. `test_panes_reassemble_both_texts_exactly` asserts each pane's spans concatenate back to
the exact text; a diff that renders a lossy picture of the prompt would have the user
accepting something other than what gets sent.

**A word diff, not a line diff.** The optimizer rewrites the middle of a labelled line, so a
line diff reports every touched line as wholly replaced and shows the user nothing. LCS over
word-and-whitespace tokens, spans merged so a rewritten phrase is one highlight, with a 1500-
token ceiling that degrades to a whole-text replacement rather than allocating a table sized
by a paste.

**Three product rules that only look small.** (1) Editing any brief field drops an accepted
prompt -- it was written against the old brief, and keeping it would leave the form describing
one song while Generate sent another. (2) `prompt_optimized` records the *accepted* prompt,
not that the optimizer ran; a reverted rewrite never reached the model and a sidecar claiming
otherwise is a false record. (3) One rewrite in play at a time: Optimize is disabled while a
proposal is on screen or a prompt is accepted, so the text being replaced is always the one
the user can see. All three have tests; (1) and (2) are the ones that would have shipped
wrong.

**`<PromptDiff>` knows nothing about lyrics** -- two strings and three callbacks, plus
optional pane labels and a note. Phase 3's audio tags use it unchanged, which is what the
phase file asked for.

**One naming collision worth remembering:** `promptDiff.ts` beside `PromptDiff.tsx` fails
`tsc` on a case-insensitive filesystem (TS1149/TS1261) even though both files exist happily on
disk. The pure module is `wordDiff.ts`.

**The optimizer prompt has never met a model.** The lyric prompt was captured working before
it was written down; this one is a first draft, said plainly in its module docs so the
confident wording is not mistaken for evidence. T-211 now carries a measurement as well as a
click-through: does the rewrite come back as the same labelled lines, does it reproduce the
five fixed lines, and does the rewritten brief actually produce a better song.

**Test counts:** `app` crate 35 -> **41** (6), `create-core` 74 -> **80** (6), vitest 87 ->
**101** (14). No new dependencies.

**Rendering is producer click-through.** The diff panes, the edit toggle and the accepted
banner are verified by `tsc`/`oxlint`/build and the pure tests; the visual claims are
WORKFLOW 5 territory.

**Next:** T-211, the Phase 2 live milestone -- and the close of the phase.

### 2026-08-26 (later) -- T-211 part one: both automated measurements run, both pass

**The two halves of the milestone that could be automated were, and they were run against
`gemma4:12b-32k` on the local Ollama.** Steps 2, 3 and 5 -- the click-through and the on-disk
check -- remain for the producer; the checklist is in [tasks/phase-2.md](tasks/phase-2.md).

**The optimizer prompt is now a measured surface.** Five round trips: the rewrite came back as
a well-formed brief in **5 of 5**, and the five fixed lines (Structure, Language, Point of
view, Explicit content allowed, Target duration) were reproduced word for word in **5 of 5**.
No truncation, no commentary, no fences, 3.4-3.6 s per call. `OPTIMIZER_MAX_TOKENS = 1024` was
a guess and is now measured adequate. The module docs no longer say the prompt is unverified,
because it no longer is -- leaving that caveat in would have been as misleading as the
overconfidence it was written to prevent.

**The first run of that harness failed, and the harness was what was wrong.** It reported
"labels intact 0/5". The cause: the model **adds** an `Era and references` line when the brief
leaves that field empty, in 5 of 5 runs -- and the check tested the label list for equality.
Era is on the rewritable list, the added line is well-formed, and it reaches the user as an
added line in the diff, so that is the optimizer doing its job on a field the user left blank.
`LabelReport` now separates the three failures that actually make a rewrite undiffable
(dropped, invented, shuffled) from the one that does not. **Second time this phase that a rule
about model output had to be run against model output before it was right** -- the lint
scanner was the first. The lesson is not "write better checks", it is that this class of check
cannot be finished offline.

**The `reasoning_effort` policy works end to end, and the numbers are not close.** Three live
lyric generations from the real assembled prompt: 1333 / 1726 / 1592 characters,
`finish_reason: stop` every time, **0 characters of reasoning**, first content delta at
**0.19-0.43 s**. The baseline this replaces was 85 characters and a first delta 44.08 s into a
44.65 s stream (LLM-SURFACE 12.1). Whole generation 6.9-9.2 s, consistent with the 8.2 s
recorded at the phase boundary.

**The lint earns its place.** Over those three generations it made 4, 6 and 1 findings, every
one a production cue the model wrote inside the lyrics -- `[dreamy female vocals]`,
`[driving beat]`, `[slow build]`, `[fade out]` -- plus an `ExtraSection` for `[Outro]`. Stray
directions in **3 of 3**, against the 10-of-13 rate measured at the phase boundary. The model
is still breaking the profile's own contract with the contract stated plainly in its system
prompt, which is exactly why T-202b's decision to catch this after generation rather than
forbid it in the prompt stands.

**Both measurements are ignored tests, not scripts**, so they live with the code they measure
and are re-runnable after any prompt, lint or policy change:
```
cargo test -p app -- --ignored optimizer --nocapture
cargo test -p app -- --ignored lyric_generation --nocapture
```

**Next:** steps 2, 3 and 5 -- the producer's click-through, and the on-disk check that
`prompt_optimized` records consent. Then `phase2-done`.

### 2026-08-26 (later still) -- T-211 steps 2, 3 and 5: the click-through, and what it found

**The producer ran the three steps no test can make.** Everything the phase set out to build
works: generation, editing, versioning, the lint, approve, the optimizer diff, accept, revert,
the brief-edit reset, and the on-disk record.

**Step 5 is the one worth reading in full**, because it is the T-210 claim only a real run
proves. The document on disk after the session:
```
approved: 3
v1 {"kind":"llm","model":"gemma4:12b-32k","prompt_optimized":false}
v2 {"kind":"edited","from_version":1}
v3 {"kind":"llm","model":"qwen3.5:397b-cloud","prompt_optimized":true}
```
v1 generated before optimizing, v3 from a prompt the user accepted, and the flag records the
difference. **`Edited` carries no `prompt_optimized` and should not** -- the producer flagged
its absence on v2 as possibly wrong. v2's provenance is "edited from v1", and v1 carries its
own flag; copying it onto the edit would be the two-sources-of-truth hazard the sidecar rules
exist to prevent. The chain is the record.

**The lint's hit rate is model-dependent, and the spread is large.** `gemma4:12b-32k` produced
**8 findings** on one generation -- seven production cues written inside the lyrics plus an
extra `[Outro]`. `qwen3.5:397b-cloud` on the same brief produced **none**. Both are healthy
outcomes and neither is a bug: the phase-boundary measurement (10 of 13) and the T-211
measurement (3 of 3) were both gemma4. The lint is not dead code for a well-behaved model, and
it is not noise for a badly-behaved one -- but "the model writes production cues" is a fact
about a model, not about models, and Phase 3's profile docs should not state it as universal.

**Three UI defects, all found by clicking, all fixed in T-213.** Each is worth naming because
none of them could have been caught any other way:
1. **The draft textarea rendered black on the dark ground.** A textarea does not inherit the
   page colour, and `.lyrics-draft` never set one. Every other textarea in the app happens to
   sit on a class that does.
2. **The approval notice was invisible in practice, while being correct in code.** It rendered,
   `approvedText` is right and tested, and the badge proved `doc.approved` was set -- but it sat
   at the foot of the panel *below the lint findings*, so a lyric carrying eight advisories
   pushed it off screen. The producer reasonably concluded the feature did not exist. **A
   correct component in the wrong place is indistinguishable from a missing one**, and no test
   in this repo can see layout.
3. **A check that found nothing said nothing.** Empty findings meant both "clean" and "never
   ran". A `linted` flag separates them, and any draft change clears it -- findings carry line
   numbers, so a stale one points at a line the user has since edited.

**Still open before `phase2-done`:** the producer re-runs steps 2 and 3 against the T-213
fixes. Nothing else is outstanding.

**One thing deliberately not built, now in the backlog:** the Lyrics Studio does not say when
the configured model is remote. The wizard does, which satisfies the 2026-08-24 decision as
written -- but this session generated an unreleased lyric on `qwen3.5:397b-cloud` from a screen
that never mentions it. Raised for the owner rather than decided at a phase close.

### 2026-08-26 (later still) -- T-214: the approval notice, and what two false reports were really saying

**Reported missing twice; rendering both times.** The document on disk carries `approved: 1`
with version 1 present and 1765 characters of text, so `approvedText` returns a string and the
line was on screen for both reports. Two causes, and the first is mine:

1. **T-213 changed the copy and not the checklist.** The step said to look for the "ready for
   audio" line; T-213 had reworded it to "...is what audio will use", so the phrase existed
   nowhere in the app. A checklist and a UI that disagree turn a working feature into a failed
   step -- and the producer was right to report it as one.
2. **The treatment was too quiet to find.** T-213's fix was to move the line above the lint
   findings, on the theory that eight advisories had pushed it off screen. That theory was
   probably right and still insufficient: a small green sentence sitting among other small
   green sentences does not register. **Twice reported missing is a fact about the design, not
   the reader.**

Approval is a property of the document, so it now renders as a `vN approved` **status pill in
the output panel's header**, beside the generation status -- the same primitive the wizard
uses, in the one part of the panel that is always in view. The sentence stays, reworded to the
words the checklist actually uses.

**The general fix is `approvedLabel`.** Both rounds were correct logic derived inline in a view
no test can see. It is now a pure selector with tests, like `approvedText` and
`generationPhase` before it. The rule this phase keeps re-teaching: **anything a view decides
inline is a thing no test in this repo can check**, and the visible-state decisions are exactly
the ones worth pulling into the store.

**Two owner decisions recorded**, both from the same session: the remote-model disclosure stays
in the wizard only (closing the question T-211 raised), and the Gemma lyric recommendation goes
under review as OQ-7 -- it was measured with a hand-tuned system prompt this app does not
reproduce, and unaided against this app's own prompt, qwen wrote clean lyrics where gemma wrote
stray production cues.

**Phase 2 status:** steps 1, 3, 4 and 5 pass. Step 2 passes except the approval notice, which
is what this task fixes. One re-check of step 2.6 and the phase tags.

### 2026-08-26 (session close) -- Phase 2 complete, tagged `phase2-done`

**The milestone passed on the re-check.** The approval pill was found where T-214 put it; the
producer confirms the earlier line had been on screen and they were looking for different
words. All five T-211 steps pass. Phase 2 is closed and tagged.

**What Phase 2 shipped:** a brief form with profile-driven prefills, streaming generation that
shows the model's reasoning as proof of life, a versioned editor with restore and approve, an
advisory structure-tag lint, a consent-gated prompt optimizer with a shared `<PromptDiff>` that
Phase 3 reuses for audio tags, and one JSON file per lyric document carrying every version and
its provenance. Test counts across the phase: `create-core` to **84**, `app` crate to **41**
plus 2 live measurements, vitest to **109**.

**The lesson of the milestone, stated once so Phase 3 inherits it.** Steps 1 and 4 were
automated, passed first time, and found nothing. **Every defect came from a person clicking**,
and three of the four were invisible to `tsc`, `oxlint` and 109 tests because they were about
where something sat on screen and whether config had ever been written:
- **T-212**: the wizard let a model be picked and tested while never writing `config.json`. The
  test call could not catch it -- `llm_test` takes the endpoint as an argument, so it passed
  against a config that did not exist. Third instance in this repo of a command with no caller.
- **T-213**: a textarea rendering black on the dark ground; the approval notice below eight
  lint advisories; a clean check saying nothing.
- **T-214**: that notice again, reported missing while rendering, half of it caused by a
  checklist I reworded the UI out from under.

The recurring shape is **correct logic derived inline in a view**. `approvedLabel` was the
answer, as `generationPhase` and `approvedText` were before it: pull the decision into the
store where a test can reach it. Phase 3's param panel and LoRA stack are far more stateful
than anything here, so this is the habit to carry, not the exception.

**Open for Phase 3, in the order they will bite:**
1. **Re-verify the comfy-mcp surface against the live server before writing T-301.** It has
   moved once already, and the pipeline touches more of it than Phase 1 did.
2. **`SaveAudioAdvanced.format` is a V3 dynamic combo** and unresolved (MCP-SURFACE 5). The
   lossless save-node swap is a correctness requirement, not a nicety, so this blocks the
   pipeline rather than decorating it.
3. **OQ-7**: which model `data/lyric-llms.json` should suggest. Not a Phase 3 blocker -- a JSON
   and docs edit whenever the owner's comparison is done.
4. **`default_profile_id` is still never persisted**, the same class as T-212 but degrading
   silently to `ace-step-1.5-turbo`. Phase 3 owns the profile picker and should fix it there.

**Session ends here.** Working tree clean, gate green, everything pushed.
### 2026-08-27 -- Phase 3 opened: the surface re-verified, and the blocker cleared by running it

Session ritual first: PROJECT.md, ARCHITECTURE.md and the ROADMAP all agreed with `git log`
at `37cd446`, tree clean, in sync with origin. **No drift to fix** -- the first session in a
while where that was true on the first check.

**The verification pass, and what it changed.** Full evidence in
[docs/MCP-SURFACE.md 16](docs/MCP-SURFACE.md).

- **Versions:** comfy-cli **unchanged at 1.16.0** -- which is why the tool *names* held.
  ComfyUI went v0.33.3 -> **v0.34.1**. Worth knowing for the wizard's health pill: reading
  `freshness` with the server **down** reported `outdated: true`, and `launch_comfyui` then
  brought it up current, because the owner updates as part of launching. A stale reading on a
  stopped server is not a stale install.
- **What did not move:** the ACE-Step turbo template still `runnable: true` with **33 slots at
  identical addresses**, both duration slots and both seeds where the profile expects them.
  MiniMax still fails on exactly one hardcoded `fp16` filename with the same three
  `COMFY_MATCHTYPE_V3` warnings. Both shipped profiles are still correct as written.
- **What did move:** the LoRA list went **95 entries to 53** and the case-variant directory
  disappeared. The picker's design requirements are unaffected -- epoch checkpoints still
  dominate, `training_state.pt` files are still listed and still not loadable -- but
  ARCHITECTURE 5a's numbers were stale and are now marked historical. Also four tools exist
  that MCP-SURFACE 1 never listed (`discover`, `free_memory`, `upload_file`, a local
  `run_template`), `run_workflow` grew `confirm_spend`/`timeout_seconds`, and **`job` grew
  `action="error"`** -- a normalized failure view the queue panel should read instead of
  parsing status.

**The `SaveAudioAdvanced.format` blocker is closed, and the answer was not the expected one.**
It cannot be set through `set_workflow_slot`: the dynamic combo is not surfaced as a slot at
all, and the attempt errors `[workflow_slot_invalid]` naming the widgets that do exist. So the
format is a **graph edit** -- a positional `widgets_values` entry whose **array length varies
by format**, because `flac` has no sub-widget while `mp3`/`opus` carry a `quality` sub-combo.
`flac` is the only lossless option the node offers; there is no WAV, and no bit-depth control,
so the ceiling is 16-bit/48 kHz and the UI must not imply otherwise.

I did not stop at validating it. `validate_workflow` **documents dynamic-combo sub-inputs as
one of its own blind spots**, so a clean validate proves nothing about the format -- exactly
the "checked nothing, reported valid" hazard 9.3 already records in another form. So the swap
was run: template copy, node 107 retyped, 10-second generation, output parsed from the file
header -- `fLaC` magic, 48 kHz, 16-bit stereo, **480000 samples = 10.000 s exactly**, ratio
0.471. Real lossless, and the two duration slots stayed in sync through the edit.

**The finding that would have shipped a bug.** The MiniMax template carries
`SaveAudioAdvanced` **already configured to `mp3`/`V0`**. ARCHITECTURE 7 step 3 said the
pipeline intervenes where the template disagrees with the profile's `output` block and
described MiniMax as the one that "already ships `SaveAudioAdvanced`" -- a **node-class**
test. That test passes MiniMax and hands MP3 to the mastering chain, which is the precise
outcome the lossless rule exists to prevent. The condition is now the **format value**.
This is the same shape as T-212 and the two before it: a check written against the thing that
was easy to observe rather than the thing that mattered.

**Owner decisions this session.**

1. **No lyric-model recommendations at all** -- OQ-7 withdrawn rather than answered. The app
   is not a place to promote models; its users already have a go-to, many of them on APIs, and
   **assuming everyone runs Ollama is not logical**. What the app owes them is connecting to
   whatever they already use. For the record, his own finding was that local **qwen** is the
   best he has tested with no prompt adjustment -- but it does not become a suggestion.
2. **Testing another API is not a blocker.** Breadth of endpoint support is the goal; where a
   provider question arises, connect to one and find out. ComfyUI's HTTP API and any cloud
   endpoint are equally available -- **neither has ever been tested here**, only local stdio.

**A live consequence of decision 1 that needs deciding, not assuming.**
`reasoning_effort: "none"` -- the fix for a whole song arriving as 99% chain-of-thought -- is
sent **only where `thinks` is true**, and `thinks` exists only where Ollama's native
enrichment answered. That was a safe rule when Ollama was the assumed path. If most users are
on OpenAI-compatible APIs the app cannot enrich, then **most users never get the field**, and
the 44-second-before-first-token behaviour is theirs by default. Not a Phase 3 blocker, but it
is the first thing decision 1 breaks, and it belongs in the same task as the suggestion
removal. Recorded rather than fixed on a guess -- the repo's rule is that a prompt/parameter
change against a third-party surface gets measured, and the measurement here needs a
non-Ollama endpoint.

**State at close:** ComfyUI left running at 127.0.0.1:8188 (I launched it; `stop_comfyui`
ends it). No code changed -- docs only. `tasks/phase-3.md` does not exist yet; the briefs are
the next artifact, and they can now be written against a verified surface with the save-node
mechanism known rather than open.

### 2026-08-27 (later) -- Phase 3 briefed; T-301 landed, and my brief put the copy in a dead state

Phase 3's breakdown is written ([tasks/phase-3.md](tasks/phase-3.md)), **T-301 ... T-314**,
ordered so everything testable without a running ComfyUI comes first. T-301 is the first
task to land in the phase.

**T-301 -- the app recommends no lyric model.** The suggestion layer is gone whole:
`data/lyric-llms.json`, both `suggestions.rs` modules, `Suggested`/`MissingSuggestion`, the
"recommended for lyrics" chip, and the help text carrying an `ollama pull` command. What
survives is what was never a recommendation -- the remote-model privacy disclosure,
`Option<bool>` capabilities with unknown never rendered as false, and `preselect`'s
**settings** half (a configured model wins; an uninstalled one selects nothing), now three
tests in `src-tauri`. `app` crate 41 -> 43 tests, vitest 109 -> 108.

**Two things the brief found that the phase entry had not**, both by reading the code rather
than reasoning about it:

- **`DataDir` had exactly one consumer** -- the suggestion load. Left behind it is dead code,
  and the gate runs `clippy -D warnings`.
- **`bundle.resources` listed `"../data/*.json"`** while `data/` held only that one file.
  Deleting it leaves the glob pointing at a directory a fresh clone will not have, and
  **`npm run gate` cannot see it**: the gate runs `vite build`, never `tauri build`. Called
  out as a producer click-through item rather than trusted to CI.

**The defect in this task was mine, and it is the same shape as T-213/T-214.** I wrote the
new empty-state copy into the `not_configured` branch. `Setup.tsx` always calls `probe` with
a non-empty `DEFAULT_BASE_URL` constant, so **`not_configured` is unreachable from this
wizard** -- a user with no local model lands on `unreachable`, which showed only a raw
connection error. The executor transcribed the brief faithfully; the brief was wrong about
which state the user reaches. The guidance now renders on `unreachable`, where it also names
the address, because nothing else on screen reveals it.

Third time in three tasks that the defect was **a correct-looking thing attached to the wrong
state**, and the second time the state in question was one no test exercised because no test
could reach it. The Phase 2 lesson said to pull decisions into the store where a test can
reach them; this adds a corollary for the views that remain: **when writing copy for a state,
check the state is reachable.**

Also changed after the run: an unrequested sweep replacing every section sign in MODELS.md
with the word "section", reverted on rows the task never touched; `llm_probe`'s doc comment,
which described an `Err` arm for reading shipped data it no longer reads and can no longer
take at all; one `cargo fmt` miss on a call left multi-line after losing an argument -- the
sixth instance of that class; and a stale comment describing `preselect` as beating a
suggestion.

**T-301b is the task that actually delivers the owner's decision, and it came out of writing
T-301's brief.** `DEFAULT_BASE_URL` is hardcoded in five places and **the wizard has no
endpoint field at all**, nor one for the API key, though `has_key` already rides on
`LlmStatus::Ready` and `SecretKey::LlmApiKey` has been plumbed since T-004. So the LLM step
can only ever reach a local Ollama on the default port: a user on a hosted API cannot connect,
which is the one capability the owner named. Removing the suggestion list without this would
have left the Ollama assumption fully load-bearing and merely invisible. **Owner decision: the
field ships prefilled with the Ollama address** -- nothing regresses for local users, and a
prefilled field still shows everyone else what the app had been assuming.

**Outstanding for T-301:** the producer click-through in the brief -- `npm run dev` starts and
the step renders (the real check on the `tauri.conf.json` change), the model list and
disclosure still read correctly against a running Ollama, and the new copy appears with
nothing listening on 11434.

### 2026-08-27 (later still) -- T-301b landed; the executor built a test stack this repo does not have

The LLM step now has an endpoint field and a write-only API-key field, both persisted, so
the app can reach any OpenAI-compatible endpoint. Until today `DEFAULT_BASE_URL` was a
constant in five places with no way to point it anywhere else -- the load-bearing half of
the Ollama assumption, where the suggestion list T-301 removed was only the visible half.
Frontend only: `llm_probe`/`llm_test` already took `base_url`, the three secret commands were
already registered, `SecretKey::LlmApiKey` already whitelisted. vitest 108 -> 113.

**The executor's run did not compile, and the reason is worth keeping.** My acceptance
criterion asked for a test that the key input is never populated from the backend. It
answered by importing `@testing-library/react` and rendering the component -- inside a `.ts`
file, which cannot hold JSX -- against a repo that has **no DOM test stack at all**: vitest
runs in `node`, there is no jsdom, and none of the three packages it needs are installed.
Every existing test in this app is pure logic, by construction.

The executor was not wrong about what the criterion asked for. **The criterion asked for
something this repo cannot express**, and the honest version of it is a store selector --
`keyField(status)` -- with two pure tests, which is also exactly what this phase file's own
opening note demands. The write-only property itself is **guaranteed by construction**: no
Tauri command returns a secret value, and the input reads only from local draft state. There
is no code path a test could catch, so it is a review item, not a test. Saying that plainly
beats a test that appears to prove it.

**Second correction, and it is the same mistake twice.** I specified `not_configured` as the
state a cleared field produces. It is not: blank means the **default**, since the prefill is
the app's baseline and there is no useful state where the step points at nothing. As briefed,
clearing probed null while the sync effect refilled the box -- the message read "enter an
address" beside a filled-in address. One rule now. `not_configured` is therefore unreachable
again and is commented as type-complete only. **That is twice in two tasks that I put
user-facing copy into a branch nothing can reach**, which is why the corollary from the last
entry is now written into the brief itself rather than just the log.

**Outstanding, and it is the whole point of the task:** point the app at a **non-Ollama**
endpoint -- a hosted API with a key, or LM Studio -- and confirm the list and a test call.
That path has never been exercised in this repo's life. It is also what makes **T-302**
measurable, since Ollama's own cloud models still go through native enrichment and so do not
answer the question about endpoints the app cannot enrich.

### 2026-08-27 (later still) -- T-301b verified live: the app reached a non-Ollama endpoint

The click-through passed, including **the item that had never been exercised in this repo's
life**: a hosted OpenAI-compatible API listed its catalogue and returned a good test call.
The API-key badge survives a restart and offers Remove, and the key never reappears in the
input. T-301b is done.

**A lyric run on `qwen3.8-flash` produced the finding, and it is not the one the click-through
was looking for.** The model reasoned at length; the reasoning rendered as status text above
the editor; the lyrics then arrived clean in the box with **no reasoning text in the
document**. That is `ChatDelta`'s `Content`/`Reasoning` split (T-108) holding against a
**second provider's wire format** -- until today the rule that both spellings are read, and
that only `Content` may reach the user's document, was justified by one vendor's stream.
Another vendor's stream fed the same code and the document received only content.

It also confirms **T-302's premise and answers none of it.** `reasoning_effort: "none"` is
sent only where `thinks` is true, and `thinks` comes only from Ollama's native enrichment --
so on this endpoint the field was never sent, and the long think the producer sat through is
the unsuppressed default. The conservatism is now visibly costing something. What is still
unmeasured: whether that endpoint honours the field, ignores it the way Ollama ignores its own
`think: false`, or errors on it; which spelling its stream uses; and the first-content-delta
timing with and without. Recorded as [LLM-SURFACE 13](docs/LLM-SURFACE.md), with 13.1 naming
the three measurements. **T-302 is now cheap to run** and should go next, while the endpoint
is configured.

Open and small: the streamed reasoning renders between the approval badge and the lyric box,
which is where T-208 put it. With a model that thinks *a lot* that block can be long, and
nobody has yet looked at whether it should cap or scroll. Not reported as a problem -- noted
so it is looked at deliberately rather than discovered.

### 2026-08-27 (later still) -- T-302 measured: the conservative rule is costing real money

Ran against the endpoint T-301b made reachable. Harness committed as an `--ignored` test
beside T-211's (`cargo test -p app -- --ignored reasoning_effort --nocapture`); it reads the
endpoint from `config.json` and the key from the keychain, and **never prints the key**.
`reqwest` added as a **dev-dependency** of `src-tauri` for the raw-SSE half -- dev-only,
already in the tree via `llm-bridge`, nothing in the app talks HTTP directly.

**The answer is possibility 1 of the three: QwenCloud honours the field.** 33.12 s -> 1.13 s
to first content, 2771 -> 235 completion tokens, both runs a complete song. The rule that
sends `reasoning_effort` only to endpoints the app can enrich was written when the cost was
patience on a local model. On a paid endpoint it is **11.8x the billed tokens on every
generation**, and the user watches a model think for half a minute first.

**I did not change the rule, and the reason is the rule's own logic.** Two providers honour
the field; that is not evidence a third will not reject it, and an unsupported parameter sent
blindly turns lyric generation into an error for whoever's endpoint is strict. The move the
evidence actually supports is **discovering acceptance per endpoint instead of inferring it
from enrichment** -- and the app already makes exactly the right call to discover it in, the
wizard's test call. Proposed as **T-302b**, not taken unilaterally: it trades robustness
against cost, and the cost is the owner's.

**The second finding was not on the list.** QwenCloud streams **`reasoning_content`**, never
`reasoning`. Every prior capture was Ollama's `reasoning`. The 2026-08-24 rule to read both
spellings was written from documentation, defensively, with no provider in hand that needed
it -- and the first hosted endpoint this app ever reached needed it. Had it read only
`reasoning`, the producer's qwen run would have shown a blank panel for 33 seconds rather
than the thinking text they described. Worth recording in a repo whose habit is to delete
defensive code that never fires.

Also from the producer: the streamed reasoning **is already capped and scrolls**, so the
long-think case does not swamp the editor. Styling to make that reassuring rather than merely
tolerable goes to the backlog rather than a task. And QwenCloud publishes an
**Anthropic-compatible** endpoint alongside the OpenAI-compatible one -- which is the second
wire format `LlmProvider` has been deferred against since T-109, now available whenever that
question is picked up.

### 2026-08-27 (session close) -- T-302b verified live: 33 seconds became 1 to 2

The payoff T-302 measured is real in the app. The producer ran the wizard's test call against
QwenCloud and generated a lyric: **back in 1-2 seconds**, against the 33.12 s to first content
that the same endpoint gave with the field unsent (LLM-SURFACE 13.1). `config.json` carries
`"accepts_reasoning_effort": true`, so the verdict reached disk -- the half of this that the
T-212 class of bug lives in, checked rather than assumed.

**An unplanned confirmation of the design's central distinction.** By the time the check ran
the producer had switched models, from `qwen3.8-flash` to **`glm-5.2`** -- a different
vendor's model on the same gateway -- and the verdict was still there and still applied. That
is exactly what "acceptance is per endpoint, honouring is per model" (LLM-SURFACE 13.4) was
built for: the endpoint fact survives a model change, `choose` preserves it while
`saveEndpoint` clears it, and a third model on a second vendor honoured the field without
anything being re-probed.

**The arc, end to end.** T-301 removed the app's opinion about which model; T-301b removed the
assumption about where it runs; T-302 measured what that assumption had been costing; T-302b
made the app find out for itself instead of inferring. The through-line is the same one the
repo keeps rediscovering: **the app should verify a third-party fact rather than derive it
from something adjacent.** `thinks` was a good proxy for one provider and wrong as a rule.

Also this session: [docs/CSS-TODO.md](docs/CSS-TODO.md) opened, so styling gaps get written
down when they are noticed instead of being rediscovered at the Phase 5 polish pass. Two
entries, both from click-throughs -- the model list's unstyled scrollbar (the list itself
handles the producer's **163-model** endpoint with no lag) and the streamed-reasoning panel.

**Next: T-303**, `default_profile_id` persistence and the profile picker -- the last of the
Phase 2 carry-overs, and the same "never written to config" class as T-212.

### 2026-08-27 (later) -- T-303 verified live; T-304 briefed, and the type was already there

**T-303 passed every check**, including the one it was written around: with ComfyUI stopped
the profiles still list, tagged "cannot check", and the Audio page carries the not-running
notice -- readiness stayed information rather than becoming a gate. `default_profile_id` is
written on selection, changes as the model changes, survives a restart, and the Lyrics Studio
follows it. **Fourth reader-with-no-writer in this repo, and the first one checked against the
file rather than the screen.**

**T-304 turned out narrower than the phase file said.** `GenerationSpec`, `InputValue`,
`LoraRef`, `LyricRef` and the `ResolvedSlots` alias have existed since T-003; what was never
written is `resolve_slots`. The brief is therefore about the **fan-out** alone -- and the
fan-out is where the two traps the profile abstraction exists to hide actually live:
`duration_s` writes `94.duration` **and** `98.seconds`, one seed writes `94.seed` **and**
`3.seed`. Neither failure shows up until a track exists at the wrong length or refuses to
reproduce, so the acceptance criteria name the invariant (*every* declared address carries the
value) rather than the mechanics.

Three decisions the brief settles, each recorded because the opposite is defensible:

- **Only what the spec sets is written.** The template already carries defaults, so writing
  every declared slot would make a profile's `default` a second source of truth against the
  template's.
- **Types are matched exactly, never widened.** `set_workflow_slot`'s structured form
  preserves the type it is given (MCP-SURFACE 9.1), so an `Int` accepted for a `Float` control
  lands as an integer in a FLOAT slot -- and a `Seed` demoted to `Int` is the unreproducible
  track that `InputValue`'s adjacent tagging was introduced to prevent.
- **`create-core` gains `thiserror`, its second dependency ever.** The crate has been
  serde-only with no error type at all; `ResolveError` is its first. Noted in the brief rather
  than done quietly, because "create-core has one dependency" was a property worth noticing
  before it stopped being true.

### 2026-08-27 (later still) -- T-304 landed; mutation testing found the guard that mattered

`ModelProfile::resolve_slots` exists: the fan-out from semantic choices to the slot values
actually submitted. `create-core` 77 -> 94 tests, and it gained `thiserror` as its second
dependency ever (`Cargo.lock` grew exactly one line -- an edge to a package already in the
tree, as briefed).

The executor's run was faithful and its tests were **not** vacuous: the two fan-out tests
assert both addresses with non-default values, so mutating `resolve_slots` to write only the
first address failed both immediately.

**Then a second mutation found a real hole.** Loosening the seed arm so an
`InputValue::Int` is accepted where a `Seed` belongs -- the precise demotion that makes a
track unreproducible, and the reason `InputValue` is adjacently tagged at all (T-003) --
**passed all 22 tests**. `test_type_mismatch_errors` existed, but it exercised a *float*
control; nothing guarded the seed. Two tests added, both re-checked by re-running the
mutation.

Second time in this repo that mutation testing has found guards written and never armed
(T-110 was the first), and the shape is identical both times: a test named for a *mechanism*
covers one instance and reads as if it covers the class. **The habit to keep: after a task
whose whole point is a correctness rule, mutate the rule and watch the suite fail.** A green
suite is evidence only against the mutations someone tried.

Also this run: the seventh `cargo fmt` miss, and an unused `BTreeSet` import that was unused
in the lib and used in a test -- it belonged inside `mod tests`.

### 2026-08-27 (later still) -- T-305a landed; the mutations the brief named all died, and two more did not

`ensure_lossless_output` exists in the new `create-core::graph`: every audio save node in a
workflow is rewritten to `SaveAudioAdvanced` with `widgets_values` **rebuilt** to
`[filename_prefix, "flac"]`. `create-core` 94 -> 104 tests, and it gained `serde_json` as its
third dependency (promoted from dev-dependency; `Cargo.lock` unchanged, the package was
already in the tree).

The executor's run was faithful to the brief and the fixtures are the real templates. **All
three mutations the brief named turned the suite red**, and the middle one is the evidence
the whole task was shaped around: patching `widgets_values[1]` in place instead of rebuilding
the array **passed the ACE-Step test and failed only MiniMax**. Exactly the trap MCP-SURFACE
16.3 predicted -- a suite with one fixture would have shipped the stale `"V0"`.

**Two further mutations found holes the briefed three did not reach:**

1. **Deleting the entire `links` array passed every test.** The executor's
   `assert_other_nodes_unchanged` walked the nodes arrays pairwise, so nothing outside them
   was compared -- not `links`, not `extra`, not `last_node_id`. A workflow with no links is
   a disconnected graph, and T-306 would submit it. Replaced with `without_node` +
   whole-document equality, plus an `assert_ne!` guard so the comparison cannot go vacuous if
   the node id ever stops matching.
2. **Stopping after the first save node in each array passed every test.** Neither shipped
   template has two, so nothing enforced the "every" in the function's own contract. Added a
   synthetic three-save-node workflow (two top-level, one nested).

Both were re-checked by re-running the mutation against the new tests.

**The pattern worth carrying**: the briefed mutations test what the brief already understood.
The ones that find something are the mutations aimed at what the *tests* assume -- here, that
"unchanged" meant "the nodes are unchanged" and that "every" was covered by fixtures that
each have one. Third and fourth holes mutation testing has found (T-110, T-304 before them).

Also this run: the eighth `cargo fmt` miss and a `clippy::needless_lifetimes` failure, both in
the executor's own test helpers, neither in the briefed reference code.

### 2026-08-27 (later still) -- T-305b briefed; the splice was run live, and the near-miss was the finding

`tasks/t-305b-brief.md` is ready. The reference implementation was compiled, run against the
ACE-Step fixture, and its output **submitted to the live ComfyUI** -- `valid: true`,
`converted_node_count` 11 -> 13, completed in ~19 s, audio written.

**Then the near-miss, which is the real output of this session.** I built the same splice with
one plausible mistake -- loaders inserted, consumer link left sourced at the anchor -- expecting
validation to reject it. It did not. `valid: true`, zero errors, same converted node count, job
`completed`, `error: null`, audio written, **and not one byte of it touched by either LoRA**.
ComfyUI prunes unreachable nodes silently. Recorded as MCP-SURFACE 17.1 and as a standing
decision, because it generalises past LoRAs to every graph edit the pipeline will make.

Getting to that answer cost three dead ends, each worth not repeating:

1. **Compare the audio.** Two runs of the *unmodified* template, same seed, sampling forced
   greedy, differ in 98.1% of their bytes. ACE-Step is not reproducible run-to-run at all
   (17.3) -- which is now a constraint on what `provenance` may claim.
2. **Read the engine log.** `get_logs` returned a plausible run with `0 patches attached`. Its
   `mtime` predated every submission by nine hours: comfy-cli only captures a server it
   launched itself, and the owner launches his own (17.4). I nearly reported that stale line as
   evidence the LoRA had not applied.
3. **Poison the LoRA.** Point the splice at a `training_state.pt`, expecting a load failure to
   prove the node executes. It succeeded -- ComfyUI warns on unmatched keys and continues. That
   corrects MCP-SURFACE 4, which claimed those files cause failures (17.6). The design
   consequence is stronger, not weaker: picking one now yields a silent no-op.

What settled it was `GET /history/<prompt_id>`, the executed API prompt. Correct:
`78.model=["112",0]`. Dangling: `78.model=["104",0]`, with 111 and 112 present, correctly
configured, and feeding nothing.

**Why this took a long session and was worth it:** three of the four things I tried to verify
with returned a confident, wrong-shaped answer -- a clean validation, a stale log, a successful
poison run. Any one of them, taken at face value, produces a brief that ships the bug. The
brief now leads with the comparison table and requires the chain test as a traversal, because
'the loaders are present with the right names' is true of the broken graph.

Also settled while in there: `widgets_values` is `[lora_name, strength_model]` (live schema),
spliced loaders become ordinary addressable slots so T-308 can change a strength without
re-splicing (17.5), and `last_node_id` can exceed the top-level maximum -- MiniMax declares 43
against a top-level max of 40 -- so id allocation takes the max of declared and present.

### 2026-08-27 (later still) -- T-305b landed; the chain test was checking the field the engine ignores

`splice_loras` exists. `create-core` 104 -> 118 tests. The executor reproduced the reference
implementation faithfully and its own test coverage was good -- fourteen tests, every acceptance
criterion, including the fan-out case and a synthetic stale-high-water-mark graph I had only
described in prose.

**All four briefed mutations died**, and the first one died hard: the dangling splice took out
five tests including both chain tests. That is the failure the whole brief was built around, so
it mattering was the point.

**Then a fifth mutation found the hole, and it was in the chain test itself.** Setting every
loader's `inputs[0].link` to null -- leaving the `links` array perfectly correct -- **passed all
118 tests.** `assert_model_chain` read only the `links` array.

Two runs on the live install established which representation actually matters (17.8):

- Anchor's `outputs[].links` left stale -> `valid: true`, and the **executed prompt is identical
  to the correct splice**. Cosmetic; it only renders wrong if the graph is opened in ComfyUI,
  which the owner does routinely.
- Loaders' `inputs[0].link` null -> **`valid: false`**, `required_input_missing` on node 112.

So the UI-to-API converter builds from `inputs[].link`, and the chain test was asserting the one
of the three edge records the engine ignores. It now checks all three, with the two live results
written into the doc comment so the next person does not re-derive them. Both mutations now fail.

**Worth naming, because it is a pattern now.** The brief demanded a chain test *because* a
field-by-field check would pass the dangling graph -- and the chain test that resulted had the
same shape of flaw one level down: it verified the representation that was convenient to read
rather than the one that is load-bearing. Making a test 'about the real invariant' does not make
it about the real invariant; only checking what the consumer consumes does. Fifth and sixth holes
mutation testing has found (T-110, T-304, twice in T-305a, now here).

Also this run: the ninth `cargo fmt` miss, the same `clippy::needless_lifetimes` on a test helper
as last time, and two drive-bys reverted -- a `section`-wording sweep across T-305a's landed doc
comments (against the crate's own `(MCP-SURFACE 9.1)` convention) and `SaveNodeChange` shuffled
above the error enum for no reason.

### 2026-08-27 (later still) -- T-306 briefed and split; the seed the app would have lied about

Briefing the pipeline meant resolving the shipped ACE-Step profile against the real template for
the first time. Two of the seven addresses it writes **do nothing**: `3.seed` and `94.seed` are
link-fed from `PrimitiveInt` 109, `set_workflow_slot` reports both `applied`, and the engine's
executed prompt reads `seed: ["109", 0]`. Every generation would have used the template's seed
while the sidecar recorded the user's -- and batches, which are N seeds of one spec, would have
been N identical jobs.

**The near-miss inside the near-miss:** the obvious guard, "flag any link-fed slot", is wrong.
The same template feeds `94.duration` and `98.seconds` by link too, and those writes land,
because node 99 is a frontend-only `PrimitiveNode` whose links are dropped at conversion. Proof:
99 holds 120, the consumers were written 10, the engine ran 10.0. A guard that condemned all
four would have been a false alarm on half of them, and false alarms are how guards get deleted.
So `audit_slots` keys on the source node's class, and reports what it could not check rather
than skipping it.

Also verified for the brief: `serde_json::to_value(InputValue)` sends the adjacent tag over the
wire and is rejected for INT and STRING alike -- a wrong conversion that fails closed (18.2);
an over-range value warns on write but fails validation (18.3); and `KSampler.seed` really does
declare the full `u64` T-003 claimed, while `PrimitiveInt.value` caps at `i64::MAX`, so this
template's usable seed range is narrower than the node's (18.4).

Split into **T-306a** (the pure seam plus the profile fix, briefed) and **T-306b** (the Tauri
command). T-306a's regression test is the one that matters: resolve the shipped profile against
the real fixture and assert no address is inert. It fails on the profile as it stands.

**The pattern, now three tasks running:** T-305a asked whether the format value was right rather
than the node class; T-305b asked whether the chain reached the consumer rather than whether the
nodes existed; this asks whether a write is read rather than whether it was accepted. Every one
of them is the same question -- *does the thing downstream actually see this?* -- and every time
the convenient signal said yes.

### 2026-08-27 (later still) -- T-306a stalled twice on my brief, not on the code

Two runs, no edits. Both stops were mine.

**Run one** asked for `profile.rs`: my file list named three files and the seed change reaches
five sites. Corrected, and the correction found a site the executor had not: adding
`109.value` to `VERIFIED_ACE_STEP_SLOTS`, without which the typo guard fails after the gate.

**Run two** asked whether line 30 was the only doc comment to change (it is), and then said it
would output the full updated files -- and stopped. That question was answerable; the real
problem was underneath it. **Aider runs in `whole` edit format**, re-emitting every `--file` in
full, and I had scoped a task across `generation.rs` (27 KB), `profile.rs` (27 KB) and
`graph.rs` (49 KB): ~102 KB to emit before writing a line of new code.

`graph.rs` went 18 KB -> 49 KB in a single task (T-305b) and I did not look at what that meant
for the next brief. **The executor lane has a working-set budget and briefs have to be written
against it**, the same way they are written against the ~400-line diff limit.

The fix was also the better design: `audit_slots` moves to a new `audit.rs`. `graph.rs`'s own
module doc says *"pure workflow graph **edits**"*, and an audit edits nothing -- so the module
boundary was wrong on merit before it was wrong on size. `graph.rs` now drops out of the task
entirely, and the run's working set falls to ~60 KB. Reference code compiled and linted
standalone in the new module before the brief went back.

Also switching this run to `--edit-format diff`, with a note to fall back to `whole` if the
model handles it badly. `whole` will not survive the crate getting bigger.

**Worth keeping:** two stalls cost nothing but time, because the executor asked instead of
guessing -- the "If unclear, do not guess" clause earning its place twice in one task. The
failure mode to watch is the opposite one, where a brief is *just* answerable enough that the
executor proceeds on a wrong assumption.

### 2026-08-27 (later still) -- T-306a landed on the third attempt; the seed bug is closed

`--edit-format diff` worked: small targeted hunks, `graph.rs` untouched, and the run completed
instead of stalling. `create-core` 118 -> 126 tests, and the shipped ACE-Step profile now writes
its seed to `109.value`, the one address that reaches the sampler.

**One miss, and it was in the site list I wrote.** §4 item 5 said the expected-address set swaps
`3.seed` and `94.seed` for `109.value`; the executor swapped `3.seed` and left `94.seed`. The
gate caught it -- one failing test, one line to fix. Worth noting the shape: the instruction
named two addresses in one sentence and only one got acted on. Naming each edit on its own line
would have been harder to half-apply.

**All six briefed mutations killed.** Two more from review:

- Reading the link's **target** node type instead of its source -- caught.
- **`audit_slots` returning an empty audit** -- caught by four tests, but *not* by
  `test_no_inert_slots_in_shipped_ace_step_profile`, the test this whole task exists for. An
  empty audit has no inert slots, so the headline assertion passed on a function that did
  nothing. Added vacuity guards -- addresses non-empty, `unchecked` empty, `link_fed` non-empty
  -- and re-ran the mutation to confirm it now bites. Same fix as T-305a's `assert_ne!`, and the
  same lesson: an assertion of the form "nothing bad was found" is only as good as the proof
  that something was looked at.

Also moved the two `to_slot_value` tests from `audit.rs` to `generation.rs`, beside the method
they cover. Every module in this crate keeps its own tests; a test that lives one module away
from its subject is one the next editor will not see.

### 2026-08-27 (session close) -- handoff: what the next session should carry forward

Phase 3's **pure half is done**. `create-core` can resolve a spec to slot values, make the save
node write FLAC, splice a LoRA stack into the MODEL chain, and tell you whether a slot write
will actually reach the engine. **Nothing generates audio through the app yet** -- T-306b is the
seam that wires it, and it is unbriefed.

**Read before writing any pipeline code.** Three things were established by running them, and
each contradicts a reasonable assumption:

1. **A clean `validate_workflow` does not mean an edit took effect.** A LoRA chain spliced in but
   feeding nothing validates clean, runs, and writes audio with no LoRA applied. It *is* good for
   enum, range and missing-input errors. MCP-SURFACE 17.1.
2. **`applied` from `set_workflow_slot` does not mean the value is read.** ACE-Step's seed wrote
   two addresses that the engine ignored. Fixed, and `audit_slots` is the standing guard --
   but it reports subgraph interiors as `unchecked`, so **MiniMax's seed is unverified, not
   working** (18.5, backlogged).
3. **ACE-Step is not reproducible run-to-run.** Two identical runs, fixed seed, greedy sampling:
   98.1% of bytes differ. No check may rest on two runs matching, and whatever the UI says about
   seeds must not promise the same waveform back. 17.3.

`GET /history/<prompt_id>` is the only surface that shows what the engine actually ran (17.2).
It settled all three. Reach for it whenever the question is "did that edit land".

**Operational notes for the executor lane:**

- **Aider now runs with `--edit-format diff`.** The default `whole` format re-emits every
  `--file` in full and stalled T-306a twice; `graph.rs` alone is 49 KB. Keep `diff`, and keep
  briefs' file sets small. **The executor lane has a working-set budget** and briefs have to be
  written against it, the same way they are written against the ~400-line diff limit.
- **Name each edit on its own line.** T-306a's site list put two address swaps in one sentence
  and only one was applied. The gate caught it, but the shape is worth avoiding.
- **Mutation testing after every correctness task is now standing practice**, and it has found a
  real hole six times (T-110, T-304, twice in T-305a, T-305b, T-306a). The mutations a brief
  names test what the brief already understood; the ones that find something are aimed at **what
  the tests assume**. Three tasks running, the same flaw appeared: an assertion of the form
  "nothing bad was found" passing because nothing was looked at. Check for vacuity explicitly.

**Two `.md` files are the authority over anything remembered or inferred:**
[docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) for anything comfy-mcp or ComfyUI (18 sections now),
and [docs/LLM-SURFACE.md](docs/LLM-SURFACE.md) for endpoints and streaming. ARCHITECTURE 7 has
been corrected twice this phase by findings in those files; if it disagrees with them, they win.

**Left deliberately undone:** MiniMax seed verification and a declared range for
`InputSpec::Seed` (both in the backlog); the two styling items in
[docs/CSS-TODO.md](docs/CSS-TODO.md). Test tracks named `T305B_*`, `det_*`, `poison_*`,
`stale_anchor` and `null_input` are sitting in the producer's ComfyUI `output/audio` and can be
deleted.

### 2026-08-27 (later still) -- T-306b briefed, and the phase file was wrong about `local_check`

Session ritual first: PROJECT.md and ARCHITECTURE.md against `git log` -- no drift, tree clean,
`1482590` is the session-close commit the snapshot describes.

**The brief's headline is a correction.** The T-306 stub said the pipeline should gate on
`local_check` before running. Following that would have made MiniMax Music 3 ungenerable: the
check runs at fetch time, before the profile's `slot_overrides` are applied, and MiniMax is
`runnable: false` over precisely the filename its override fixes. `models.rs` already opens with
a paragraph on why readiness never reads that field; the pipeline inherits the trap and adds one
of its own, because it fetches before it fixes anything. Fourth time this phase that an
available signal answered a different question than the one being asked.

**The reference code was written, compiled, run and mutation-checked before the brief went out**
-- 6 new tests green, `fmt`/`clippy`/`cargo test --workspace` clean at 51 tests for `app`, then
the tree reverted. Five mutations were run and all five bite. Two are worth naming:

- **Dropping the write-back** of the edited graph is caught *only* by re-reading the submitted
  file. The returned `output_format` says `flac` either way. That is the fifth consecutive task
  where "nothing bad was found" would have passed on a function that did nothing -- the check
  is now written into the brief as a criterion, not left to review.
- **Moving the audit below `set_slots`** is caught only by asserting the *call count* on the
  refusal path. It is the difference between refusing a bad run and performing it.

**The one structural change:** `mcp-bridge`'s mock transport is exposed behind a `test-support`
feature and taken as a dev-dependency of `src-tauri`. Four `cfg` lines. Every command so far
made one MCP call; this one makes four in order against one file, and ordering is the only thing
that can go wrong in it. `cargo build -p app --lib` was watched recompiling `mcp-bridge` without
the feature, so nothing reaches a shipped binary.

**Working set for the run: ~36 KB across seven files**, four of them a few lines each -- under
the ~60 KB the successful T-306a run carried. `create-core` is not passed at all; every
signature the task needs is in the brief.

**Not verified live, and deliberately:** no generation was run. The four-call sequence is proven
against the mock; that it produces audio is T-314's job, and it is also the first chance to
settle `vram_gb_min: 8`.

### 2026-08-27 (evening) -- T-306b landed; the app can queue a job, and has never run one

The executor reproduced the brief's reference implementation exactly -- byte-identical modulo
line endings -- so the review was not about whether the code matched the brief. It was about
what the brief's own tests could not see.

**The finding: the LoRA splice had no positive test.** The suite proved a *bypassed* LoRA is
not spliced. Two mutations pass it:

1. Remove the splice branch. (Caught only by an unused-import warning -- an accident, not a
   test.)
2. Splice into `graph.clone()`. **No warning, gate green, `lora_nodes` correctly populated in
   the `Submission`, and the submitted file carries no LoRA at all.**

The second is the exact failure MCP-SURFACE 17.1 documents -- a track with none of the user's
LoRAs and nothing anywhere saying so -- occurring inside the module written to respect it. Added
`test_an_enabled_lora_reaches_the_submitted_file`, which finds the reported loader id **in the
file that was submitted** and checks its class, name and strength. Both mutations now fail.

`app` 45 -> 52 tests. Gate green.

**What is true now:** `generate_audio` fetches a template to a per-job copy, refuses a spec whose
writes the engine would ignore before writing anything, sets every addressable slot, splices the
LoRA stack, makes the save node write FLAC, validates, submits, and hands the prompt id to the
existing pump. **What is not true yet:** any of it having produced a sound. There is no UI
calling it (T-308), nothing ingests its output (T-311), and no GPU has seen one of these graphs.

**Push check, asked and answered.** 23 commits were sitting unpushed -- all of Phase 3, not just
this session. Scanned the range and the whole history: no key literals, no `sk-` strings, no
bearer tokens, no home paths or usernames, and the captured workflow fixtures carry no absolute
paths. The QwenCloud key is out of reach by construction -- the two credit-spending live tests
read it from the OS keychain via `library::secrets::get_secret`, `config.json` holds endpoint and
model only and lives in the app data dir, and the only `sk-` in the repo is the `"sk-secret"`
fixture asserting the session log redacts it. Safe to push.

### 2026-08-28 -- T-307 briefed, and the fixture it needed

Wrote [tasks/t-307-brief.md](tasks/t-307-brief.md) and, first, captured the list it is about.

The phase file had been saying since T-301 that the 53-entry LoRA list was captured into
`testdata/mcp/`. It was not. Read it live -- ComfyUI was down, so comfy-cli answered from its
`object_info` cache; the 53 choices and the `strength_model` range match 16.5 and 17.7 exactly,
so it is a faithful copy of the 2026-08-27 read and the fixture keeps the `stale: true` flag
that says so. The case-variant pair behind the dedupe rule still cannot be captured (seen once
on 2026-08-23, described, gone since before the 24th), so it is a second file carrying
`"_synthetic": true` and a `_why` field, with a test asserting that flag survives.

**The catalog:** 53 raw choices -> 12 pickable entries in 6 groups, 21 exclusions each with a
reason. 21 `training_state.pt` files excluded (17.6: picking one does not fail, it silently
applies no LoRA), 20 epoch checkpoints folded behind an expander, one `final/` promoted, paths
carried verbatim because the path is what `splice_loras` writes into the loader.

**The thing worth remembering:** the real fixture cannot catch the likeliest bug in the module.
Because `loragoth` has a `final/`, every epoch is superseded and the epoch number is never
compared -- so a lexicographic comparison, where `epoch-90` beats `epoch-300`, passes the whole
suite. The test that catches it filters `final/` out of the real list: the same directory, mid
training run. Reference implementation written, formatted, clippy-clean at 136 create-core tests,
and all seven mutations killed before the brief was written.

Ready for the Aider run.

### 2026-08-28 (later) -- T-307 landed, and two rules the owner asked to be written down

`create-core` 126 -> 137 tests, gate green. The catalog turns 53 raw `lora_name` choices into 12
pickable entries in 6 groups with 21 reasoned exclusions.

**Review found two more sorts the captured list cannot test.** ComfyUI hands back `choices`
already sorted, so the group ordering and the within-group ordering are both satisfied by
accident on the fixture -- delete either sort outright and all fourteen other tests still pass.
Both mutations survived the briefed suite. Closed with one test that runs the same real list
**reversed** and asserts the groups come out identical, which states the real invariant: the
catalog is a function of the set of choices, not the order they arrive in. Ten of ten mutations
killed.

That is the third task running where a suite proved an absence and never the presence, and the
second where the input's own tidiness hid the rule. The generalisation, now in phase-3.md for
T-308 onward: **when the data a fixture captures already satisfies a rule, the rule is
untested** -- feed it the same real data with that property removed.

**Two owner rules written into the docs.** Aider is a token-saving device only, and the lane is
chosen before the brief is written (WORKFLOW 1, AGENTS hard rules). And doc freshness got the
rule it was missing: verify a doc's *claim* against the repo, not against another doc (WORKFLOW
6). The existing rule only ever checked docs against `git log`, which by construction cannot see
a doc that was wrong the day it was written -- which is what both T-306b and T-307 hit.

**Cleared the stale claims that prompted it**, rather than only writing the rule: AGENTS.md item
5 still described Phase 3 as not started and carried a "trust PROJECT.md if we disagree" escape
hatch; ROADMAP said Phase 1 delivered a `ComfyBackend` trait that was deferred twice and does not
exist in the code, still quoted the stale 95-entry LoRA figure, and said "Phase 0 briefs exist
now".

### 2026-08-28 (later still) -- T-308 split, T-308a landed

Whole-of-T-308 costs about 1100 lines across two Rust commands, the bridge, the panel model,
the component and its CSS -- nearly three times the 400-line rule, and T-306a stalled on brief
size once already. Split into **T-308a** (the pure model, no ComfyUI needed) and **T-308b**
(the commands and the panel). Same ordering that made T-304, T-305 and T-307 possible.

T-308a is the **first task in the architect-direct lane** the owner defined this morning. Its
brief therefore does not restate the reference implementation -- the code is the code, and
duplicating it into a brief is exactly the cost the lane rule exists to avoid. What the brief
carries is what does not survive in source: the findings, the rejected alternatives, and the
invariant behind each test. Frontend 128 -> 141 tests, gate green, seven mutations run and
seven killed.

**The finding: a `u64` seed cannot survive JavaScript.** `InputValue::Seed` exists in
`create-core` so a seed cannot be demoted to another number type, and its own tests pin
`Seed(u64::MAX)`. None of that binds the webview -- above 2^53-1 a JS number changes on the
way through, JSON cannot carry a BigInt, and a seed typed as 18446744073709551615 would arrive
in Rust as ...616, generate, and be written into the sidecar as fact. The panel refuses those
seeds instead of clamping them. Third time this phase that a guard held in the layer that owns
it and not in the layer above.

Also caught in passing: the first draft added eight `oxlint` warnings (`no-shadow`,
`unicorn/no-array-sort`). Non-fatal, and the gate would have passed with them. Fixed anyway --
a repo at zero warnings stays there or it is not a signal.

### 2026-08-28 (evening) -- T-308b's data path landed, the panel briefed

Split T-308b by lane rather than by layer, which is the lane rule doing its job. The
`profile_inputs` command, the bridge call and the panel store are small and test-bearing, so
they were written and verified here and are landed: 141 -> 151 frontend tests, 52 -> 54 app
tests, six mutations run and six killed. `<ParamPanel>` is ~300 lines of JSX and CSS with no
logic in it -- because T-308a put every derivation in `params.ts` and this put every piece of
state in `paramPanel.ts` -- so it is briefed for Aider with an exact class list, the copy
strings written out, and a producer click-through.

**Found while writing it:** the profile knows the node *instance*, not the *class*, so the
three `from_node_choices` enums cannot be filled without resolving `94` against the template.
That is T-308c, with the fix named. Verified the live schema while checking:
`TextEncodeAceStepAudio1.5` has keyscale (34), language (51, default `en`) and timesignature
(**4 values, `2 3 4 6` -- numerators, not `4/4`**). Two things noted in passing: that node's
`seed` max is `18446744073709551615`, so ComfyUI itself corroborates T-308a's ceiling, and its
`duration` runs to 2000 s while the profile caps at 300 -- the profile is deliberately
narrower and should not be "fixed" to match.

The reference Rust needed `cargo fmt` before the gate went green -- the exact failure
WORKFLOW 1 warns about, on the first Rust I had written in this lane.

### 2026-08-28 (night) -- T-308b landed; the panel exists

The Audio view now shows a real settings panel built from the profile's own declarations.
Frontend 151 -> 154 tests, gate green.

**The gate came back red, and it was the brief's fault.** `Control` carried
`range: Range | null`, so nothing narrowed it on `kind`, and the JSX this brief specified
verbatim -- `min={control.range.min}` -- did not compile. Fixed by making `Control` a
discriminated union rather than by adding a null check for an impossible state; a guard like
that reads as if the bounds were optional and leaves a dead branch behind forever. Second time
in two tasks that the executor reproduced the brief exactly and the brief was the thing that
was wrong.

**Review moved one derivation out of the view.** `distinctGroups` was in the component,
deciding which fieldsets render and in what order -- and vitest runs in `node` with no DOM, so
nothing in the gate could reach it. Now `groupsOf` in `params.ts`, with tests. Its ordering
rule was untestable against the shipped profile, which has exactly one group: sort them
alphabetically and nothing notices. Same trap as the LoRA catalog in T-307, same answer -- the
real declarations plus one more group, labelled so the two orders disagree. Three mutations,
three killed.

**And the run deleted a comment it had no business touching**, the note in `AudioStudio.tsx`
explaining why the unknown-profile wording must not promise a fallback the app does not
perform. Restored. Worth watching: an executor given a file to modify will tidy things it was
not asked about.

**Nothing here has been looked at by a person yet.** The gate cannot see any of it and neither
can vitest without a DOM -- the click-through list is in the brief, and the seed-refusal row is
the one that matters most.

### 2026-08-28 -- T-308b click-through passed (producer, live app)

All seven rows of the brief's list, run by the producer against the running app. First Phase 3
work a person has actually looked at.

- Controls render in musician order: tags, lyrics, duration, bpm, key, time signature,
  language, seed.
- **No negative-prompt box**, and "Not offered by this model" carries the profile's recorded
  reason.
- Key, time signature and language are disabled and say to start ComfyUI -- and read as a
  different situation from the negative-prompt line, which was the point of keeping
  `fromNode` and `omitted` apart.
- Advanced collapsed by default; opening it shows steps, shift and the five-control
  **Planner sampling** fieldset.
- The seed is filled on open, differs between sessions, and **Reroll** changes only it.
- **A seed of `18446744073709551615` is refused with a message.** The T-308a finding is now
  confirmed behaviour rather than a code-reading argument: nothing is rounded, and no sidecar
  will record a seed the user did not choose.
- Switching to Lyrics and back keeps typed tags and the seed -- the store's reload guard
  holding in the real remount path, not just in a test.

**What this does not mean.** No audio has been generated. The panel builds a `GenerationSpec`
nothing submits yet (T-310), three of its controls have no options until T-308c, and no GPU
has seen one of these graphs. T-314 is still the first live run.

### 2026-08-28 (late) -- T-308c: the panel is complete, and a cache stopped lying

Key, time signature and language now fill from the node registry. `InputSpec::Enum` gained the
`node` field the last session identified as missing, the ACE-Step profile names
`TextEncodeAceStepAudio1.5` on all three, and one schema read serves all of them. Entirely
architect-direct: every piece was small and test-bearing, so there was nothing for an executor
to save.

**The finding is bigger than the task.** `nodes(action="get")` succeeds while ComfyUI is down,
answering in full from comfy-cli's cache with `stale: true` and a warning naming the
connection that failed -- and `mcp-bridge` was throwing both away. Every caller was treating a
cache as the installed truth with no way to tell. The fixture captured for T-307 turns out to
be exactly such a response, so it is also the test for this.

For key signatures a stale list is nearly harmless. The same call enumerates LoRAs, and there a
cached list hides the one the user trained an hour ago -- so the warning had to exist **before**
T-309 builds the picker on the same path. *(This paragraph also claimed a cached list offers
deleted files and that picking one completes silently. Measured 2026-08-28 and wrong: the
pipeline's `validate_workflow` step rejects an unknown `lora_name` outright. 19.3.)*

Every wording decision sits in `withChoices` rather than JSX, so all four states are testable
in a repo whose vitest has no DOM. Retry is wired because ComfyUI is usually started after the
app, and options that can only be fetched once leave three dead dropdowns with no way back.
Seven mutations, seven killed.

**Owing a click-through** -- specifically that the three dropdowns fill once ComfyUI is
running, and that with it down they say the list is cached rather than presenting it as fact.

### 2026-08-28 -- T-308c click-through: the warning was firing on every healthy install

The producer ran the panel with ComfyUI up and down, which settled the question T-308c left
open and exposed two defects at once.

**A live read carries neither signal.** No `stale` key, no warning -- there is no
`stale: false`. The tri-state reading ("did not say" is not fresh) therefore warned every
single time ComfyUI was running perfectly. Worse than silence: a caution that is always on is
one nobody reads by the time the LoRA picker needs it. Absence is now evidence, because the
live shape has been observed rather than guessed at, and MCP-SURFACE 19.1 carries the two
shapes as a table instead of a hedge.

A mutation then caught that consulting **either** signal alone passed the whole suite -- they
have arrived together on every response so far -- so each now has a test for the case where
the other is missing.

**And the note was unreadable.** On screen it ran: "...may be out of date. served from cache
(http://127.0.0.1:8188): cannot reach http://127.0.0.1:8188/object_info: [WinError 10061] No
connection could be made because the target machine actively refused it Start ComfyUI and
retry to refresh them." Lowercase, unterminated, the URL twice, a Windows error number, and
the one instruction that mattered stranded past all of it. The transport detail is out of the
sentence -- which endpoint failed is the status pill's job -- and a test now asserts the note
stays one sentence with no URL in it.

**Worth keeping:** both defects were invisible to the gate and to seven passing mutations, and
both took one person looking at the screen twice. The click-through is not a formality at the
end of a UI task; for anything whose correctness is a *sentence*, it is the only test there
is.
