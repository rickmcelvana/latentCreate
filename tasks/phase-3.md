# Phase 3 — Audio Studio & the generation pipeline

Goal: the approved lyric and a set of profile-driven parameters become a queued ComfyUI job,
and the finished track lands in the library with a sidecar complete enough to reproduce it.
This is the phase where the app stops describing music and makes it.

**Read first:** [docs/MCP-SURFACE.md section 16](../docs/MCP-SURFACE.md) — the phase-start
re-verification, run 2026-08-27 against ComfyUI v0.34.1. Then ARCHITECTURE
[5a](../ARCHITECTURE.md), [5b](../ARCHITECTURE.md) and [7](../ARCHITECTURE.md), **both of
which section 16 has just corrected**. Reading the pre-2026-08-27 text of ARCHITECTURE 7
step 3 and implementing it would ship the bug described in 16.3.

**The habit Phase 2 ended on, restated because this phase is where it gets expensive.** Every
defect the Phase 2 milestone found came from a person clicking, and three of four were
invisible to `tsc`, `oxlint` and 109 tests because they were **correct logic derived inline in
a view**. `approvedLabel`, `generationPhase` and `approvedText` were each the fix: pull the
decision into the store where a test can reach it. The param panel, the LoRA stack and the
queue are far more stateful than anything in Phase 2 — a value derived in JSX here is a defect
nothing in the gate can see.

---

## Before T-301: verify the comfy-mcp surface  DONE 2026-08-27

Full evidence in [MCP-SURFACE 16](../docs/MCP-SURFACE.md). Five findings, each of which
changes a task below rather than decorating it.

1. **`SaveAudioAdvanced.format` is not a slot, and never will be.** `list_workflow_slots` does
   not surface the `COMFY_DYNAMICCOMBO_V3`, and `set_workflow_slot` rejects it outright with
   `[workflow_slot_invalid]` — loudly, unlike 9.1's three silent traps. The format is a
   **positional `widgets_values` entry set by graph edit**, and **the array length varies by
   format**: `flac` has no sub-widget, `mp3`/`opus` carry a `quality` sub-combo. Truncate; do
   not overwrite in place (16.1).
2. **`flac` is the only lossless option** — there is no WAV — and it writes **16-bit/48 kHz
   with no bit-depth control**. The UI must not offer 24-bit, and nothing should imply the app
   chose 16-bit (16.1, 16.2).
3. **The MiniMax template ships `SaveAudioAdvanced` already set to `mp3`/`V0`.** The old
   node-class test passes it and hands MP3 to the mastering chain. **The condition is the
   format value** (16.3).
4. **A clean `validate_workflow` cannot prove the format.** Dynamic-combo sub-inputs are one
   of comfy-cli's four documented blind spots. Step 4 of the pipeline still earns its place —
   it catches a bad splice before GPU-minutes — but it is not evidence about the save node
   (16.1).
5. **The LoRA list is 53 entries, confirming 12.2 rather than changing it** — 4's 95 is the
   stale figure, and the drop predates 2026-08-24. The picker's requirements are unchanged,
   but **the 95-entry list with its case-variant directory was never captured as data**, which
   costs T-307 one fixture (16.5). Also: `job` has gained **`action="error"`**, a normalized
   failure view, which is what the queue panel should read rather than parsing status (16.6).

**Unchanged, and therefore safe to build on:** the ACE-Step turbo template is still
`runnable: true` with **33 slots at identical addresses** — both duration slots, both seeds,
all 17 addresses the profile declares. MiniMax still needs exactly its one `slot_overrides`
fix. Both shipped profiles are correct as written.

**Two things this phase is expected to settle, on evidence rather than argument:**

- **OQ-3 (raw ComfyUI API fallback).** First evidence is already in: `nodes(action="get")`
  returns `choices: []` for a dynamic combo while a plain GET to `/object_info` returns the
  whole option tree. One read-only lookup during verification is not a runtime dependency, so
  this is a point in the column, not a decision. T-305 and T-306 are where it becomes one.
- **`ace-step-1.5-turbo`'s `vram_gb_min: 8`**, the oldest open question in the repo. The
  profile says 8 GiB; the XL turbo DiT alone is 9.3 GiB. It cannot be settled by argument and
  T-113 never required a generation. **T-314's full-length run is the first thing that can
  answer it** — record what the card actually does, then fix the number or leave it with a
  reason.

---

