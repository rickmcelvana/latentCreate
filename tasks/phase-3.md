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
- **T-306b — the command**  — **LANDED** ([brief](t-306b-brief.md)). `generate_audio(spec)` doing fetch →
  audit → slots → graph edits → validate → submit, then handing off to the existing
  `jobs::run_workflow` pump rather than duplicating its lifecycle. Per-job working copy under
  the app data dir; `Verdict::Vacuous` treated as failure, not success.
  ⚠ **Correction, made while briefing it: `local_check` is NOT a gate.** An earlier line here
  said it was. It is evaluated at fetch time, *before* the profile's `slot_overrides` are
  applied — and MiniMax Music 3 is `runnable: false` for exactly the filename its own override
  corrects (14.4). Gating on it would refuse to generate with a fully installed model, which is
  the same mistake the models step already documents at length. `validate_workflow` on the
  **edited** copy is what replaces it.
  Also settled here: the audit runs **before** the first slot write, so a profile bug costs
  nothing; `unchecked` addresses are reported, never a refusal (MiniMax's seed is one, 18.5);
  and the mock transport moves behind a `test-support` feature so the four-call sequence can be
  asserted with no ComfyUI running — the first `src-tauri` command that makes more than one
  MCP call, and ordering is the only thing that can go wrong in it.
  **Landed with all five briefed mutations killed and a sixth the review added:** the briefed
  suite proved *"a bypassed LoRA is not spliced"*, which passes on a pipeline that splices
  nothing at all. Splicing into a throwaway clone left `lora_nodes` correctly populated, the
  gate green and no compiler warning, while the submitted graph carried none of the user's
  LoRAs — 17.1 one layer up. The added test reads the loader node back out of the submitted
  file. **Sixth task running where "nothing bad was found" passed because nothing was looked
  at.**

⚠ **Be precise about what step 4 buys**, in the code and not only here. Measured on the live
install, `validate_workflow` **does** catch an unknown enum value, an out-of-range number and a
missing required input — all before GPU-minutes. It is **blind** to reachability: a LoRA splice
that feeds nothing validates clean and runs (17.1). It is also documented blind to
`COMFY_DYNAMICCOMBO_V3` sub-inputs, which is the save format (16.1). A comment saying exactly
that is worth more than a test asserting a guarantee that does not exist.

Also the natural place for the OQ-3 evidence to accumulate: if the pipeline needs a node fact
the MCP surface cannot answer, record it here rather than reaching for `/object_info` silently.

### T-307 — the LoRA list: filtering, grouping, dedupe  *(pure)*  — **LANDED** ([brief](t-307-brief.md))
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

**Corrected 2026-08-28, writing the brief.** Two things above were wrong. **(a) The fixture
did not exist.** "The fixture is today's 53-entry list, captured into `testdata/mcp/`" was
written as though it were already there; nothing in `testdata/` held the list. It is captured
now (`a5424eb`), verbatim, `stale: true` and all, plus the hand-built case-variant file this
paragraph asks for. **(b) Favourites and user display names move to T-309.** Both are
persisted user state keyed on the entry path, they belong with the store and the panel, and
either one would put a second argument on a function whose value is having one.

**And one thing the fixture cannot do.** The real list has a `loragoth\final\`, which
supersedes every epoch checkpoint -- so on the captured data the epoch number is never
compared to anything, and the likeliest bug in the module (comparing `checkpoint-epoch-N` as
text, where 90 beats 300) is invisible to it. The test that catches it replays the same real
paths with `final/` removed: a training run still in progress.

**LANDED 2026-08-28.** `create-core` 126 -> 137 tests. The executor reproduced the brief's
reference byte for byte, so the whole value of the run was the review after it -- and the review
found two more sorts the captured list cannot test. ComfyUI returns `choices` already sorted, so
the group ordering and the within-group ordering are both satisfied by accident on the fixture:
delete either sort outright and all fourteen other tests still pass. Closed with
`test_the_catalog_does_not_depend_on_arrival_order`, which runs the same real list reversed and
asserts the groups come out identical. Ten of ten mutations killed.

Same shape as the epoch trap above, and the third task running where a suite proved an absence
and never the presence. Worth stating as a rule for T-308 onward: **when the input a fixture
captures already satisfies a rule, the rule is untested** -- feed it the same real data with
that property removed.

### T-308 — AudioStudio: the profile-driven param panel  — **T-308a + T-308b LANDED**, click-through passed 2026-08-28; T-308c LANDED, click-through passed and corrected it
Controls rendered from the profile's `inputs`, unsupported ones simply absent — **no negative
prompt box for ACE-Step**, which has no such input. bpm, key/scale and time signature are
first-class musical controls (3). The advanced disclosure hides the LM-planner sampling
controls.

`inputs.language` is `from_node_choices` and read live from the node schema — this is the
Phase 3 half of the Phase 2 decision that kept the lyric brief's `language` a plain string so
the Lyrics Studio needs no running ComfyUI.

Derived values go in the store, not in JSX. See the note at the top of this file.

**Split 2026-08-28, writing the brief. T-308a landed the same day** ([brief](t-308a-brief.md));
T-308b is the data path and the panel, and gets its own brief.

Whole-of-T-308 is about 1100 lines -- two Rust commands with their view types, the bridge, the
panel model, the component and its CSS -- nearly three times the 400-line rule, and T-306a
already stalled on brief size once. **T-308a** is the pure half, testable with no ComfyUI: the
profile's declarations become ordered controls with defaults, bounds, a basic/advanced split,
and a typed `GenerationSpec.inputs`. Architect-direct lane (WORKFLOW 1). **T-308b** is the two
commands, `<ParamPanel>`, the CSS and the copy -- Aider lane.

**Four things T-308a settled that T-308b inherits.** (1) A `u64` seed cannot survive
JavaScript: above 2^53-1 it changes on the way through and JSON cannot carry a BigInt, so
18446744073709551615 reaches Rust as ...616 and lands in the sidecar. The panel **refuses**
seeds above `Number.MAX_SAFE_INTEGER` rather than rounding, and the cap needs UI copy. (2) An
`unsupported` input is recorded with its reason, never filtered away -- a missing
negative-prompt box and a forgotten one look identical otherwise. (3) `keyscale`,
`timesignature` and `language` are `from_node_choices` with **no** local list, so they are
empty until something asks the node registry -- and T-308b must not render that like an
unsupported input; one means ComfyUI is off, the other means the model has no such input.
(4) Group members inherit their group's `advanced` flag, or the planner's five sampler
controls surface in the basic panel while the group hiding them is hidden.

**T-308b split again, by lane** ([brief](t-308b-brief.md)). Its data path -- the
`profile_inputs` command, the bridge call, the panel store -- is small and test-bearing, so it
was written and verified by the architect and is landed. `<ParamPanel>` is ~300 lines of JSX
and CSS carrying no logic, which is what the executor lane is for, and it is briefed with an
exact class list, the copy written out, and a click-through.

**T-308c -- live enum choices.** The profile declares `slots: ["94.keyscale"]`, and `94` is a
node *instance* inside the template, not a class. Reading the live options means resolving it
to `TextEncodeAceStepAudio1.5` first, and only the workflow file holds that hop. `InputSpec`'s
`Enum` gains an optional `node` field naming the class; the input name already comes from the
slot address's field part. Until then the three controls render disabled, saying to start
ComfyUI -- which must not read like `negative`'s recorded "this model has no such input".

### T-309 — AudioStudio: the LoRA stack panel  — split a/b/c, **T-309a LANDED 2026-08-28**
T-307's output made interactive: up to `max_stack` entries, each a picker plus strength
slider, reorderable and individually bypassable, **hidden entirely when the profile has no
`loras` block**. Offer a musical strength range (about 0–2) rather than the node's
-100…100.

Split the way T-308 was, and for the same reason — the pure half is where the invariants live
and the JSX half is where the tokens go:

- **T-309a** ([brief](t-309a-brief.md)), architect-direct: `Serialize` on the catalog types, the
  `lora_panel` command, `state/loras.ts` and its store. Seven invariants, each with a test.
  **LANDED**: create-core 137 -> 138, app 58 -> 65, frontend 162 -> 195. Sixteen mutations,
  sixteen killed -- but only after two of them exposed vacuous assertions of exactly the kind
  T-307 warned about. ACE-Step's default strength is `1.0` and its range is `0.0..=2.0`, so
  `strength: panel.strength.default` and `strength: 1`, and clamping to the range versus
  clamping to a literal `0..2`, were **indistinguishable** on the only profile in the fixture.
  Both now run against a second profile with different numbers. The Rust half had the same hole
  one level up: `lora_panel` reads `loras.strength` with the node's own `-100..=100` in scope at
  the same call site, and nothing tested the choice -- `panel_for` was extracted so it could be.
- **T-309b** ([brief](t-309b-brief.md)), Aider: `<LoraStack>`, `theme.css`, AudioStudio wiring.
  **LANDED 2026-08-28, click-through passed.** The run came back clean -- three files, 241
  added lines, no rule in `theme.css` changed, every class ruled, tokens only, and the test
  count held at 205 exactly as the brief required. Review found two things. The `<option>`
  composed `${label} (epoch ${n})` in JSX, which is **conditional wording** and therefore
  unreadable by a DOM-less vitest -- and it was in the brief's own reference, the second time
  this phase a brief specified the defect. `PickerEntry` now carries `display`, with both
  directions mutated. Three buttons also lacked `type="button"`; harmless with no `<form>` in
  the tree, and a submit the moment one appears. Frontend 205 -> 206.
  The click-through passed every row: MiniMax removes the panel entirely, ComfyUI down leaves it
  visible with a working Retry, Remove returns an entry to the picker, bypass dims without
  removing, `loragoth` shows only `final` until the 20 checkpoints are disclosed, no
  `training_state.pt` anywhere, and the reorder arrows disable at the ends. **It also settled the
  label question**: the labels stay mechanical, which largely removes T-309c's premise.
- **T-309c**: favourites and user display names. **Recommend backlogging rather than
  scheduling.** Display names existed to rescue labels the owner would find unusable, and the
  click-through says he does not; favourites over twelve entries is thin on its own. Both are
  persisted user state keyed on the entry path, so neither is free -- they are a config-schema
  change for a problem that has not appeared.

**Two live findings while briefing it** (MCP-SURFACE §19.3). The 53-entry list is unchanged and
a live read again carries neither staleness signal. And a claim written into §19.1 the day
before was **wrong**: a `lora_name` the server does not know is rejected by `validate_workflow`
as `unknown_enum_value`, so a deleted LoRA under a stale picker is a rejected job, not a silent
no-op. 17.6's silence belongs to the *non-adapter* case, which validates clean because it is a
real member of the enum. A stale list is therefore a **short** list, and the panel's cache note
says what is missing rather than cautioning about what is shown.

### T-309d — Generate: the panel becomes a job  — **LANDED 2026-08-28**, click-through generated the project's first track ([brief](t-309d-brief.md))
Part 1 (spec assembly, blockers, submit store, `jobs.register`) architect-direct; Part 2
(`<GenerateBar>`, CSS, wiring) the Aider run. Frontend 206 -> **239 tests**, fourteen mutations,
fourteen killed -- including the one that reproduces the shipping bug: delete the `register` call
and the queue never hears about the job.

**The Aider run came back clean** and held the test count at 236 exactly as the criterion asked.
Review added one thing the brief had not thought of: a submission's notes outlived the settings
that produced them, so generating two LoRAs on ACE-Step and then switching to MiniMax left
`2 LoRAs applied.` sitting under a model with no LoRA support and no panel to show for it.
`notesFor` keys them on the profile -- rather than clearing on mount, which would wipe them for
anyone who generated, checked their lyrics and came back. The two button labels also moved into
`generate.ts`; that one is consistency with the T-309b rule rather than a defect, since unlike
the epoch label they carry no information a test needs to check.

**The click-through generated the project's first audio** and found one blocker. MiniMax queued,
tracked to `completed`, and wrote a playable FLAC -- which also proved the lossless swap live
(the template ships that node set to `mp3`/`V0`) and settled MCP-SURFACE 18.5 by reading
`GET /history/<prompt_id>`: MiniMax's seed does reach the sampler, through `37/38.seed` alone,
and four of the seven "could not be checked" addresses turn out to have applied. **ACE-Step
failed outright** -- `has no input named cfg_scale` -- because the panel flattened
`planner.cfg_scale` to `cfg_scale` while `flat_inputs` keeps the dot. Both sides' tests passed;
neither crossed. Fixed, with the flattened list now a contract both languages assert against.

**A sequencing mistake worth not repeating:** the brief shipped with an Aider launch command
while its architect-direct Part 1 was still unwritten, so the executor stopped and asked for four
files that did not exist. It was right to. A two-part brief must say which part lands first, and
the launch command belongs with the part that is actually runnable.

Found while closing T-309b. **Nothing in the UI can start a generation.** `generate_audio(spec)`
has had no caller since T-306b, `specInputs` none since T-308a, `specLoras` none since T-309a --
three tested seams meeting nowhere. The Audio view has no Generate button.

This matters for the ordering: **T-310's queue panel shows jobs nothing can create**, so its own
click-through would have to enqueue work by hand. Assembling the spec and submitting it should
come first.

Scope: a Generate button on AudioStudio; `GenerationSpec` built from `specInputs(model, values)`
plus `specLoras(stack)` plus the profile id and a `LyricRef`; the `generate_audio` bridge call;
the button's disabled states. Split a/b in the brief: the spec assembly and store are
architect-direct, the component is Aider.

**Briefing it found the defect that would have shipped.** `generate_audio` starts the job pump
itself, and the frontend store learns of a job only through `useJobsStore.run()`, which this
path does not call -- while `applyJobEvent` correctly ignores events for ids it does not know.
So Generate would have run a full generation on the GPU with **every progress, done and failed
event silently discarded**: an empty queue panel, no error, nothing in the log. Fifth instance
this phase of *a guard in one layer does not bind the layer above it*, and the first where both
layers are right on their own and wrong together. Part 1 adds `register(id)`.

**Two corrections to this entry's own earlier scope.** ComfyUI-not-connected is **not** a
blocker: `generate_audio` calls `ensure_connected`, which spawns `comfy-mcp` itself, so
disabling on `jobs.connected` would leave the button dead on every cold start. And the
`LyricRef` gets a rule rather than being passed through -- it is attached only when the lyric
text being submitted is byte-identical to the approved version's, because a ref naming v2 beside
v3's words is exactly the provenance error T-311's "reproduces from the sidecar alone" bar
cannot survive. That closes, cheaply, the gap PROJECT.md deferred to T-311 on 2026-08-27.

### T-309e — the audit could not read a subgraph  — **LANDED 2026-08-28**, click-through passed ([brief](t-309e-brief.md))
**This entry's own earlier scope was wrong, and correcting it is the task.** It said the three
inert addresses "are three of the seven 'could not be checked' warnings a MiniMax user sees".
They are three of **eight**, and the other five are equally unchecked -- because `audit_slots`
refused any address containing a slash, and **every address MiniMax declares is a subgraph
interior**. The warning had a 100% false-positive rate and had fired on every generation this
project has ever run, naming `37/6.unet_name` (no model loads without it) and `37/38.seed` (which
18.5 proved reaches the sampler). Trimming three would have left five.

**The two halves cannot ship apart.** `generate.rs` does not warn about an inert address, it
**refuses the run** -- so teaching the audit to see a subgraph without dropping the three
link-fed addresses would have stopped MiniMax generating at all. MiniMax was passing that guard
only because the audit was blind. Sixth instance this phase of *a guard in one layer does not
bind the layer above it*, and the first where the **blindness** was load-bearing.

No new measurement: 18.5's live `GET /history` table already had the ground truth, and a
structural read of the committed fixture reproduces it exactly. The `is_inert` rule needed one
new answer (the subgraph's `inputNode` boundary is a promoted widget, not a driving edge), not a
new rule. Full evidence in [MCP-SURFACE 22](../docs/MCP-SURFACE.md).

create-core 139 -> 148, src-tauri 69 -> 70. **Nine mutations, nine killed**, control survived.
Two worth naming. The profile edit is an **absence** -- three addresses that are no longer
written -- so it needed a test that reads the profile, the same shape as M49 on the cancel task.
And `test_subgraph_address_is_unchecked` had to be *inverted* rather than updated, which is how a
rule gets deleted by accident: it was replaced by a test asserting the live table, not the
implementation. The seam test lives in `src-tauri`, because a clean `audit_slots` is one layer and
`Submission.unchecked_slots` is what the user reads.

**Click-through passed 2026-08-28.** MiniMax generates with no warning line at all, ACE-Step is
unaffected, and both wrote files. The refusal path was the thing to watch -- a MiniMax run failing
with "writes ... to inputs a node drives" would have meant the profile edit had not landed with the
audit edit -- and it did not fire.

### T-310 — the queue panel  — **T-310a LANDED 2026-08-28** ([brief](t-310-brief.md)); **T-310b briefed** ([brief](t-310b-brief.md)), Aider lane, ready to run
Pending / running / elapsed / failed with error text, cancel, multiple jobs.

⚠ **This entry told the implementer to do the wrong thing, and a live read caught it**
(MCP-SURFACE §23). It said to read `job(action="error")` rather than parsing
`job(action="status")`. Measured against three real cancelled jobs, `action="queue"` and
`action="error"` **swap** on the same job in opposite directions, while `action="status"` answers
`cancelled` for all three and is stable on repeat calls. Following this entry would have made two
of three cancels arrive as `error` — reintroducing the §21 defect of reporting the user's own
cancel back to them as a failure. **The app's current polling surface is correct and T-310 changes
none of it.** `action="error"` also returns two shapes with mutually exclusive keys, and a
*cancelled* job comes back `error: null` despite the tool documenting that key as meaning healthy —
third time in this project an absent key was being read as a value.

**Also settled by the read:** there is no progress data on any surface the app uses
(`JobProgress` is `{id, status, outputs}`), so the panel shows **elapsed time**, timestamped
locally at `register` because `submitted_at` exists on only one of comfy-cli's two stores.

**The failure case was then produced deliberately** (owner's go-ahead, MCP-SURFACE §24): an
ACE-Step graph pointed at MiniMax's VAE -- a legitimate enum member and the wrong file, so it
validates, runs, and throws at `VAEDecodeAudio`. That softens the line above in one direction:
**T-310 needs both surfaces.** The *outcome* comes from `action="status"`; the failure *detail*
(`node_id`, `node_type`, `exception_type`, `exception_message`) comes from `action="error"`, which
is the only surface carrying it in named fields. `traceback_tail` is never rendered.

It also found that `error` inside `action="status"` has **two shapes that share no key** -- `code`
is absent on the failure shape, `exception_message` absent on the cancel shape -- so classifying an
outcome by reading `error.code` silently returns nothing for every real failure. `mcp-bridge`'s
docs said this payload's shape was "NOT yet captured"; it is captured now, verbatim, in
`testdata/mcp/job_outcomes.json`, and the failure test's hand-written fixture (which matched
neither real shape) was replaced with the payload the server actually sends. mcp-bridge 94 -> 96.

**Two things still unmeasured.** `server_died` -- producing it means actually killing ComfyUI,
which is T-314's own check. And the fallback this entry assumed: `get_logs` returned a file a day
stale, reporting v0.34.1 against a running v0.34.2, while comfy-cli's own trust signals both said
it was fine -- **a server restarted by hand has no log for `get_logs` to read**, and nothing in the
response says so (§24.5).

The panel today is **32 lines** written incidentally across T-309d and the cancel fix: a raw status
string, error text, and a Cancel button. A row cannot say *which* job it is, which is why during the
cancel investigation a frozen row and a live row were indistinguishable on screen.

**T-310a LANDED 2026-08-28.** `state/queue.ts` (ordering, labels, elapsed, the Cancel condition,
the error rule), `Job` gaining `profileId` and a locally-stamped `submittedAt`, and
`register(id, profileId)`. Frontend 245 -> **263**, src-tauri 70 -> **73**. Ten mutations, ten
killed.

⚠ **The brief said "no Rust change". It was wrong, and the §24 measurement is what showed it.**
`failure_reason` read `status.error.as_str()` and nothing else -- correct for none of the shapes
comfy-cli sends. A real failure's payload is an *object*, so `as_str()` returned `None` and the
fallback rendered the entire error as the bare word **`"error"`**. Every test passed throughout,
because the only fixture was a hand-written `error: json!("node blew up")` -- a string nobody has
ever observed. Now handles all three shapes and is asserted against the committed capture. This is
the fourth time this phase that a suite was green because its fixture was written to agree with the
code; the difference here is that the real payload existed on disk by the time the test was written.

Also removed while writing it: a test asserting `queueRows` does not reorder its input, which
**cannot fail** -- `Object.values` already returns a fresh array. Caught by oxlint suggesting
`toSorted`, which made the redundancy visible.

**Deliberately consumer-less until T-310b.** `state/queue.ts` has no caller yet: `<JobQueue>` still
renders the raw status. That is the same "tested seams meeting nowhere" shape T-309d was written to
fix, and it is acceptable only because T-310b is the next task rather than a someday one.

**`modelName` added while briefing T-310b**, because the alternative was shipping raw profile ids on
screen -- and choosing between a display name, an id and a fallback is a conditional, so it belongs
in the pure module rather than in the JSX an executor writes. It also caught a hole in its own first
draft: `??` guards `undefined` but not `''`, so a profile carrying `"display_name": ""` blanked the
column -- the one outcome the function exists to prevent. Frontend 263 -> **268**; four more
mutations, three killed and the control alive.

**T-310b briefed** ([brief](t-310b-brief.md)). Three files, tokens only, test count pinned at 268.
⚠ It carries one live fix: **`.job-item-failed .job-status` has never applied**, because a real
failure's status is `error`, not `failed`. The rule has been dead since it was written and nobody
could see it, since no generation had ever failed until 24 was measured.

### T-311 — output ingestion, library write, provenance sidecar — **SPLIT 2026-08-29; T-311a briefed** ([brief](t-311a-brief.md), Aider lane)

**Split into three, because one task here is far past a 400-line diff and the parts have different
verification stories.**

- **T-311a** — the offline half: `Project::next_track_seq`, `create_core::audio::flac_duration_s`,
  and `library::tracks` (mint, save, load, paths). **LANDED 2026-08-29** ([brief](t-311a-brief.md));
  create-core 153, library 51. Review found a vacuous test, an untested id whitelist and a
  short-read bug, all fixed before the commit.
- **T-311b** — ingestion. **LANDED and VERIFIED LIVE 2026-08-29** ([brief](t-311b-brief.md)).
  A two-LoRA ACE-Step run generated from the app wrote `tracks/tr-0001.flac` (real FLAC,
  120.000 s, duration read from the header) and its sidecar, and **the sidecar matches what
  ComfyUI actually executed field for field** -- checked against `GET /history`, not against
  our own tests (MCP-SURFACE 27). The milestone's bar is met. src-tauri 82.
- **T-311d** — `Provenance.prompt_id`, so a track can be traced to the run that made it.
  **LANDED 2026-08-29** ([brief](t-311d-brief.md), architect-direct). Numbered `d` but landed
  **before** `c`, whose number was already published; numbers are labels, not an ordering.
- **T-311c** — the Library's **data path**: `list_tracks`, the `library_tracks` command, the
  bridge, and `state/library.ts`. **LANDED 2026-08-29** ([brief](t-311c-brief.md)); library 55,
  frontend 285. Review found the module could not be called `library` -- `src-tauri` depends on a
  crate of that name, so `mod library;` shadowed it crate-wide. Renamed `tracks`.
- **T-311e** — `<Library>` itself. **LANDED 2026-08-29** ([brief](t-311e-brief.md)) and **passed
  its click-through, all five steps**: the existing track renders with its recipe and *no* Run line
  (it predates T-311d), a new track appears **without a reload** and does have one, and the recipe
  grid wraps with no sideways scroll. Step 3 was the first thing ever to exercise `track://saved`
  end to end. Two files, test count held at 285. Review reverted an out-of-scope `.lyrics-draft`
  edit and backlogged a third identical retry rule.

**Three claims in the original entry were checked while briefing, and two were wrong.**

1. It reads as though the sidecar must be designed. **`Track`, `Provenance` and `ComfyServerInfo`
   already exist** in `create-core/src/provenance.rs` with a two-LoRA round-trip test, landed in
   T-003b. T-311 writes them; it does not define them.
2. It says the sidecar needs "both levels". Correct, and already the shape of `Provenance`
   (`spec` + `resolved_slots`).
3. "Record the real output format from the file rather than from intent" — correct and now
   verified: the produced file is genuinely FLAC at 48 kHz/16-bit/120.000 s (MCP-SURFACE 26.2), and
   its duration is readable from 42 bytes of header, so `duration_s` is measured rather than copied
   from what the user asked for.

**The design problem this entry does not mention, and T-311b's real work:** provenance needs the
spec, the resolved slots and the profile's licence, all of which exist only at *submit* time, while
ingestion happens at *completion*. `generate_audio` retains none of it. Recomputing `resolve_slots`
later is not equivalent — it records what the app would resolve now, not what it did. Details and
the likely shape are at the bottom of the T-311a brief. Also unresolved: **which project a track
belongs to**, since `generate_audio` takes no slug and only `lyricdoc.rs` has a `default_project`
helper.

#### The original entry, as written before any of the above was checked

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
**LANDED 2026-08-29** ([brief](t-312-brief.md)) and **passed its click-through**: a batch queued,
the queue listed it as expected, and the Library's recipe cards showed a **different seed per
batched track** — the acceptance check, since the audio could never have been one. Frontend
285 -> 299. Review found four things in the executor's tree, one of them caused by the brief
(see the session log): the gate was red on two unused imports; `notesFor` kept the previous
submission's notes through a click that queued nothing; the button could count to a number it
would never reach; and `Queueing…` lost its ellipsis while the new select lost its focus ring.

**Two of the original entry's claims were checked against the code and both were wrong.** The
original read: *"N jobs from one spec, differing only in seed, sharing the queue and the ingestion
path. Small, and it is the feature that makes the two-seed trap (T-304) visible if it was got
wrong."*

1. **"Sharing the queue and the ingestion path" is already true and needs no work.** `mint_job_id`
   gives every call its own working copy, `pump` stores a `PendingTrack` per prompt id,
   `ingest_outputs` mints one track per output, and `queueRows` already lists live jobs oldest-first
   (T-310d). The Rust side needs no change; **the task is the frontend loop that was never
   written**, four files.
2. **The two-seed trap is closed and cannot be the acceptance check.** The seed fans out to *one*
   address in both shipped profiles — `109.value` (T-306a's `PrimitiveInt` redirect) and
   `37/38.seed` (settled live, 18.5). The surviving fan-out is `duration_s`, which a batch does not
   vary. And "the variations sound different" proves nothing either: ACE-Step is not reproducible
   run-to-run on a fixed seed (17.3). **The check is four sidecars carrying four different seeds.**

### T-312b — serialize ingestion (measured 2026-08-29: not warranted yet)
**T-312's click-through watched for this and the window did not open** — the batch's tracks
appeared in the Library one at a time, never two together, which is what ComfyUI running one job
at a time predicts. Left unbriefed on that evidence. Revisit if T-313's imported workflows, a
remote `comfy_target`, or anything else makes two jobs finish at once.

`ingest_outputs` does an unguarded read-modify-write of `project.json` — load the project, mint the
id, save — with one tokio task per job and no lock between them, so two overlapping completions
could mint the same `tr-NNNN`. **Never observed**, and narrow: ComfyUI runs one job at a time, and
an ingest is seconds against a minutes-long generation. T-312 is the first thing that makes
back-to-back completions ordinary, so its click-through is the first real evidence about the gap.
Fix is a mutex in `ComfyState` held around `ingest_outputs` only — never across the `fetch_outputs`
await — plus a two-thread test. Write the brief on what the run shows, not on this paragraph.

### T-313 — custom workflow import and input mapping — **SPLIT 2026-08-30 into a … e**
ARCHITECTURE 5b, and the pressure-release valve that stops the profile abstraction becoming a
cage for users who already have working graphs — which is most serious ComfyUI users. Import
a workflow, `validate_workflow` it against the live registry, map node inputs to semantic roles
(candidates pre-suggested by node class and input name), save as a user profile indistinguishable
from a shipped one.

**Scoped live 2026-08-30 before splitting** (MCP-SURFACE 29), which corrected the design in one
important way: **the import takes the *frontend* format (`File > Save (As)`), not API format.**
`list_workflow_slots` refuses an API export outright, and slots are the entire parameter mechanism
— 5b's stated flow would have reached the mapping screen with zero mappable parameters.
ARCHITECTURE 5b and the decisions log are corrected.

Two things the scoping found already built, which is most of why this splits cleanly:

- `ComfySpec.workflow: Option<String>` **already exists** in the profile schema.
- `list_workflow_slots` **already reports each slot's node class and widget type**, already
  modelled as `mcp_bridge::Slot`. 5b's "pre-suggested by node class and input name" needs no new
  bridge work at all.

**T-313a — the pipeline honours `comfy.workflow`. LANDED 2026-08-30**
([brief](t-313a-brief.md), architect-direct). `place_working_copy` takes the working copy from a
gallery template or a copy of an imported file, and refuses an API export up front by naming
`File > Save (As)` — because `validate_workflow` accepts that shape and calls it valid, so the run
would otherwise have failed three steps later talking about inert slots. src-tauri 87 → 93; three
mutations, three killed. **Review found one defect the brief's own reference code carried**: the
format check reported against the working copy under `jobs/`, not the user's file.
**Click-through passed 2026-08-30**: a hand-written `my-import.json` pointing at a
`File > Save (As)` export generated fine, and repointing it at an API export gave the refusal
naming the right menu item. It also found one copy defect — the message rendered a literal `--`
to the user, the only user-facing string in the app that did.

Below is the original entry; today `build_and_submit` refused outright:
`"declares no gallery template; imported workflows are not wired up yet"`. Replace step 1 with
"place this job's working copy", from either a gallery template or a copy of the imported file.
**Deliberately first**: it is the smallest part, and because user profiles already load from
`config_dir/profiles`, it makes a hand-written profile pointing at any workflow generate — the
whole point of 5b — before a single screen exists. It is also the only part T-314's "an imported
user workflow generates successfully" strictly needs.

**T-313b — import and inspect. LANDED 2026-08-30** ([brief](t-313b-brief.md), architect-direct).
`create_core::workflow::detect_format` tells the three shapes apart (frontend / API / neither) and
`import_workflow` stages a copy, validates and reads slots **on the copy**, then commits — so a
refused import leaves nothing behind and the report describes the bytes that were kept. Copying
rather than referencing is an owner decision (decisions log): a profile pointing at a live file
would make every earlier sidecar a lie. create-core 154 → 157, src-tauri 93 → 101; three mutations,
three killed **after the first was made killable** — validating the source instead of the staged
copy passed all 101 tests until the happy path was made to assert which file ComfyUI was asked
about. Click-through deferred to T-313e, when there is a button.

**T-313c — role suggestion. LANDED 2026-08-30** ([brief](t-313c-brief.md), architect-direct).
`create_core::roles::suggest_roles` ranks candidates per semantic role over two real captured slot
lists. Scoping it found the rule that decides the design: **name matching alone produces a mapping
the pipeline refuses.** ACE-Step's `3.seed` and `94.seed` are both named `seed`, both `INT` and both
**inert** — driven by `PrimitiveInt` 109, so `build_and_submit` would refuse to generate. The seed
lives on `109.value`, whose name and class match nothing. So suggestion reads the graph, drops what
`audit` calls inert, and hops to the driver. The duration role goes the *other* way: both its slots
are link-fed and both land, because `PrimitiveNode` is frontend-only. create-core 157 → 164; three
mutations, three killed.

**T-313d — profile emission. LANDED 2026-08-30** ([brief](t-313d-brief.md), architect-direct).
`create_core::emit::build_profile` plus the `save_imported_profile` command: accepted mappings
become a real `ModelProfile` in `config_dir/profiles/`, verified by loading the written **file**
back through `library::profiles::load` rather than by a struct round trip. Bounds come from the live
node registry and a numeric role without them is **refused rather than guessed**; lyrics never get a
default and tags do. create-core 164 -> 173, src-tauri 101 -> 104; three mutations, three killed.

**T-313e — the import data path and store. LANDED 2026-08-30** ([brief](t-313e-brief.md),
architect-direct). `ImportReport` now carries ranked suggestions, and `state/import.ts` holds every
decision: the pre-tick rule, the row model, the save condition. **The rule it exists to carry is
that a `possible` candidate is never pre-ticked** — otherwise T-313c's confidence field is
decoration and the app silently accepts its own graph-shape guess as the user's seed mapping, with
nothing erroring. Frontend 299 -> 310; two mutations, two killed.

**T-313f — the view. LANDED 2026-08-30** ([brief](t-313f-brief.md), architect-direct).
`<ImportWorkflow>` in the Audio view's profile-picker section: pick a file, map the roles the app
suggested, name it, save. Renders the store and derives nothing -- frontend **stays at 310**, which
is the proof. `@tauri-apps/plugin-dialog` needed no plumbing; it was already wired end to end.
**Its click-through is owed and carries three others** (T-313b, T-313d, T-313e), and step 5 of it is
the Phase 3 milestone line *"an imported user workflow generates successfully"*.

### T-315 — the crash path says what to do about it
**LANDED 2026-08-29** ([brief](t-315-brief.md), architect-direct lane). `transport_reason` gives
the poll-failure path the vocabulary `failure_reason` already had for node failures: ~400
characters of tool diagnostics became one sentence ending in a next step, and the diagnostic moved
to `session.log` rather than being deleted. Two codes mapped, both verified — `server_not_running`
and `prompt_not_found` — and `server_died` deliberately absent, because the app never sees it.
src-tauri 83 → 87; four mutations, four killed, including the call-site unwiring that only the
updated wiring test catches. **Click-through passed 2026-08-29, all five steps** — a killed
ComfyUI gave a row reading exactly the mapped sentence at 6s, the full diagnostic was in
`session.log` under `job_status` with `ok:false`, the next run generated and wrote its FLAC, and
the library was clean. **This also discharges T-314's "kill ComfyUI mid-job → clean failed state +
retry"**, which is now observed twice: once as the defect (28.2) and once as the fix.

**Found 2026-08-29** by the producer closing ComfyUI mid-generation -- the check T-314 owed,
done early and out of order. Full evidence: MCP-SURFACE 28.

The row rendered ~400 characters of tool diagnostics, with the error code and the word
"failed" doubled, and the one actionable phrase (`run: comfy launch`) buried in the middle.
CONVENTIONS requires user-facing errors to say what to do next.

Scope: `terminal_outcome`'s `Err` arm currently renders `ComfyError::to_string()` verbatim and
has no vocabulary of its own, while `failure_reason` does careful work for the *node* failure
path next to it. Give the transport path the same treatment: a short sentence per known code,
starting with `server_not_running`, ending with the next step. **The app never sees
`server_died`** -- that code only exists in the state file after recovery, by which time the
pump has retired (28.1), so do not build the mapping around it.

Small, and it is the last thing between a crash and a queue row a person can act on.

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
