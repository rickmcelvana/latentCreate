# PROJECT.md — latentCreate (living document)

> Load this file at the start of every session (Claude Code, Opencode, any agent). Update it at the end of every session. **Session-start rule: verify this file and ARCHITECTURE.md agree with `git log` since the last session entry; fix drift before new work.**

## Snapshot
- **Project:** latentCreate — open-source, desktop-only (Tauri 2) AI music creation front-end. Orchestrates user-provided ComfyUI (via Comfy MCP) for audio/image generation and a user-provided LLM for lyrics. **Ships no models.** Complements the closed-source siblings `../latent-mixing` and `../latent-mastering` (send-to targets) and the in-development latentPlayer.
- **Repo:** public, Apache-2.0, `github.com/rickmcelvana/latentCreate`. CI green on ubuntu/windows/macos.
- **Phase:** **0 and 1 complete**, tagged `phase0-done` (2026-08-23) and **`phase1-done` (2026-08-25)**. **Phase 2 (Lyrics Studio) is open** -- T-201 (project and lyric store), T-202 (brief type and prompt assembly), T-203a/b (the lyric lint: structure-tag scanner and section rules), T-204 (`reasoning_effort` on `ChatRequest`), T-205 (Tauri lyric streaming command and event pump) and T-206 (frontend lyric bridge + streaming store) have landed — the lyric surface was verified live on 2026-08-25 and the phase is planned as T-201 … T-211 in [tasks/phase-2.md](tasks/phase-2.md). The app builds, runs, and has a **working three-step setup wizard**: it detects and can **start** the user's ComfyUI, checks their installed models against shipped profiles and installs what is missing, and configures a lyric LLM with a live test call. `mcp-bridge` (88 offline tests) covers the whole verified comfy-mcp tool surface; `llm-bridge` (35 + 4 live) covers OpenAI-compatible streaming plus Ollama's native API. **Nothing is wired to a generation pipeline yet** — the app proves it *could* make music, which is exactly what Phase 1 set out to do.
- **Landed in Phase 1:** T-101 (stdio transport, `ComfyError`, health), T-102 (mock transport rig), T-102b (session log + redaction), T-102c (stderr capture + free-text redaction), T-103a (templates + `local_check` tri-state), T-103b (slots + self-verifying writes), T-103c (validation verdicts + untrusted notes), T-104a (job lifecycle wrappers), T-104b (Tauri managed state + job event pump), T-104c (frontend jobs bridge + store + queue panel), T-105a (model discovery), T-105b (model download), T-106 (node registry), T-106b (`minimax-music-3` profile + `slot_overrides`), T-107a (profile loader), T-107b (profile slot addresses), T-108a/b/c (`llm-bridge` `openai_compat`: SSE framing, wire types, streaming client), T-109a/b (`ollama_native`: model listing + pull with progress), T-110a/b/c (Setup wizard ComfyUI step: typed `server_info`, `ComfyStatus` tagged union, health pill with a next step per state), T-111a-e (models step: profiles declare their model files, readiness by exact match against `search_models`, per-file install with byte-weighted progress, licence on every row), **T-112a-d (LLM step: capability-filtered picker, remote-model privacy disclosure, suggestions as data, test call)**. The comfy-mcp surface these were built against is **verified live** and recorded in [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) — that file is the authority, not the tool docs.
- **Next up:** **T-207** (LyricsStudio: the brief form -- prefilled from the selected profile's `prompt_guide.examples`, structure picker, plain-text language; one primary action). [tasks/phase-2.md](tasks/phase-2.md) carries the rest of the phase.
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
## Open questions (owner to decide)
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