## Tasks

Briefs are written one at a time, each after the previous lands. Each gets its own
`tasks/t-3NN-brief.md` when written.

Ordering principle: **everything that can be tested without a running ComfyUI comes first**
(T-304, T-305, T-307 are pure functions over captured JSON), then the wiring, then the UI,
then the live milestone. This is the same shape that made Phase 1's `mcp-bridge` 88 offline
tests possible, and it is what keeps the pipeline's hard part — graph surgery — out of the
"you had to be there" category.

### T-301 — remove the lyric-model suggestions  — **LANDED** ([brief](t-301-brief.md))
**First, and deliberately so.** It is Phase 2 code changed by an owner decision
(2026-08-27, PROJECT.md): **the app recommends no lyric model**. Users already have a go-to,
many are not on Ollama, and what the app owes them is connecting to whatever they already use.
Doing it before the pipeline work stacks on top keeps the Ollama assumption from being
inherited by anything in this phase.

Scope: delete `data/lyric-llms.json`, `crates/create-core/src/suggestions.rs` (230 lines) and
`crates/library/src/suggestions.rs` (121 lines), the `missing_suggestions` block and
`llm-suggestion` styling in `Setup.tsx`, the preselect path in `app/src/state/llm.ts`, and the
recommendation table in `docs/MODELS.md` — roughly 47 references across seven files. The
`pull_command: "ollama pull ..."` field is the clearest artifact of the assumption; it goes
with the rest.

**What must survive, because it is correctness rather than promotion:** the remote-model
privacy disclosure, `Option<bool>` capabilities with "capabilities unknown" never rendered as
false, and a model the user already configured always winning. None of those are opinions
about which model is good.

**The hole this opens.** The suggestion block was also the empty state: a user with nothing
configured currently gets a named model and a command to run. Afterwards they get an empty
picker. T-301 replaces it with copy that says what the app talks to and what address it just
tried, naming no model — which is the honest minimum, but only the minimum. The rest is
T-301b.

**Also removed, because nothing else uses it:** `DataDir` in `src-tauri`, whose only consumer
is the suggestion load, and the `"../data/*.json"` bundle glob, which would otherwise point at
a directory that no longer exists in a fresh clone. **The gate cannot catch that one** —
`npm run gate` runs `vite build`, never `tauri build`.

### T-301b — let the user set the endpoint and the API key  — **LANDED** ([brief](t-301b-brief.md))
**This is the task that actually delivers the owner's decision**, and T-301 is only its
clearing-up. Found while writing T-301's brief:
`DEFAULT_BASE_URL = 'http://127.0.0.1:11434/v1'` is a **hardcoded constant in five places in
`Setup.tsx`, and the wizard has no field for the endpoint at all** — nor for the API key,
although `has_key` already rides on `LlmStatus::Ready` and `SecretKey::LlmApiKey` is already
plumbed through the keychain from T-004.

So today the LLM step can only ever talk to a local Ollama on the default port. A user on
OpenAI, Anthropic, OpenRouter, LM Studio, vLLM or a LAN box **cannot connect at all**, which
is the exact capability the owner named as the one thing the app owes them. Removing the
suggestion list without this would leave the Ollama assumption fully load-bearing and merely
invisible.

**Owner decision 2026-08-27: the endpoint field ships prefilled with the Ollama address.**
Nothing regresses for local users, and a prefilled field still shows every other user what
the app has been silently assuming -- which an empty field with placeholder text would not.

**Frontend only** -- confirmed while briefing: `llm_probe`/`llm_test` already take
`base_url` as an argument, `set_secret`/`has_secret`/`delete_secret` are registered commands,
and `SecretKey::LlmApiKey` is already whitelisted. No `.rs` file changes.

Scope: an endpoint field defaulting to the current constant, an API-key field that writes through the existing keychain path and **never reads
back** — the value must not cross the boundary, only `has_key` (T-004) — both persisted to
`config.json` like the model selection T-212 fixed, and probe/test/choose taking the entered
URL rather than the constant. The `unreachable` hint that recognises a base URL missing `/v1`
(LLM-SURFACE 11.3) becomes considerably more valuable once users are typing URLs.

