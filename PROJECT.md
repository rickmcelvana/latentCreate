# PROJECT.md — latentCreate (living document)

> Load this file at the start of every session (Claude Code, Opencode, any agent). Update it at the end of every session. **Session-start rule: verify this file and ARCHITECTURE.md agree with `git log` since the last session entry; fix drift before new work.**

## Snapshot
- **Project:** latentCreate — open-source, desktop-only (Tauri 2) AI music creation front-end. Orchestrates user-provided ComfyUI (via Comfy MCP) for audio/image generation and a user-provided LLM for lyrics. **Ships no models.** Complements the closed-source siblings `../latent-mixing` and `../latent-mastering` (send-to targets) and the in-development latentPlayer.
- **Repo:** public, Apache-2.0, `github.com/rickmcelvana/latentCreate`. CI green on ubuntu/windows/macos.
- **Phase:** **0 complete, tagged `phase0-done`** (2026-08-23). **Phase 1 in progress** — [tasks/phase-1.md](tasks/phase-1.md). The app builds, runs, has a nav shell over five placeholder views, a complete domain model, a config store with OS-keychain secrets, and CI. **It can now talk to `comfy-mcp`** (`mcp-bridge`, 55 offline tests) but nothing is wired to the UI yet.
- **Landed in Phase 1:** T-101 (stdio transport, `ComfyError`, health), T-102 (mock transport rig), T-102b (session log + redaction), T-102c (stderr capture + free-text redaction), T-103a (templates + `local_check` tri-state), T-103b (slots + self-verifying writes), T-103c (validation verdicts + untrusted notes). The comfy-mcp surface these were built against is **verified live** and recorded in [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) — that file is the authority, not the tool docs.
- **Next up:** **T-104a** (job lifecycle wrappers — briefed, ready to run) then **T-104b** (Tauri managed state + event pump). The `ComfyBackend` trait is deferred (decisions log 2026-08-24); the run/job/fetch success shapes are captured (MCP-SURFACE §10).
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
