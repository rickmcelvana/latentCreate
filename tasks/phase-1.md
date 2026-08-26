# Phase 1 — Connections & setup wizard

Goal: the app can talk to the user's ComfyUI and their LLM, and a first-run wizard gets
someone from a clean install to "ready to generate". At the end of this phase the app does
not yet make music — it proves it *could*.

**Read first:** [docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md). The comfy-mcp tool surface
was verified live on 2026-08-23; the cloud documentation names different tools and is not a
guide for the local backend. Do not write a wrapper from the docs.

---

## Before T-101: verify `rmcp`  ✅ DONE 2026-08-23

Findings recorded in **[docs/MCP-SURFACE.md §8](../docs/MCP-SURFACE.md)**; T-101's brief
([tasks/t-101-brief.md](t-101-brief.md)) carries reference code compiled against rmcp 3.1.4
and run against the live server. Answers to the five questions, in short:

1. **`rmcp = "3.1.4", default-features = false, features = ["client", "transport-child-process"]`.**
   Defaults drag in the whole server half plus macros/schemars/uuid/base64 — all unnecessary.
   Everything it pulls in is permissively licensed. It raises the workspace MSRV to **1.88**.
2. **`().serve(TokioChildProcess::new(cmd))`** does the whole handshake; negotiated protocol
   `2025-11-25`. **The child is already killed on drop** — ARCHITECTURE §3's requirement needs
   no code of ours. A missing binary is `io::ErrorKind::NotFound`, which is T-110's signal.
3. **`CallToolRequestParams` (plural) is `#[non_exhaustive]`** — struct literals are a compile
   error; build with `::new(name).with_arguments(map)`.
4. **Arguments are `serde_json::Map`; results are JSON *inside a text block*.**
   `structured_content` is always `None` and no tool publishes an `output_schema`, so every
   wrapper is a two-stage decode. And ⚠ **a failing tool returns `Ok` with
   `is_error: true`**, not `Err` — unknown tool names included.
5. **`server_info` round-trips**, as do `system_stats` and `list_workflow_slots` against the
   frozen MiniMax fixture — where **24 of 25 slot addresses are subgraph-form**, making
   T-103's warning measured rather than predicted.

---

## Tasks