### T-302 — `reasoning_effort` where the app cannot enrich  — **MEASURED 2026-08-27**
The direct consequence of T-301, and the reason it is second. `reasoning_effort: "none"` — the
fix for a whole song arriving as 99% chain-of-thought — is sent **only where `thinks` is
true**, and `thinks` exists only where Ollama's native enrichment answered (LLM-SURFACE 12.3).
That rule was safe when Ollama was the assumed path. If most users are on OpenAI-compatible
APIs the app cannot enrich, **most users never get the field**, and the 44-second-before-first-
token behaviour is their default.

This is a measurement task, not an implementation one: point `llm-bridge` at a non-Ollama
endpoint, find out whether `reasoning_effort` is honoured, ignored, or an error there, and
write the finding into LLM-SURFACE before changing the rule. The repo's standing rule is that
a parameter change against a third-party surface gets measured like one — and the owner has
said testing another API is not a blocker. Outcome is a decision entry plus, if warranted, a
small change to when the field is sent. **Not a Phase 3 blocker; it blocks nothing below it.**

### T-302b — discover whether an endpoint accepts `reasoning_effort`  — **LANDED** ([brief](t-302b-brief.md))
T-302's measurement (LLM-SURFACE 13.1) found **QwenCloud honours the field**: 33.12 s -> 1.13 s
to first content and **2771 -> 235 completion tokens**, for a song no worse. The app never
sends it there, because `thinks` only exists where Ollama's native enrichment answered -- so
on a paid endpoint the current rule bills **11.8x** the tokens on every generation.

**The fix is not "send it everywhere".** Two providers honouring it is not evidence a third
will not reject it, and an unsupported parameter sent blindly turns lyric generation into an
error for whoever's endpoint is strict. That is the guess the current rule exists to avoid.

Instead, **discover it where the app already makes a call for exactly this purpose**: the
wizard's test call (`llm_test`). Probe once at configuration time, persist the answer beside
the endpoint, and send the field wherever it is known-accepted -- turning an inference from
enrichment into a verified per-endpoint fact, which is this repo's whole method. **All three open design questions were settled by measurement before the brief was written**
(LLM-SURFACE 13.3, 13.4): a rejection is a 400 naming the field, an **unknown** parameter is
accepted and silently ignored rather than rejected, and **acceptance is per endpoint while
honouring is per model** -- so the probe asks one question, "does sending this fail", and
rides on the wizard's existing test call at the cost of a second request only when it does.
Detection is **differential, never an error-message match**.

### T-303 — `default_profile_id` persistence and the profile picker  — **LANDED** ([brief](t-303-brief.md))
The same class as T-212 — a value the wizard never writes, degrading silently to
`ace-step-1.5-turbo` — and Phase 2's close assigned it here because this phase owns the
picker. Fourth instance in this repo of a command or setting with no caller, so the brief
should also say how the test would have caught it: assert the file on disk, not the store.

Scope: persist the selection, load it, and a picker in AudioStudio that lists the loaded
profiles with their licence terms (the per-model licence rule from T-111 applies here too —
users ship these tracks commercially).

### T-304 — `create-core`: semantic-to-slot resolution  *(pure)*  — **LANDED** ([brief](t-304-brief.md))
The type Phase 3 revolves around and the provenance sidecar records. **`GenerationSpec`,
`InputValue`, `LoraRef`, `LyricRef` and the `ResolvedSlots` alias already exist** (T-003,
found while briefing) -- what has never existed is `resolve_slots`, which turns the spec
into the concrete `address -> value` list actually submitted. The task is the fan-out, not
the type.

This is where the **two traps the profiles exist to hide** get hidden: duration fans out to
`94.duration` **and** `98.seconds`, and one UI seed fans out to the planner seed `94.seed`
**and** the sampler seed `3.seed`. A test that sets duration and asserts one address is
vacuous — name the invariant (*both* addresses carry it) the way WORKFLOW 4.2 requires.
ARCHITECTURE 8 wants both levels stored, so `resolve_slots` output is not a throwaway: it is
half the sidecar.

### T-305 — the workflow working copy and its graph edits  *(pure, and the hard one)*  — **SPLIT**
A pure transform over workflow JSON: take a fetched template, apply resolved slots, splice
LoRA loaders, and make the save node write lossless. No MCP calls, no ComfyUI — which is what
makes the phase's riskiest code unit-testable.

Three things the brief must carry verbatim from MCP-SURFACE 16, because each is a way to get
it subtly wrong:

- **The save-node rule is on the format value, not the node class** (16.3). MiniMax ships the
  modern node set to `mp3`; a class check passes it.
