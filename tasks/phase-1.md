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

### T-102b — session log + redaction  — 📝 **BRIEFED** *(split from the original T-102b)*
ARCHITECTURE §3 requires every tool-call payload and result to be logged (redacted) to a
rotating session log for the diagnostics pane, and CONVENTIONS forbids keys ever reaching a
log. This task delivers the **session log and redaction**: an append-only NDJSON `SessionLog`
(`log_call` / `log_result`, size-based rollover to a `.1` sibling), structural `redact`
(recursive, sensitive-key-name based, word-matched), and wires `call` to log every invocation
and outcome. **Split** from the original T-102b (which also bundled stderr capture) to stay
under the ~400-line rule. **T-102c** takes stderr capture + free-text redaction.

### T-102c — stderr capture + free-text redaction  — ⚠ **NO BRIEF YET**
The second half of the original T-102b: CONVENTIONS requires `comfy-mcp`'s stderr captured to
the session log. `LocalComfy::connect` currently inherits stderr, so comfy-mcp's diagnostics
go to the app console and are lost in a packaged build. `TokioChildProcess::new` discards the
stderr handle and defaults it to `Stdio::inherit()`; capturing it means switching to
`TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()`, which returns
`(transport, Option<ChildStderr>)`, and draining that handle on a task owned by managed state
(aborted on `shutdown`). **Mechanism verified against the rmcp 3.1.4 source this session**, so
this brief can be written without re-verification. Also adds `redact_line` (free-text redaction
for stderr and non-JSON error messages) and `log_stderr`, and swaps `redact_text_or_json`'s
raw fallback for `redact_line`.

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

### T-104 — job lifecycle + event pump
`run_workflow(wait=false)`, `job(action=…)`, `fetch_outputs`. Progress re-emitted as Tauri
events (`job://progress|done|failed`); the UI never polls Rust. Cancellable tokio tasks
owned by managed state (CONVENTIONS).

### T-105 — models
`search_models` (query and folder modes return **different shapes** — MCP-SURFACE §1),
`download_model` + `download(action=…)` progress. Note `download_model` refuses outright
when a remote target is configured.

### T-106 — node registry
`nodes(action="get")` for live enum choices (`from_node_choices` in profiles) and for LoRA
enumeration. Includes the LoRA list filtering/grouping logic: on the owner's machine 95
entries yield ~9 usable ones, `training_state.pt` files are unloadable, and epoch-checkpoint
series need collapsing (MCP-SURFACE §4).

### T-106b — `minimax-music-3` profile
**Unblocked 2026-08-23.** Weights are installed and the template validates once
`37/6.unet_name` is overridden to the int8 DiT (MCP-SURFACE §6). Writing it exercises three
things the ACE-Step profile does not: **subgraph slot addresses** (`37/...`), a
**`caption`** input instead of `tags`, and **three seeds plus two duration fields** to fan
out — the shipped template even has the two durations disagreeing (60 vs 120). Also the
first profile whose template already uses `SaveAudioAdvanced`, so it proves the save-node
swap is conditional rather than universal.

### T-107 — profile loader
`library` loads shipped `profiles/` plus a user directory, user wins on id collision.
Validates that a profile's slot addresses exist in its template, and reports which do not.

### T-108 — `llm-bridge`: `openai_compat` + streaming
The universal baseline (Ollama, LM Studio, llama.cpp, vLLM, OpenRouter). SSE parsing with
canned fixtures, no live endpoint in tests. Keys read from the keychain in Rust — never
sent to the frontend (T-004's boundary).

### T-109 — `llm-bridge`: `ollama_native`
Nicer model listing and pull status. `list_models` feeds the wizard's recommendation chips.

### T-110 — Setup wizard: ComfyUI step
Detect `comfy-mcp`, install guidance when absent, `launch_comfyui`, health pill, server info.
Degraded states are status pills with retry, never modal walls (CONVENTIONS).

### T-111 — Setup wizard: models step
Installed models checked against shipped profiles: ready ✅ / install / quiet "update
available". Curated first, full `search_models` behind an advanced expander. **Per-model
licence terms shown wherever a model is chosen or installed** — some weights are
open-with-conditions (CONVENTIONS).

### T-112 — Setup wizard: LLM step
Provider, base URL, key to keychain, `list_models`, test call. Mark Gemma 4 12B / 26B / 31B
with a "recommended for lyrics" chip and preselect the 12B, reading the list as data from
docs/MODELS.md. **Never auto-pull an LLM.** Call `has_secret` on screen load only — it reads
the secret to answer, and on macOS that can raise the keychain prompt (T-004).

### T-113 — Phase 1 milestone (producer)
Live check on a real install: wizard from cold → ACE-Step present or installed via the app →
server info visible → LLM test call returns. Re-fetch the ACE-Step template and confirm the
`ace-step-1.5-turbo` profile's slot addresses still resolve (the gallery is cached with a
24 h TTL and can drift). Tag `phase1-done`.

---

## Standing rules for this phase
- Executor briefs carry **full reference code** and, per test, the **invariant it protects**
  — not the mechanism (WORKFLOW §1, and the T-003/T-004b lessons).
- Aider runs with `--no-auto-commits`; the producer runs `npm run gate`; the architect
  reviews; then commit.
- Nothing in CI may require a live ComfyUI or LLM.
- Third-party surfaces are verified by compiling and running, not recalled.