Briefs are written one at a time, each after the previous lands (Phase 0's rhythm). Each
gets its own `tasks/t-1NN-brief.md` when written.

### T-101 — `mcp-bridge` foundation  — ✅ **LANDED** `851bd88` ([brief](t-101-brief.md))
Stdio transport spawning `comfy-mcp`, typed `server_info` / `system_stats` wrappers,
`ComfyError` via `thiserror`. Local backend only — the cloud backend is a separate later
task, gated on verifying a live cloud endpoint.

The `ComfyBackend` trait itself (ARCHITECTURE §3) is **deferred to T-104**: with one impl it
would be an untested abstraction, and async fns in traits are not object-safe, so the
dyn-vs-enum choice should be made when a backend first goes into Tauri managed state.
"Child killed on drop" is already provided by rmcp's transport and needs no code.

### T-102 — mock transport test rig  — ✅ **LANDED** `974642a` ([brief](t-102-brief.md))
A fake MCP peer over an in-memory pipe, so every later `mcp-bridge` task has non-live tests.
**CI must never need a running ComfyUI** (WORKFLOW §5). Build this before the tool wrappers
so they arrive with tests.

Mechanism verified 2026-08-23 before the brief: `tokio::io::duplex` is a valid rmcp
transport (rmcp implements `IntoTransport` for any `AsyncRead + AsyncWrite`), needing **no
extra rmcp feature** — so the fake peer is hand-written newline-delimited JSON-RPC, not
rmcp's server half. `testdata/mcp/list_workflow_slots.minimax.json` is the **live-captured**
`list_workflow_slots` response to serve from the mock (24 of its 25 addresses are
subgraph-form); `testdata/workflows/minimax_music3_int8.json` remains the frozen graph
itself, for T-103's own use.

**First test it owes us:** `LocalComfy::call` must turn `Ok(is_error: true)` into
`ComfyError::Tool`. T-101 landed that branch **untested** — it needs a transport, so it
could not be covered there — and it is the single finding most likely to cause a silent
bug (docs/MCP-SURFACE.md §8.3). The mock must be able to serve an `is_error` result, an
`Ok` result whose text is not JSON (→ `ComfyError::Payload`), and a well-formed payload.

### T-102b — session log + redaction  — ✅ **LANDED** ([brief](t-102b-brief.md))
ARCHITECTURE §3 requires every tool-call payload and result to be logged (redacted) to a
rotating session log for the diagnostics pane, and CONVENTIONS forbids keys ever reaching a
log. This task delivers the **session log and redaction**: an append-only NDJSON `SessionLog`
(`log_call` / `log_result`, size-based rollover to a `.1` sibling), structural `redact`
(recursive, sensitive-key-name based, word-matched), and wires `call` to log every invocation
and outcome. **Split** from the original T-102b (which also bundled stderr capture) to stay
under the ~400-line rule. **T-102c** takes stderr capture + free-text redaction.

### T-102c — stderr capture + free-text redaction  — ✅ **LANDED** ([brief](t-102c-brief.md))
The second half of the original T-102b: CONVENTIONS requires `comfy-mcp`'s stderr captured to
the session log. `LocalComfy::connect` currently inherits stderr, so comfy-mcp's diagnostics
go to the app console and are lost in a packaged build. `TokioChildProcess::new` discards the
stderr handle and defaults it to `Stdio::inherit()`; capturing it means switching to
`TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()`, which returns
`(transport, Option<ChildStderr>)`, and draining that handle on a task owned by managed state
(aborted on `shutdown`). **Mechanism verified against the rmcp 3.1.4 source this session**, so
this brief can be written without re-verification. Also adds `redact_line` (free-text redaction
for stderr and non-JSON error messages) and `log_stderr`, and swaps `redact_text_or_json`'s
raw fallback for `redact_line`. Folds in the **transport-abort mock case** (`Reply::Hangup`)
noted at the T-102b review — it exercises `call`'s transport-fault branch, which was the one
untested path in T-102b.

### T-103 — templates and slots  — **split in two; six tools is over the ~400-line limit**
All six surfaces were captured live on 2026-08-24 before either brief: **MCP-SURFACE §9**.

#### T-103a — templates  — ✅ **LANDED** `3c9ea38` ([brief](t-103a-brief.md))
`search_templates`, `get_template`, `fetch_template`. The task's real content is
**`local_check` as a tri-state**: `{"checked": false}` means the comparison could not be
made and has **no `runnable` key**, so a `bool` reads "unknown" as "cannot run" (§9.4). Also
`search_templates`' `match: "all-words"`, which flags a query the server broadened (§9.5).

#### T-103b — slots and parameter writes  — ✅ **LANDED** `3a55c63` ([brief](t-103b-brief.md))
`list_workflow_slots`, `set_workflow_slot`. Four tools would have run ~470 lines, so
validation and notes moved to T-103c. The write path carries the traps:

- ⚠ **`set_workflow_slot` does not write by default** — `stdout` defaults to `true`, which
  *returns* the modified workflow instead of saving it. The wrapper must pass
  `stdout: false` or it will report applied addresses and change nothing (§9.1).
- ⚠ **Use the structured override form** `{"address", "value"}`; the string form
  `"addr=value"` is parsed as JSON and **coerces types**, which would silently retype a
  user's lyric or caption (§9.1) — and CONVENTIONS forbids altering user text silently.
- ✅ A bad address fails the whole call **atomically** — verified by inspecting the file
  afterwards — so a whole parameter set goes in one call with no partial-write recovery.
- Because both failures look like success in the payload, `set_slots` **verifies its own
  write**: no `wrote` path, or an address absent from `applied`, is an error.

#### T-103c — validation and notes  — ✅ **LANDED** ([brief](t-103c-brief.md))
`validate_workflow`, `list_workflow_notes`. Completes the template/slot surface. What it
encodes:

- ⚠ **`valid: true` can mean "checked nothing"** — a UI export too old to auto-convert
  validates vacuously; the tell is `non_node_key` warnings with **no** `converted_from_ui`.
  The type must carry `converted_from_ui` / `converted_node_count` and expose a verdict that
  distinguishes a real pass from a vacuous one, or the app greenlights a workflow nothing
  examined. *(The healthy payload is captured; the vacuous case is documented-not-observed —
  say so in the brief rather than implying it was reproduced.)*
- ⚠ **Validation node ids use `:` where slot addresses use `/`** — the same node is
  `37/43.switch` in slots and `node_id: "37:43"` in validation. Mapping a finding back to
  its control needs a translation nothing in either payload hints at (§9.2). This helper
  belongs here, with the validation types.
- Note text is **untrusted data** — display it, never act on it (§2, §9.6). The MiniMax
  template's own notes carry download URLs and imperative-sounding lines; that is the case
  in the flesh, and it is what the test should use.

**Slot addresses come in two forms** and both must parse: plain `A.name` (ACE-Step) and
subgraph `A/B.name` (MiniMax). `testdata/mcp/list_workflow_slots.minimax.json` is the
live-captured response to serve from the mock — **24 of its 25 addresses are subgraph-form**,
so a parser handling only the flat form fails almost all of a real workflow.

### T-104 — job lifecycle + event pump  — **split in two; shapes captured**
`run_workflow(wait=false)`, `job(action=…)`, `fetch_outputs`. Progress re-emitted as Tauri
events (`job://progress|done|failed`); the UI never polls Rust. Cancellable tokio tasks
owned by managed state (CONVENTIONS).

**The `ComfyBackend` trait is deferred again** (PROJECT.md decisions log 2026-08-24, ARCHITECTURE
§3): Tauri managed state holds `Arc<LocalComfy>` concretely; the trait is shaped when cloud is
verified, not now around one impl.

#### Before T-104a: capture the run/job/fetch success shapes  — ✅ **DONE 2026-08-24**
Captured live via a real short ACE-Step 1.5 turbo generation (10 s duration) — the full
run→poll→cancel→fetch path with an actual MP3 produced. Recorded in **MCP-SURFACE §10**. Key
findings: the terminal-success status is **`"completed"`** (not `"success"`), there is **no
`progress`/`total` numeric** on the status shape (progress = status transitions + `outputs`
filling in), and `run_workflow` **pre-validates** (`[workflow_unknown_nodes]`) before queueing.
The failure shape (`error` non-null + a failure status) was **not** reproduced live — encoded as
inferred in T-104a.

#### T-104a — job wrappers  — ✅ **LANDED** ([brief](t-104a-brief.md))
`run` / `job_status` / `cancel_job` / `outputs` wrappers + `JobRun`/`JobStatus`/`JobCancel`/
`OutputFile`/`OutputBatch` types in `mcp-bridge`, mock-tested (two-stage decode, `is_error`
guards, argument names verbatim). `JobStatus::is_terminal`/`is_success` encode the
`"completed"`-not-`"success"` finding.

#### T-104b — Tauri managed state + event pump  — ✅ **LANDED** ([brief](t-104b-brief.md))
`src-tauri` holds `Arc<LocalComfy>` in managed state (`ComfyState`); a cancellable tokio poll
loop (`poll_until_terminal`) re-emits `job://progress|done|failed`; `connect_comfy`/`run_workflow`/
`cancel_job` commands. Adds `tokio` as a direct src-tauri dep.

#### T-104c — frontend jobs bridge + store + queue panel  — ✅ **LANDED** ([brief](t-104c-brief.md))
The frontend half of the job path, closed out of the T-104b split. `app/src/bridge/jobs.ts` typed
invoke/listen wrappers mirroring the Rust event payloads, a `useJobsStore` queue with a pure
`applyJobEvent` fold, and a `JobQueue` component mounted in AudioStudio. The run trigger stays
empty until the §7 pipeline (T-107) — this is the plumbing, not the pipeline.

### T-105 — models  — **split in two**
`search_models` (query and folder modes return **different shapes** — MCP-SURFACE §11) and
`download_model` + `download(action=…)` progress. Note `download_model` refuses outright when a
remote target is configured. Split for the ~400-line rule.

#### T-105a — model discovery  — ✅ **LANDED** ([brief](t-105a-brief.md))
`search_models` in its three modes (list-folders / folder / query) as three typed wrappers with
three distinct result shapes. The trap is the same tool answering three ways — `files` of
`{name, pathIndex}` vs `rows` of `{name, type, tags}` (MCP-SURFACE §11.1).

#### T-105b — model download  — ✅ **LANDED** ([brief](t-105b-brief.md))
`download_model(wait=false)` → `DownloadSubmit`, and `download(action="status"|"cancel")` →
`DownloadState` (one shape for all actions). `filename` is effectively required when the URL
does not end in the file name (`[missing_argument]`).

### T-106 — node registry  — ✅ **LANDED** `6cf434f` ([brief](t-106-brief.md))
`nodes(action="get")` for live enum choices (`from_node_choices` in profiles) and for LoRA
enumeration. Delivers the `NodeSchema` type + `node_schema(class)` wrapper + a `choices_for`
helper that reads a COMBO input's live `choices` — the primitive the param panel (T-107+) and
the LoRA picker (Phase 3) both build on.

**Before T-106: capture the `nodes(action="get")` shape.** ✅ **DONE 2026-08-24** — recorded in
**MCP-SURFACE §12**. The full node-schema response (metadata + `inputs[]` with `type`/`is_link`/
`section`/`choices`/`options` + `outputs[]`) is captured live, including the two traps: `options`
is polymorphic (`default` is string/bool/number/null) and the `INT` seed's `max` is `u64::MAX`,
which does not fit `i64`.

**Scope note (2026-08-24):** the LoRA list *filtering/grouping* (drop `training_state.pt` and
non-adapters, group by directory, collapse epoch series, dedupe case variants) is **Phase 3**, not
here — ROADMAP Phase 3 and ARCHITECTURE §5a both assign it to the LoRA stack panel, and the rules
are fuzzy enough (telling a real adapter from a misfiled full model by filename alone) to need
owner iteration alongside the picker UI. T-106 delivers the **raw** list; the picker shapes it.

### T-106b — `minimax-music-3` profile  — ✅ **LANDED** `f337083` ([brief](t-106b-brief.md))
**Unblocked 2026-08-23.** Weights are installed and the template validates once
`37/6.unet_name` is overridden to the int8 DiT (MCP-SURFACE §6). Writing it exercises three
things the ACE-Step profile does not: **subgraph slot addresses** (`37/...`), a
**`caption`** input instead of `tags`, and **three seeds plus two duration fields** to fan
out — the shipped template even has the two durations disagreeing (60 vs 120). Also the
first profile whose template already uses `SaveAudioAdvanced`, so it proves the save-node
swap is conditional rather than universal.

**Briefed 2026-08-24** — [tasks/t-106b-brief.md](t-106b-brief.md). Adds one schema field,
`ComfySpec.slot_overrides` (`BTreeMap<SlotAddress, InputValue>`), so a profile can pin a
checkpoint variant the template gets wrong — the generalisation MCP-SURFACE §6 calls for.

### T-107 — profile loader  — ✅ **LANDED** as T-107a + T-107b (split; one brief was ~529 lines)
`library` loads shipped `profiles/` plus a user directory, user wins on id collision.
Validates that a profile's slot addresses exist in its template, and reports which do not.
The reference implementation came to ~529 lines against the ~400 rule, so it is two briefs.

**Where the check lives (2026-08-24).** The address *collector* is pure domain logic and
sits in `create-core` next to `ModelProfile`; the *comparison* is `mcp-bridge`'s landed
`SlotList::missing`, and the two meet at the `src-tauri` seam that already owns both the
fetch and the profile. `library` therefore gains no dependency on `mcp-bridge`, and
nothing here needs a live ComfyUI to test.

#### T-107a — profile loader  — ✅ **LANDED** `883d240` ([brief](t-107a-brief.md))
`library::profiles`: two directories into one id-keyed `ProfileSet`, user wins, every
failure a `ProfileWarning` rather than an error. Never fails, mirroring `config::load`.

#### T-107b — profile slot addresses  — ✅ **LANDED** `0b272ed` ([brief](t-107b-brief.md))
`ModelProfile::slot_addresses()`: inputs (groups walked), `slot_overrides` keys, and
`lyrics_contract.languages_from`, de-duplicated and sorted. Composes with
`SlotList::missing`; adds no comparison function of its own.

### T-108 — `llm-bridge`: `openai_compat` + streaming  — ✅ **LANDED** as T-108a/b/c (one commit, `fc6dc1b`)
The universal baseline (Ollama, LM Studio, llama.cpp, vLLM, OpenRouter). SSE parsing with
canned fixtures, no live endpoint in tests. Keys read from the keychain in Rust — never
sent to the frontend (T-004's boundary).

**Surface verified live first** against Ollama on 127.0.0.1:11434, recorded in
[docs/LLM-SURFACE.md](../docs/LLM-SURFACE.md) with the raw capture in `testdata/llm/`. The
reference implementation was written, compiled and **run against the live endpoint** before
briefing; at 1093 lines it is three briefs.

**The headline finding: streamed text is not one kind of text.** Prompted "Reply with
exactly: tulip", `gemma4:12b-it-qat` — the model this app recommends for lyrics — sent
**163 characters of `delta.reasoning` and 5 of `delta.content`**. Merging them writes the
model's thinking into the user's song; dropping reasoning shows a frozen UI for 40 frames.
Two spellings exist (`reasoning`, `reasoning_content`) and both must be read.

#### T-108a — errors and SSE framing  — ✅ **LANDED** `fc6dc1b` ([brief](t-108a-brief.md))
`error.rs` + `sse.rs`. Byte-buffered decoder (multi-byte characters split across reads),
comment heartbeats, CRLF framing; error bodies that are not JSON. No async, no HTTP.

#### T-108b — the wire format and the reasoning split  — ✅ **LANDED** `fc6dc1b` ([brief](t-108b-brief.md))
`wire.rs`. `ChatDelta` as an enum so only `Content` can reach the user's document. Replays
the committed live capture through the decoder as one test.

#### T-108c — `OpenAiCompat`, the streaming client  — ✅ **LANDED** `fc6dc1b` ([brief](t-108c-brief.md))
`openai.rs` + dependencies. Hand-written `Debug` that redacts the API key. ~435 lines,
knowingly a little over the guide: splitting one stream state machine would cost more than
it saves. Carries an `#[ignore]` live test for the T-113 checklist.

### T-109 — `llm-bridge`: `ollama_native`  — ✅ **LANDED** as T-109a/b (one commit, `a15e377`)
Nicer model listing and pull status. `list_models` feeds the wizard's recommendation chips.

**Surface verified live** against Ollama 0.32.15 (LLM-SURFACE 8-9), including a real 46 MB
pull to capture the progress frames. Reference implementation written, compiled, gate-run
and run against the live server before briefing; 745 lines — two briefs.

**⚠ T-109 answered the `LlmProvider` trait question, and the answer was no.** The second
implementation is not an implementation of the same thing: **`ollama_native` does not
chat**. Ollama's `/v1/chat/completions` already goes through `openai_compat`, so this is an
*enrichment layer* over an endpoint that happens to be Ollama, not a peer provider. Forcing
it into the trait would mean a `stream_chat` that returns an error. The trait stays
deferred — `anthropic` is what will settle it, because it genuinely chats with a different
wire format (ARCHITECTURE 4).

#### T-109a — model listing  — ✅ **LANDED** `a15e377` ([brief](t-109a-brief.md))
`/api/tags`: `capabilities` tells the app which models can chat at all (an embedding model
is indistinguishable on `/v1/models`), which emit reasoning, and which run on someone
else's hardware. Traps: `families: null`, unnormalised `parameter_size`, stub `size` on
cloud entries.

#### T-109b — pull with progress  — ✅ **LANDED** `a15e377` ([brief](t-109b-brief.md))
`/api/pull`: NDJSON framing, and **a failed pull answers HTTP 200** with the error in the
body — comfy-mcp's `Ok(is_error: true)` in a second protocol. `completed` is absent, not
zero, on a layer's first frame.

### T-110 — Setup wizard: ComfyUI step  — ✅ **LANDED** as T-110a/b/c (one commit, `50186c2`)
Detect `comfy-mcp`, install guidance when absent, `launch_comfyui`, health pill, server info.
Degraded states are status pills with retry, never modal walls (CONVENTIONS).

**Surface verified live** against comfy-cli 1.16.0 (MCP-SURFACE 13), with the payload
committed to `testdata/mcp/server_info.json`. Reference implementation written, compiled,
gate-run, and the five rendered states driven through the real store in a browser before
briefing; 922 lines plus CSS — three briefs.

**The `server_info` type written at T-101 was guesswork.** It modelled three blocks as opaque
`Value`s; the live payload has seven, and four drive the wizard — `server.running`,
`hardware.gpu.vram_bytes` (the number `vram_gb_min` is checked against), `workspace.path`, and
`freshness.core.outdated` (the quiet update badge). `freshness` is also polymorphic: an older
comfy-cli answers `{"unsupported": true}`, meaning "could not check", not "up to date".

#### T-110a — typed `server_info` + `launch`  — ✅ **LANDED** `50186c2` ([brief](t-110a-brief.md))
`mcp-bridge/health.rs`. Absent blocks stay absent: no `server` means not running, no GPU means
unknown VRAM rather than zero. `launch` passes no arguments (every flag it accepts exposes an
unauthenticated ComfyUI to the network).

#### T-110b — Tauri commands  — ✅ **LANDED** `50186c2` ([brief](t-110b-brief.md))
`src-tauri/comfy.rs`. `ComfyStatus` is a tagged union with one variant per state, so the UI
never parses an error string. `comfy_status` never returns `Err` for a service problem, and
`[port_in_use]` is treated as "something is already serving", not a failure.

#### T-110c — the view  — ✅ **LANDED** `50186c2` ([brief](t-110c-brief.md))
`bridge/comfy.ts`, `state/comfy.ts`, `Setup.tsx`, `theme.css`. Every degraded state carries a
next step, enforced by a test that sweeps all states. ~440 lines, knowingly a little over the
guide.

### T-111 — Setup wizard: models step  — ✅ **LANDED** as T-111a-e (one commit, `ca610ad`)
Installed models checked against shipped profiles: ready ✅ / install. Curated first.
**Per-model licence terms shown wherever a model is chosen or installed** — some weights are
open-with-conditions (CONVENTIONS).

**Surface verified live** against comfy-cli 1.16.0 (MCP-SURFACE 14). Reference implementation
written, compiled, gate-run, exercised against a real ComfyUI, and every rendered state driven
through the real store in a browser before briefing; ~1659 lines — five briefs.

**Two items from the original line were disproved and dropped:**

- **"quiet update available" is not answerable for models.** `search_models` returns filenames
  only — no hash, version or timestamp (MCP-SURFACE 11.1, 14.7). Nothing local can tell a
  stale checkpoint from a current one. The badge stays where there *is* data: ComfyUI core, on
  the T-110 step.
- **The advanced `search_models` expander is deferred.** Browsing the full model list is a
  different feature from "can I use this profile", and the query mode returns a third response
  shape with every registry field null. Backlogged, not built.

**The trap this step is built around:** `local_check.runnable` answers "can this template run
here", which is *not* "are the models installed". MiniMax Music 3 has all three files and
reports `runnable: false` over a filename its own `slot_overrides` corrects. Readiness is
decided by comparing a profile's **declared** file list against `search_models(folder=)`,
because no comfy-mcp tool answers "which model files does this workflow need" — `workflow_deps`
maps node packs and `node_dependencies` checks Python requirements.

#### T-111a — profiles declare their files  — ✅ **LANDED** `ca610ad` ([brief](t-111a-brief.md))
`create-core/readiness.rs` + `ComfySpec.models` + both profiles. Four states, because three
different absences must not collapse into "not installed": no inventory (ComfyUI stopped), no
declared list, and genuinely missing files.

#### T-111b — the models command  — ✅ **LANDED** `ca610ad` ([brief](t-111b-brief.md))
`src-tauri/models.rs`. Never returns `Err` for a service problem. Lists only the folders the
profiles name. Adds `ProfilesDir` — the shipped profiles had no runtime home until now.

#### T-111c — installing  — ✅ **LANDED** `ca610ad` ([brief](t-111c-brief.md))
`src-tauri/install.rs`. Per-file submit and per-file reporting; `relative_path` must start with
`models`. The only thing in the app that starts a multi-gigabyte transfer, and only ever from a
button.

#### T-111d — bridge and store  — ✅ **LANDED** `ca610ad` ([brief](t-111d-brief.md))
`bridge/models.ts`, `state/models.ts` + tests. Progress is byte-weighted, not file-counted.
~505 lines, over the guide, but around 210 of it is tests and splitting a store from its tests
lands an untested half.

#### T-111e — the view  — ✅ **LANDED** `ca610ad` ([brief](t-111e-brief.md))
`Setup.tsx`, `theme.css`. Licence on every row. Install offered only when every missing file
carries a URL.

### T-112 — Setup wizard: LLM step  — ✅ **LANDED** as T-112a-d (one commit, `e4c3e98`)
Provider, base URL, key to keychain, `list_models`, test call. Mark Gemma 4 12B / 26B / 31B
with a "recommended for lyrics" chip and preselect the 12B, reading the list as data.
**Never auto-pull an LLM.** One keychain read per probe, never a bare `has_secret` from the
frontend — answering it means reading the secret, and on macOS that can raise a prompt (T-004).

**Surface verified live** against Ollama 0.32.15 with 13 models installed (LLM-SURFACE 11).
Reference implementation written, compiled, gate-run, exercised against the real endpoint by
two ignored tests, and every rendered state driven through the real store in a browser before
briefing; ~1405 lines of code — four briefs.

**This is where T-109's `ollama_native` work pays for itself, and the numbers are stark.**
`/v1/models` returns ids and nothing else. Of the 13 models on the verification machine, **2
cannot chat at all** and **8 run on Ollama's servers**, and the OpenAI-compatible list presents
all 13 identically. Without enrichment the wizard offers two models that fail later at lyric
time, and says nothing when a user picks one that sends their unreleased lyrics to a third
party.

**Two traps that only a live call finds:**

- **A thinking model spends the token budget on reasoning first.** Asking for "ok" with 20
  tokens returned **empty content** and `finish_reason: length` on a healthy endpoint. A test
  call that asserts non-empty content reports a broken setup to a user whose setup is fine, so
  success means a well-formed response, not text.
- **Recommendation matching cannot be equality.** The machine has `gemma4:12b-32k` and
  `gemma4:12b-it-qat`; neither is named `gemma4:12b`. Matching is by prefix, and because two
  can match, the preselect is deterministic.

#### T-112a — suggestions as data  — ✅ **LANDED** `e4c3e98` ([brief](t-112a-brief.md))
`data/lyric-llms.json` + `create-core/suggestions.rs` + `library/suggestions.rs`. A configured
model always wins over a suggestion; that is the difference between a suggestion and a setting.

#### T-112b — the LLM commands  — ✅ **LANDED** `e4c3e98` ([brief](t-112b-brief.md))
`src-tauri/llm.rs`. Capabilities are `Option<bool>`: unknown is neither false nor unusable.
One keychain read; the key value never crosses the boundary. ~574 lines, over the guide, but
roughly 250 of it is tests and the file is one coherent surface.

#### T-112c — bridge and store  — ✅ **LANDED** `e4c3e98` ([brief](t-112c-brief.md))
`bridge/llm.ts`, `state/llm.ts` + tests. Never implies privacy for a model it could not check.

#### T-112d — the view  — ✅ **LANDED** `e4c3e98` ([brief](t-112d-brief.md))
`Setup.tsx`, `theme.css`. The remote disclosure sits on the row, not in a footnote.

### T-113 — Phase 1 milestone (producer)  — ✅ **DONE 2026-08-25**, tagged `phase1-done`
Live check on a real install: wizard from cold → ACE-Step present or installed via the app →
server info visible → LLM test call returns. Run `cargo test -p llm-bridge -- --ignored` against the real endpoint too (T-108c). Re-fetch the ACE-Step template and confirm the
`ace-step-1.5-turbo` profile's slot addresses still resolve (the gallery is cached with a
24 h TTL and can drift). Tag `phase1-done`.

**Result.** Producer: wizard from cold, ComfyUI started from the app's own button, server info
visible, models step Ready, LLM test call returns, both `--ignored` suites green. Architect,
same day: a freshly fetched `audio_ace_step1_5_xl_turbo` reports `local_check: runnable: true`
with **zero errors** now the four model files are installed (it had four errors before), and
**all 17 slot addresses** the `ace-step-1.5-turbo` profile declares resolve against it. No
gallery drift.

**One seam not exercised through the UI:** `models_install` / `models_progress` as Tauri
commands. The 18.5 GiB install ran the same `download_model` calls they make, but the wizard
was clicked through afterwards, when the models were already present and the Install button was
therefore never offered. Verified by construction and unit tests, not by a click — worth doing
once in Phase 2 on a machine missing a model.

---

## Standing rules for this phase
- Executor briefs carry **full reference code** and, per test, the **invariant it protects**
  — not the mechanism (WORKFLOW §1, and the T-003/T-004b lessons).
- Aider runs with `--no-auto-commits`; the producer runs `npm run gate`; the architect
  reviews; then commit.
- Nothing in CI may require a live ComfyUI or LLM.
- Third-party surfaces are verified by compiling and running, not recalled.