- **`widgets_values` is positional and its length varies by format** (16.1). Writing
  `[prefix, "flac", "V0"]` leaves a stray sub-widget value from a format that has none.
- **LoRAs need node insertion, not slot-setting** (4): `LoraLoaderModelOnly` spliced between
  `UNETLoader` (104) and its downstream consumer, per the profile's `attach_after`, stacking
  in order.

**Split in two at briefing time**, because link rewiring needs its own reference code and the
pair exceeds the ~400-line run limit:

- **T-305a — the save node**  — **LANDED** ([brief](t-305a-brief.md)).
  `ensure_lossless_output`: the format is a positional `widgets_values` entry, the array is
  rebuilt to two entries rather than patched, and **the test is the format value, not the
  node class** -- MiniMax ships the modern node already set to `mp3`. Landed with the three
  briefed mutations killed and two more the review added: the whole-document comparison
  (a version that deleted `links` had passed) and a second save node.
- **T-305b — the LoRA splice**  — **LANDED** ([brief](t-305b-brief.md)). Inserting `LoraLoaderModelOnly`
  nodes and rewiring the MODEL chain. In the ACE-Step fixture that chain is `104 -> 78 -> 3`
  with the profile's `attach_after` at `104`, so the splice goes between 104 and 78 and
  re-sources link `260`; fresh ids come from `last_node_id` 110 and `last_link_id` 265.
  ⚠ **The reason this brief is long:** a splice that inserts the loaders but leaves the
  consumer link at the anchor **validates clean, runs, and writes audio with no LoRA
  applied** (MCP-SURFACE 17.1, produced and run live). Nothing downstream catches it, so
  the unit tests must assert the MODEL chain by traversal. Landed with all four briefed
  mutations killed; review added a fifth that found the chain test was reading the `links`
  array only, which is the one of the three edge records the engine ignores (17.8).

**Fixtures are the real captured templates**, committed to `testdata/` — not hand-written
JSON. This is the T-203 lesson generalised: a rule about model output has to run against model
output, and a rule about template JSON has to run against template JSON. Hand-written fixtures
are written to agree with the code.

### T-306 — the pipeline: the pure seam, then the command  — **SPLIT**
The `src-tauri` seam that puts T-304 and T-305 on the wire, following ARCHITECTURE 7:
`fetch_template` to a **per-job working copy** (never a shared path — the MCP docs warn about
TOCTOU), `set_slots` for everything addressable, the T-305 graph edits for what is not,
`validate_workflow`, then `run_workflow(wait=false)` into the existing job pump.

**Split at briefing time**, because two live findings turned the seam into its own task:

- **T-306a — the pure seam and a profile bug**  — **LANDED** ([brief](t-306a-brief.md)). `InputValue` is
  adjacently tagged, so `serde_json::to_value` yields `{"type":"seed","value":42}` where the
  slot wants `42`. And **the shipped ACE-Step profile writes the seed to two addresses that do
  nothing**: `3.seed` and `94.seed` are link-fed from `PrimitiveInt` 109, `set_workflow_slot`
  reports both `applied`, and the executed prompt shows the sampler reading node 109 — so every
  track would render with the template's seed whatever the user picked. `audit_slots` is the
  guard, and it distinguishes a real backend node's link (inert) from a frontend-only
  `PrimitiveNode` link (dropped at conversion, so the write lands). Landed in a new
  `audit.rs`; all six briefed mutations killed, plus two the review added.
- **T-306b — the command.** **Next up.** `generate_audio(spec)` doing fetch → slots → graph edits →
  validate → submit, then handing off to the existing `jobs::run_workflow` pump rather than
  duplicating its lifecycle. Per-job working copy under the app data dir; `local_check` gated
  before running; `Verdict::Vacuous` treated as failure, not success.

⚠ **Be precise about what step 4 buys**, in the code and not only here. Measured on the live
install, `validate_workflow` **does** catch an unknown enum value, an out-of-range number and a
missing required input — all before GPU-minutes. It is **blind** to reachability: a LoRA splice
that feeds nothing validates clean and runs (17.1). It is also documented blind to
`COMFY_DYNAMICCOMBO_V3` sub-inputs, which is the save format (16.1). A comment saying exactly
that is worth more than a test asserting a guarantee that does not exist.

Also the natural place for the OQ-3 evidence to accumulate: if the pipeline needs a node fact
the MCP surface cannot answer, record it here rather than reaching for `/object_info` silently.

