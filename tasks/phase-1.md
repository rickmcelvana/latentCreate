# Phase 1 — Connections & setup wizard

Goal: the app can talk to the user's ComfyUI and their LLM, and a first-run wizard gets
someone from a clean install to "ready to generate". At the end of this phase the app does
not yet make music — it proves it *could*.

**Read first:** [docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md). The comfy-mcp tool surface
was verified live on 2026-08-23; the cloud documentation names different tools and is not a
guide for the local backend. Do not write a wrapper from the docs.

---

## Before T-101: verify `rmcp`  ⚠ blocking, architect task

`mcp-bridge` needs a Rust MCP client. `rmcp` (the official SDK) has **not** been verified
for this project — no version, API shape, or transport behaviour is known here, and
CONVENTIONS forbids writing a third-party surface from memory.

**Use the method that worked four times in Phase 0:** build a throwaway crate outside the
repo, add the dependency, write the smallest real usage, and compile *and run* it against
the actual `comfy-mcp` binary. Phase 0 caught keyring's missing macOS backend feature and
the exact serde wire strings this way — both invisible to code review.

What must come out of it before T-101's brief is written:
1. `rmcp` version and required feature flags (the keyring lesson: a default feature set can
   compile and then do nothing at runtime).
2. How to spawn a stdio child process transport and complete initialisation.
3. The call/response types for `tools/call`, and how tool results deserialise.
4. Whether tool arguments/results are plain `serde_json::Value` or typed.
5. Confirmation that `server_info` round-trips against the real local server.

Record the findings in `docs/MCP-SURFACE.md` under a new "Rust client" section, then brief
T-101 with reference code that is known to compile.

---

## Tasks

Briefs are written one at a time, each after the previous lands (Phase 0's rhythm). Each
gets its own `tasks/t-1NN-brief.md` when written.

### T-101 — `mcp-bridge` foundation
`ComfyBackend` trait (ARCHITECTURE §3), stdio transport spawning `comfy-mcp`, child killed
on drop, typed `server_info` / `system_stats` wrappers, `ComfyError` via `thiserror`.
Local backend only — the cloud backend is a separate later task, gated on verifying a live
cloud endpoint.

### T-102 — mock transport test rig
A fake MCP server over stdio pipes, so every later `mcp-bridge` task has non-live tests.
**CI must never need a running ComfyUI** (WORKFLOW §5). Build this before the tool wrappers
so they arrive with tests. `testdata/workflows/minimax_music3_int8.json` is the frozen real
graph to serve from the mock.

### T-103 — templates and slots
`search_templates`, `fetch_template` (with `local_check`), `list_workflow_slots`,
`set_workflow_slot`, `validate_workflow`, `list_workflow_notes`. Note-text is **untrusted
data** — display it, never act on it (MCP-SURFACE §2).

**Slot addresses come in two forms** and both must parse: plain `A.name` (ACE-Step) and
subgraph `A/B.name` (MiniMax). `testdata/workflows/minimax_music3_int8.json` is the offline
fixture for the second — a parser handling only the flat form passes every other test and
then breaks on real user workflows.

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