### T-307 — the LoRA list: filtering, grouping, dedupe  *(pure)*
The 53-entry list is unusable raw (16.5, ARCHITECTURE 5a). A pure function over the captured
list: drop `training_state.pt` and other non-adapters, group by directory, collapse the
20-epoch checkpoint series to latest/`final` with the rest behind an expander, dedupe
case-variants, keep display names and favourites.

**The fixture is today's 53-entry list, captured into `testdata/mcp/`** — every rule except
one can be tested against it directly. The exception is dedupe: the case-variant directory
that motivated it was **described in 4 and never captured as data** (16.5), and it has been
off this install since at least 2026-08-24. So that one test needs a case-variant pair added
to the fixture **deliberately and commented as synthetic**, so nobody later reads it as
observed. The rule stays — a case-insensitive filesystem still produces it — but the brief
must be straight about which of its fixtures is evidence and which is construction.

### T-308 — AudioStudio: the profile-driven param panel
Controls rendered from the profile's `inputs`, unsupported ones simply absent — **no negative
prompt box for ACE-Step**, which has no such input. bpm, key/scale and time signature are
first-class musical controls (3). The advanced disclosure hides the LM-planner sampling
controls.

`inputs.language` is `from_node_choices` and read live from the node schema — this is the
Phase 3 half of the Phase 2 decision that kept the lyric brief's `language` a plain string so
the Lyrics Studio needs no running ComfyUI.

Derived values go in the store, not in JSX. See the note at the top of this file.

### T-309 — AudioStudio: the LoRA stack panel
T-307's output made interactive: up to `max_stack` entries, each a picker plus strength
slider, reorderable and individually bypassable, **hidden entirely when the profile has no
`loras` block**. Offer a musical strength range (about 0–2) rather than the node's
-100…100.

### T-310 — the queue panel
Pending / running / progress / failed with error text, cancel, multiple jobs. **Read
`job(action="error")`** — the normalized view added since Phase 1 (16.6) — rather than parsing
`job(action="status")`. It distinguishes `server_died` (an OOM kill, where `get_logs` still
reads across the crash) from an ordinary node failure, which is exactly the distinction the
milestone's kill-ComfyUI-mid-job check exercises.

### T-311 — output ingestion, library write, provenance sidecar
`fetch_outputs` on completion, the audio copied into `library/projects/<slug>/tracks/`, and
the sidecar written per ARCHITECTURE 8: **both levels** — the `GenerationSpec` the user chose
*and* the resolved slot values actually submitted — plus the LoRA stack (file, strength,
order), seed, `LyricRef`, the `prompt_optimized` consent flags, licence, template, and comfy
server info.

The acceptance bar is the milestone's: **a two-LoRA run must reproduce from its sidecar
alone.** A sidecar that cannot do that is a bug, not a nuance.

Record the real output format from the file rather than from intent — the app asked for
`flac`, and what it got is what provenance should say.

### T-312 — batch by seeds
N jobs from one spec, differing only in seed, sharing the queue and the ingestion path. Small,
and it is the feature that makes the two-seed trap (T-304) visible if it was got wrong.

### T-313 — custom workflow import and input mapping
ARCHITECTURE 5b, and the pressure-release valve that stops the profile abstraction becoming a
cage for users who already have working graphs — which is most serious ComfyUI users. Import
an API-format workflow, `validate_workflow` it against the live registry, map node inputs to
semantic roles (candidates pre-suggested by node class and input name), save as a user profile
indistinguishable from a shipped one.

Largest task in the phase and the most likely to need splitting.

### T-314 — Phase 3 milestone verification (live)
The ROADMAP's checklist, run by a person: tags + lyrics → queued job → track in the library
with a complete sidecar; a two-LoRA ACE-Step run reproduces from its sidecar alone; **the
output is lossless, not MP3**; an imported user workflow generates; kill ComfyUI mid-job →
clean failed state and retry.

Phase 2's milestone is the reason this is a numbered task with a written checklist rather than
a click-around: the automated steps passed first time and found nothing, and **every defect
came from a person clicking**. Budget for fix-ups after it (T-315+), because there will be
some.

Two extras to capture while a real generation is running, since nothing else in the repo can:
**the actual VRAM behaviour** on the 15.9 GiB card, which is the only way to settle
`vram_gb_min`; and a **full-length** run, since every generation so far has been 10 seconds.
