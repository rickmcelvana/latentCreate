# MCP-SURFACE.md — verified against a live local comfy-mcp (2026-08-23)

**This file supersedes docs/RESEARCH.md §1 for anything about tool names.** RESEARCH §1 was written from the *cloud* MCP documentation; the tool surface a **local** `comfy-mcp` exposes is materially different. Everything below was observed against the owner's running install, not read from docs.

Environment observed: comfy-cli 1.16.0, ComfyUI v0.33.2 (core outdated, latest v0.33.3), workspace `C:\Comfy-Installs\comfyUI\ComfyUI`, server up at `127.0.0.1:8188`, RTX 5060 Ti / 16 GB VRAM, 64 GB RAM, no custom node packs installed.

## 1. Actual local tool names (cloud docs are NOT a guide)

| Purpose | **Local (verified)** | Cloud docs said |
|---|---|---|
| Server health | `server_info`, `system_stats`, `which`, `get_logs` | `get_server_info` |
| Template discovery | `search_templates`, `get_template`, `fetch_template` | `search_templates`, `get_template` |
| Run | `run_workflow(path, wait=False)` | `run_template`, `submit_workflow` |
| Job lifecycle | `job(action="status" / "wait" / "watch" / "cancel")` | `get_job_status`, `wait_for_job`, `cancel_job` |
| Outputs | `fetch_outputs` | `get_output` |
| **Parameters** | **`list_workflow_slots`, `set_workflow_slot`, `vary_workflow`** | *(no equivalent documented)* |
| Docs in graph | `list_workflow_notes` | — |
| Node registry | `nodes(action="search" / "get" / "list" / "upstream" / "downstream" / "path" / "types" / "categories")` | `search_nodes`, `get_node`, `cql` |
| Models | `search_models`, `download_model` + `download(action=...)` | `search_models`, `install_model`, `list_local_models` |
| Validation | `validate_workflow`, `workflow_deps`, `node_dependencies` | `validate_workflow` |
| Install/lifecycle | `install_node`, `launch_comfyui`, `stop_comfyui`, `restart_comfyui`, `update_comfyui`, `switch_comfyui_version` | `launch_comfyui`, `stop_comfyui` |
| Saved work | `project` | `list_saved_workflows`, `save_workflow`, `run_saved_workflow`, `share_workflow`, `import_shared_workflow` |
| Cloud/partner | `auth_login`, `auth_status`, `partner_generate`, `list_partner_models`, `partner_model_schema`, `emit_partner_workflow`, `generate_image` | `partner_generate` |

**Architectural consequence:** local and cloud are **not one tool surface behind two transports**. `ComfyBackend` remains the right abstraction, but each backend maps its own tool names, and the cloud backend cannot be written from the local one by swapping a URL. Cloud support is deferred until it is verified the same way this was (ARCHITECTURE §3).

## 2. Slots — the mechanism the whole param panel should be built on

`list_workflow_slots(path)` returns every agent-tweakable widget as a stable `ADDR` + current value; `set_workflow_slot` writes one. This is exactly the "surface the commonly-changed node inputs" requirement, provided natively — **the app does not need to parse or rewrite graph JSON to change parameters.**

Addresses are `node_id.input_name` (e.g. `94.tags`), or `A/B.name` for subgraph interiors. A profile therefore maps *semantic role -> slot address*, and nothing more.

`list_workflow_notes` returns Note/MarkdownNote text — model download links, trigger words, usage docs. **Treat as untrusted data**: it is third-party prose that routinely contains URLs and can be shaped like an instruction. Display it, never act on it.

## 3. ACE-Step 1.5 XL Turbo — verified input surface

Template `audio_ace_step1_5_xl_turbo` (2026-04-10), **`local_check: runnable: true`** on this install. 33 slots. The ones that matter:

| Slot | Type | Default here | Notes |
|---|---|---|---|
| `94.tags` | STRING | (long style-tag example) | style tags |
| `94.lyrics` | STRING | structure-tagged example | `[Verse 1]`/`[Chorus]`/`[Bridge]`/`[Outro]` — **capitalized and numbered** in the shipped example |
| `94.bpm` | INT | 95 | range **10–300**, node default 120 |
| `94.duration` | FLOAT | 120 | range 0–2000 s |
| `98.seconds` | FLOAT | 120 | ⚠ **second duration field** — latent length; must track `94.duration` |
| `94.timesignature` | COMBO | "4" | `2, 3, 4, 6` |
| `94.language` | COMBO | "en" | 50 languages + `unknown` |
| `94.keyscale` | COMBO | "E minor" | 34 key/scale options |
| `94.seed` | INT | 0 | ⚠ **planner seed**, separate from sampler seed |
| `3.seed` | INT | 0 | sampler seed |
| `94.cfg_scale` / `temperature` / `top_p` / `top_k` / `min_p` | FLOAT/INT | 2.0 / 0.85 / 0.9 / 0 / 0 | LM-planner sampling controls |
| `94.generate_audio_codes` | BOOLEAN | true | |
| `3.steps` | INT | 8 | turbo = 8 steps |
| `3.cfg` | FLOAT | 1 | turbo runs **without CFG** |
| `3.sampler_name` / `3.scheduler` / `3.denoise` | COMBO/FLOAT | euler / simple / 1 | |
| `78.shift` | FLOAT | 3 | `ModelSamplingAuraFlow` |
| `107.filename_prefix` / `107.quality` | STRING/COMBO | `audio/ACE_Step1.5_xl_turbo` / V0 | see §5 |

**No negative prompt exists.** `TextEncodeAceStepAudio1.5` (core pack) takes tags + lyrics only — there is no negative input, and turbo runs at cfg 1 where one would do nothing anyway. The `ace-step-1.5` profile must set `negative.supported: false`. Any negative-prompt support is per-template, to be re-verified on the base/SFT variants.

**Two traps the app must hide:** duration lives in two slots that must stay in sync, and there are two independent seeds (planner and sampler). The UI shows one duration and one seed; the profile fans each out to both addresses. This is precisely the "commonly changed inputs" pain the app exists to remove.

**Opportunity:** bpm, key/scale, and time signature are first-class musical controls, not prose. They belong in the UI *and* in the lyric-LLM's context.

## 4. LoRAs — verified, and messier than assumed

Enumeration works: `nodes(action="get", name="LoraLoaderModelOnly")` returns `lora_name` as a COMBO whose `choices` are the installed LoRA paths (identical to `search_models(folder="loras")`, 95 entries here). Either call is a valid source; the node schema is the better one because it is what the graph will actually accept.

- The loader is **core `LoraLoaderModelOnly`** (`model` + `lora_name` + `strength_model`), *not* a custom ACE-Step node. The earlier profile example naming `ACEStepLoraLoader` was wrong. Variants present: `LoraLoader` (model+CLIP), `LoraLoaderBypass*`, `LoraModelLoader`.
- `strength_model`: FLOAT, node range **−100…100**, step 0.01, default 1.0. The UI should still offer a sane musical range (≈0–2) rather than the node's full range.
- **Entries are paths with backslashes inside subdirectories**, e.g. `ACE-Step-v1.5-ambient_dream1-LoRA\adapter_model.safetensors`.
- **The list is dominated by training noise.** Of 95 entries on this install, ~85 are epoch checkpoints from two training runs (`LoRAgoth\checkpoint-epoch-105\adapter\...` across 20 epochs and two case-variant directories). Roughly 9 are real, usable LoRAs.
- **`training_state.pt` files are listed but are not loadable LoRAs** — they must be filtered out or users will pick them and get failures.
- **A single LoRA directory can contain several adapters** (`...5-LoRAs\` holds `male_vocals_`, `instrumental_`, `voc_06_inst_14___`, etc.) — the unit the user picks is a *file*, not a folder.
- Case-duplicate directories (`LoRAgoth\` and `loragoth\`) both appear on a case-insensitive filesystem.
- Video LoRAs for unrelated models sit in the same folder (`minimax_h3_fl2v_turbo_*`) — filtering by base model is not possible from filenames alone.

**Design consequence — a raw 95-entry dropdown is unusable.** The picker must: drop `training_state.pt` and other non-adapter files, group by directory, collapse epoch-checkpoint series to the latest/`final` with the rest behind an expander, dedupe case-variants, and let users pin favorites and give them display names. This is a real UI design task, not a combo box (Phase 3).

**LoRAs require graph surgery.** The shipped turbo template contains no loader node, so applying one means inserting `LoraLoaderModelOnly` between `UNETLoader` (104) and the model's downstream consumer — slots cannot add nodes. This confirms LoRA runs go through workflow editing + `run_workflow`, not slot-setting alone.

## 5. Output format — the shipped template writes lossy MP3

The turbo template ends in **`SaveAudioMP3`** at quality **V0**, and `SaveAudioMP3`, `SaveAudio` (FLAC) and `SaveAudioOpus` are all marked **DEPRECATED** in this install. The current node is **`SaveAudioAdvanced`**.

For an app whose whole purpose is feeding a mixing/mastering chain, generating lossy MP3 is the wrong default. latentCreate should replace the save node with `SaveAudioAdvanced` writing a lossless format. **Owner-confirmed 2026-08-23:** he swaps this node out of every workflow by habit — so the app doing it automatically removes a manual step that experienced users already know to take, and rescues everyone who does not. Treat lossless output as a correctness requirement, not a preference. Caveat found while verifying: `SaveAudioAdvanced.format` is typed `COMFY_DYNAMICCOMBO_V3` with `is_link: true` — a dynamic combo, not a static enum, so setting it is not a plain string write. **Open item for Phase 3:** determine how to set a V3 dynamic combo through `set_workflow_slot`, or whether the save node must be swapped by graph edit.

## 6. MiniMax Music 3 — installed, and runnable with one slot change

`audio_minimax_music_3` is a **native, non-API (free/local) template**, dated **2026-08-13**,
`open_source: true`. Weights were installed on 2026-08-23 (all three files):

| File | Folder |
|---|---|
| `minimax_music3_dit_int8_convrot.safetensors` | `diffusion_models` |
| `minimax_music3_text_encoder_pruned_int8_convrot.safetensors` | `text_encoders` |
| `minimax_music3_dav.safetensors` | `vae` |

**The template still fails `local_check` out of the box**, with exactly one error: it
hardcodes `minimax_music3_dit_fp16.safetensors` in `37/6.unet_name`, and the **int8** DiT
is what is installed. Fixing it is a one-slot change, verified end to end:

```
fetch_template -> set_workflow_slot("37/6.unet_name",
                    "minimax_music3_dit_int8_convrot.safetensors")
              -> validate_workflow  =>  valid: true, 0 errors
```

**This is the profile mechanism earning its keep.** A profile carries the slot override, so
a user running the int8 weights never sees the mismatch. Generalise it: a profile may pin
*which* checkpoint variant it targets, and `local_check` failing on a filename is not the
same as a missing model. (The fp16 DiT can be installed too if a quality comparison is
wanted; nothing in the design depends on which is present.)

Three residual `COMFY_MATCHTYPE_V3` warnings remain (`ComfySwitchNode`'s wildcard type
against `AUDIO`). They predate the fix, do not block validation, and appear to be a
validator limitation around match-any types rather than a real wiring fault. **Not yet
proven by an actual run** — no generation has been executed.

### 6a. MiniMax slot surface — differs from ACE-Step in ways the profile schema must absorb
From `list_workflow_slots` (25 slots). First real example of **subgraph addressing**:
everything lives under instance `37`, so addresses take the `A/B.name` form.

| Slot | Note |
|---|---|
| `37/13.caption` | ⚠ **`caption`, not `tags`** — `MiniMaxMusic3TextEncode` |
| `37/13.lyrics` | lyrics |
| `37/13.max_duration` | **60** in the shipped template |
| `37/15.seconds` | **120** — ⚠ the two duration fields **disagree as shipped** |
| `37/13.seed`, `37/9.seed`, `37/38.seed` | ⚠ **three** independent seeds (text-encode, sampler, `SeedNode`) |
| `37/13.cfg_scale` / `37/9.cfg` | both 1.7 — duplicated |
| `37/9.steps` | 30 |
| `37/13.top_k` | 50 |
| `37/43.switch` | BOOLEAN — **false** ships; flips VAE decode to the tiled path (`37/42`, tile 1536 / overlap 64). Turn on when pushing toward long songs on a VRAM-limited card — relevant on the owner's 16 GB machine at MiniMax's 5-minute ceiling |
| `35.filename_prefix` | **`SaveAudioAdvanced`** |

A frozen copy of this workflow (int8 DiT) lives at `testdata/workflows/minimax_music3_int8.json` — the only subgraph-structured graph in the repo, and the offline fixture for `A/B.name` address parsing. See that directory's README.

Two consequences:

1. **The lossy-MP3 problem is ACE-Step-specific, not universal.** MiniMax's template already
   ends in `SaveAudioAdvanced`. So the pipeline's save-node swap (ARCHITECTURE §7) must be
   **conditional on what the template already does**, not applied blindly — profiles say
   what output they want, and the pipeline only intervenes when the template disagrees.
2. **Fan-out is worse than ACE-Step, and the shipped defaults are inconsistent.** Three
   seeds and two duration fields that ship *disagreeing* (60 vs 120) is precisely the
   confusion the app exists to hide. One duration control and one seed control, fanned out
   by the profile.

## 7. Verification status of earlier assumptions

| Assumption | Status |
|---|---|
| Comfy MCP exposes model search/download, templates, job lifecycle | ✅ confirmed (different names) |
| Local/cloud = one surface, two transports | ❌ **wrong** — different tool sets |
| "Commonly changed node inputs" need per-profile node mapping | ⚠ superseded — slots do it natively |
| LoRA enumeration via node registry | ✅ confirmed via `nodes(action="get")` |
| LoRA loader is a custom ACE-Step node | ❌ wrong — core `LoraLoaderModelOnly` |
| ACE-Step supports negative prompts | ❌ wrong for 1.5 turbo — no negative input |
| MiniMax Music 3 has a native ComfyUI template | ✅ confirmed, and **runnable** after one slot override (§6) |
| The save-node swap is needed for every model | ❌ wrong — MiniMax already uses `SaveAudioAdvanced`; the swap must be conditional (§6a) |
| ACE-Step turbo runs on consumer hardware here | ✅ `runnable: true` on a 16 GB card |
| `rmcp` returns typed/structured tool results | ❌ wrong — JSON-in-text, no `output_schema`, no `structured_content` (§8.4) |
| A failing tool call surfaces as `Err` | ❌ wrong — `Ok` with `is_error: true`, unknown tool names included (§8.3) |

## 8. Rust client (`rmcp`) — verified 2026-08-23

Method: throwaway crate outside the repo, compiled and **run against the live `comfy-mcp`**
(Phase 0's method). Everything below was observed, not read from docs. `rmcp` has no
usable published examples for this shape, and three of these findings are compile-or-runtime
traps that code review would not have caught.

### 8.1 Dependency

```toml
rmcp = { version = "3.1.4", default-features = false, features = ["client", "transport-child-process"] }
tokio = { version = "1.53", features = ["rt-multi-thread", "macros", "process", "io-util"] }
```

- **`default-features = false` is deliberate.** The default set drags in the whole *server*
  half plus `macros`, `schemars`, `uuid` and `base64`; a client-only bridge needs none of it.
  Verified to compile and run with them off — 71 crates in the tree.
- Licences of everything this pulls in are permissive (rmcp Apache-2.0; `process-wrap`,
  `pastey`, `chrono` dual MIT/Apache; `tokio-util`, `nix` MIT). No copyleft, so no
  decisions-log entry needed (PROJECT.md's Apache-2.0 rule).
- `transport-child-process` pulls `process-wrap`; the transport spawns via
  `tokio::process::Command`.
- The optional **`which-command`** feature exists specifically because Windows
  `.cmd`/`.exe` shims are not reliably resolved by `tokio::process::Command`. We do not need
  it — `comfy-mcp` is a real `.exe` on PATH and spawns fine — but it is the fix if a user's
  install is a shim.

### 8.2 Connect and initialise

```rust
use rmcp::{ServiceExt, transport::{ConfigureCommandExt, TokioChildProcess}};

let transport = TokioChildProcess::new(
    tokio::process::Command::new("comfy-mcp").configure(|c| { c.env("PYTHONIOENCODING", "utf-8"); }),
)?;
let client = ().serve(transport).await?;      // `()` is a no-op ClientHandler
```

- `().serve(transport)` performs the full `initialize` handshake and returns
  `RunningService<RoleClient, ()>`. Negotiated protocol version against this server:
  **`2025-11-25`**.
- **Child cleanup is already handled.** `TokioChildProcess` owns a `ChildWithCleanup` whose
  `Drop` kills the child; `client.cancel().await` shuts down cleanly, and
  `graceful_shutdown()` closes the transport and waits up to 3 s before killing. ARCHITECTURE
  §3's "child killed on drop" requirement needs **no extra code** — do not hand-roll it.
- `client.peer_info()` carries the server's `instructions` string. comfy-mcp's is ~14 KB of
  third-party prose. **Same untrusted-data rule as note text (§2): log it, never act on it.**
- Binary missing → `TokioChildProcess::new` returns `std::io::Error` with
  `kind() == NotFound` ("program not found"). That is T-110's detection signal — a typed
  `ComfyError::NotInstalled`, not a generic spawn failure.
- ⚠ **`TokioChildProcess::new` throws the child's stderr away.** It defaults stderr to
  `Stdio::inherit()` and drops the handle its builder returns, so comfy-mcp's diagnostics go
  to the host console and vanish in a packaged build. CONVENTIONS requires that stderr in
  the session log; capturing it means
  `TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()`, which yields
  `(transport, Option<ChildStderr>)` to drain on an owned task. Tracked as T-102c (the
  original T-102b "session log + child stderr" was split 2026-08-24: T-102b is the log +
  redaction, T-102c this stderr capture).

### 8.3 Calling tools — two traps

```rust
use rmcp::model::CallToolRequestParams;

let res = client.call_tool(
    CallToolRequestParams::new("list_workflow_slots")
        .with_arguments(args)          // args: serde_json::Map<String, Value>
).await?;
```

- ⚠ **`CallToolRequestParams` is `#[non_exhaustive]`** — a struct literal is a hard compile
  error (E0639). Build it with `::new(name)` + `.with_arguments(..)`. Note also the name is
  **plural** in 3.x; older snippets say `CallToolRequestParam`.
- ⚠ **A failing tool returns `Ok`, not `Err`.** Every tool-level failure — bad arguments,
  missing file, *and an unknown tool name* — comes back as
  `Ok(CallToolResult { is_error: Some(true), .. })` with the message in the text content.
  A wrapper that only matches `Result::Err` **treats every Comfy failure as success**.
  `Err(ServiceError)` is reserved for transport/protocol faults (`TransportClosed`,
  `Timeout`, `McpError`, `Cancelled`).
  → `ComfyError` must be produced from `is_error` as well as from `Err`.

### 8.4 Results are JSON-in-text, not structured  ⚠

comfy-mcp sets **`structured_content: None`** and returns **no `output_schema` on any of its
39 tools**. The payload is a JSON document serialised into a single text `ContentBlock`:

```rust
let text = res.content.first().and_then(|c| c.as_text()).map(|t| t.text.clone());
let value: serde_json::Value = serde_json::from_str(&text)?;   // second decode
```

So `mcp-bridge`'s typed wrappers are a **two-stage decode**: extract text, then
`serde_json::from_str` into our own structs. There is no schema to derive from and no
structured field to read — answering Phase 1's question 4: **arguments are
`serde_json::Map<String, Value>`, results are plain JSON text we type ourselves.**

Tool errors carry a machine-readable code in brackets, worth parsing into `ComfyError`:

```
Error executing tool list_workflow_slots: comfy workflow slots Z:/nope.json failed
  [workflow_not_found]: Workflow file not found: Z:/nope.json
hint: check the path
```

### 8.5 `list_workflow_slots` — the real wire shape

Verified against `testdata/workflows/minimax_music3_int8.json`:

```json
{ "workflow": "<abs path>", "id": "minimax_music3_int8", "count": 25,
  "slots": [ { "address": "37/6.unet_name", "name": "unet_name", "type": "COMBO",
               "current_value": "minimax_music3_dit_int8_convrot.safetensors",
               "instance_id": "37/6", "node_type": "UNETLoader" } ] }
```

**24 of the 25 slots are subgraph-form (`37/6.unet_name`); exactly one is flat
(`35.filename_prefix`).** T-103's warning is now measured rather than predicted: a parser
handling only `node.input` would mis-handle 96 % of this real workflow. `instance_id` carries
the same two forms, so splitting on the **last** `.` is the rule — node ids contain `/`, never
the reverse.

### 8.6 Timeouts

`call_tool` sends with `PeerRequestOptions::no_options()` — **no timeout at all**; a wedged
server hangs the caller forever. For bounded calls use
`peer.send_cancellable_request(req, PeerRequestOptions::with_timeout(d))`, which also offers
`reset_timeout_on_progress` / `max_total_timeout` — the right shape for T-104's long
generations. Cheap calls can simply be wrapped in `tokio::time::timeout`.

### 8.7 Argument names (confirmed by a deliberate failure)

Passing `path` instead of `workflow_path` returns a pydantic "Field required" error. The
server's own instruction block states the convention and it held: **`workflow_path`** for
input graphs, **`out_path`** / **`out_dir`** for outputs, **`name`** for registry lookups,
**`prompt_id`** for jobs, **`download_id`** for downloads. No tool takes a bare `path`.

## 9. Templates, slots, validation — verified 2026-08-24

Captured live before the T-103 briefs, against the same install. Payload shapes below are
verbatim except where noted.

### 9.1 `set_workflow_slot` — three traps, all silent

1. ⚠ **It does not write by default.** `stdout` defaults to **`true`**, which is
   *non-destructive*: it returns the modified workflow instead of saving it. **The app must
   pass `stdout: false`.** A wrapper built on the defaults appears to succeed, reports the
   addresses it applied, and changes nothing on disk.
2. ⚠ **Use the structured override form.** Each `overrides` entry may be either
   `{"address": "37/13.caption", "value": "..."}` (**type preserved exactly**) or the string
   `"37/13.caption=..."`, which is **parsed as JSON and therefore coerces** — `"x.y=true"`
   sets a boolean, `"x.y=123"` an integer. User lyrics or a caption that happens to read as
   JSON would be silently retyped. The app uses the structured form, always.
3. ✅ **A bad address fails the whole call atomically.** Verified by inspecting the file
   afterwards: sending one valid and one invalid override returns
   `[workflow_slot_invalid]` ("node 99 not found in workflow") and writes **nothing** — the
   previously-applied values were still intact and the valid override in the failed batch
   was absent. So the app may send an entire parameter set in one call and needs no
   partial-application recovery.

Success shape (`stdout: false`):
```json
{ "workflow": "<abs path>", "applied": ["37/13.caption", "37/9.seed", "37/43.switch"],
  "warnings": [], "wrote": "<abs path>" }
```
`applied` is the confirmation that a value landed; treat an address missing from it as a
failure even when `warnings` is empty.

✅ **A no-op write is still reported in `applied`.** Verified by re-sending two addresses at
the values they already held: both came back in `applied`, and `wrote` was present. This is
what makes treating a missing address as a failure safe — the app sends the whole parameter
set whenever the user changes one field, so most addresses in a typical write are no-ops. If
`applied` listed only *changed* values, that strictness would reject ordinary edits.

### 9.2 ⚠ Validation node ids use `:`, slot addresses use `/`

The **same node** is `37/43` in `list_workflow_slots` and `37:43` in `validate_workflow`:

| Tool | Address for the switch node |
|---|---|
| `list_workflow_slots` | `37/43.switch` (`instance_id: "37/43"`) |
| `validate_workflow` | `node_id: "37:43"` |

Mapping a validation finding back to the UI control that owns it therefore needs a
separator translation. Nothing in either payload hints at this.

### 9.3 `validate_workflow` — `valid: true` can mean "checked nothing"

Real response on the MiniMax fixture: `valid: true`, `error_count: 0`, `warning_count: 3`,
plus `partner_nodes`, `spends_credits`, `object_info_source`, and crucially:

```json
{ "converted_from_ui": true, "converted_node_count": 12 }
```

The tool's own documented blind spot: **a UI-export file too old to auto-convert checks
zero nodes and reports `valid: true`** — the tell is `non_node_key` warnings with **no**
`converted_from_ui`. So the `Validation` type must carry `converted_from_ui` and
`converted_node_count`; a type modelling only `{valid, errors, warnings}` cannot distinguish
a real pass from a vacuous one, and the app would greenlight a workflow nothing examined.

Findings quote the workflow itself — third-party content, same untrusted-data rule as notes.

### 9.4 `local_check` — a tri-state, not a boolean

`get_template` / `fetch_template` return, when the comparison ran:
```json
{ "checked": true, "runnable": true, "summary": "...", "error_count": 0, "errors": [] }
```
`{"checked": false}` means the comparison **could not be made** (usually ComfyUI not
running) and carries **no `runnable` key at all** — it is "unknown", not "no". On a drifted
payload there is no `local_check` key whatsoever. Model it as an enum with three arms
(`Checked { runnable, .. }` / `NotChecked` / absent), never `bool` — a
`#[serde(default)] runnable: bool` reads "unknown" as "cannot run" and sends the user to fix
a problem they do not have.

**Both arms now verified live** (the `checked: false` arm on 2026-08-25, with ComfyUI
stopped). It carries two fields beyond what the tool documents, and no `runnable` key:

```json
{ "checked": false, "reason": "check_unavailable",
  "summary": "could not check this template against your ComfyUI install (the live node
    catalog was unreachable — the server may not be running). The template was still
    written. ... Details: comfy validate ... failed [cql_no_graph]: cannot reach
    http://127.0.0.1:8188/object_info ..." }
```

The template **is still written to `out_path`** when the check cannot run, so a caller that
treats "not checked" as "no file" is wrong twice over.

### 9.5 `search_templates` — `match` tells you the query was widened

```json
{ "total": 10, "shown": 3, "offset": 0, "rows": [ { "name": "audio_ace_step1_5_xl_turbo",
  "title": "...", "description": "...", "output_type": "audio", "tags": ["Music", "Text to Music"],
  "category_title": "Audio", "api": false } ], "match": "all-words" }
```
A phrase pass runs first; only if it finds nothing does an all-words pass run, and the reply
then carries **`match: "all-words"`**. A wrapper that drops that field presents a widened
result as an exact one. `api: true` means the row is a paid hosted route — surface it, since
identically-titled free and paid siblings both exist.

### 9.6 `list_workflow_notes`

`{ "workflow", "count", "notes": [ { "id", "type", "title" (nullable), "text", "pos", "size",
"subgraph" (nullable) } ] }`. No notes is `count: 0`, not an error. An API-format export is
rejected with `workflow_not_frontend_format`.

The MiniMax template's own two notes carry model download URLs and lines that read as
instructions ("Please update ComfyUI first"). **This is the untrusted-data case in the
flesh** (§2): render it as quoted prose, never let it drive a fetch, a download, or a run.

## 10. Run / job / fetch — verified 2026-08-24 (error AND success shapes)

Captured live against the running server (comfy-cli 1.16.0, ComfyUI v0.33.3). Success shapes came
from a real short ACE-Step 1.5 turbo generation (10 s duration) — the full run→poll→fetch path,
with an actual MP3 produced.

| Tool | Args (verified) | Error slug (verified) |
|---|---|---|
| `run_workflow` | `workflow_path`, `wait` | `workflow_not_found`; `workflow_unknown_nodes` |
| `job(action="status"/"wait")` | `prompt_id` | `prompt_not_found` |
| `job(action="queue")` | — | *(success)* `{ "host", "port", "where", "scope", "count", "jobs": [] }` |
| `job(action="cancel")` | `prompt_id` | *(success — see §10.5)* |
| `fetch_outputs` | `prompt_id`, `out_dir` | `download_job_not_found` |

### 10.1 ⚠ `run_workflow` pre-validates before submitting

This is the finding that reshapes T-104. A workflow with an unknown checkpoint and no output
node was **rejected outright**:

```
comfy run --workflow <path> failed [workflow_unknown_nodes]: Workflow has 2 validation error(s) against server
hint: node 1: 'definitely-not-a-real-checkpoint.safetensors' is unavailable: the server reports 0 installed options for ckpt_name
node ?: workflow has no output nodes — the server will reject it (prompt_no_outputs)
```

So `run_workflow` does a validation pass (against the live `object_info`, mirroring
`validate_workflow`'s logic) and a "no output nodes" check *before* POSTing to `/prompt`. The
wrapper's error granularity therefore comes from comfy-cli, not from `/prompt` — and a workflow
that fails validation never produces a `prompt_id`. (This makes ARCHITECTURE §7 step 4's
"validate before submit" partly redundant for the run path, though still valuable for graph
edits.)

### 10.2 Error messages (verbatim shapes, for the wrapper's decode)

- `job(action="status")`, unknown id: `… failed [prompt_not_found]: No prompt with id '<id>' on
  127.0.0.1:8188. hint: check 'comfy jobs ls'; very old prompts may have been pruned from /history`
- `fetch_outputs`, unknown id: `… failed [download_job_not_found]: Job <id> not found in state
  files or API. hint: check the prompt_id and ensure the job has completed`
- `run_workflow`, missing file: `… failed [workflow_not_found]: Specified workflow file not
  found: <path>. hint: check the path; pass the API-format JSON exported from ComfyUI`

### 10.3 `run_workflow(wait=false)` — success shape

```json
{
  "workflow": "<abs path>",
  "status": "queued",
  "prompt_id": "196a0dc9-4b7e-437f-a16f-ce3ef61e1849",
  "client_id": "bf502ccb-1cb7-4fe8-8447-c6c529d85559",
  "outputs": [],
  "elapsed_seconds": null,
  "host": "127.0.0.1",
  "port": 8188,
  "state_file": "C:\\Users\\<user>\\AppData\\Local\\comfy-cli\\jobs\\<prompt_id>.json",
  "watcher_spawned": true
}
```

`prompt_id` is the handle everything else keys on. `watcher_spawned: true` — comfy-cli spawns a
background watcher; that is what lets `job(action="status")`/`wait` reflect progress later, and
`state_file` is the on-disk record `fetch_outputs` reads (which is why `fetch_outputs` works for a
job this machine submitted). `status` here is the *queue* state (`"queued"`), not the run state.

### 10.4 `job(action="status"|"wait")` — the job status shape

Running (`action="status"`):
```json
{ "prompt_id": "…", "status": "running", "workflow_size": 11, "outputs": [],
  "outputs_by_node": {}, "outputs_by_item": {}, "text_outputs": {},
  "host": "127.0.0.1", "port": 8188 }
```

Completed (`action="wait"` or a later `status`):
```json
{ "prompt_id": "…", "status": "completed", "workflow_size": null,
  "outputs": [ "http://127.0.0.1:8188/view?filename=ACE_Step1.5_xl_turbo_00001.mp3&subfolder=audio&type=output" ],
  "outputs_by_node": { "107": [ "<same url>" ] },
  "outputs_by_item": {}, "text_outputs": {},
  "error": null, "host": "127.0.0.1", "port": 8188 }
```

⚠ **The terminal status is `"completed"`, not `"success"`.** Observed statuses: `queued` →
`running` → `completed`. A failure is signalled by a non-null `error` field (not observed here —
the failed-run capture needs a workflow that passes validation but fails a node, e.g. a missing
model at execution). There is **no `progress`/`total` numeric field** in this shape on
comfy-cli 1.16.0 — progress is conveyed by status transitions and outputs filling in, matching the
MCP instruction note ("no per-step events: expect `progress: null`"). The T-104b pump therefore
polls on an interval and reads `status` + `outputs`, not a percentage.

`outputs` are full `view?…` URLs; `outputs_by_node` maps node id (107 = the save node) to the same
URLs. `workflow_size` is node count while running, `null` once terminal.

### 10.5 `job(action="cancel")` — success shape

```json
{ "prompt_id": "…", "where": "local", "host": "127.0.0.1", "port": 8188,
  "found": true, "queue_delete_ok": true, "interrupt_ok": true }
```

`found`/`queue_delete_ok`/`interrupt_ok` are all booleans; a cancel of an already-finished job
still returns `found: true` rather than erroring. **Cancel is racy against a fast job** — with the
model already cached, a second run completed before the cancel landed (its `status` read
`"completed"`), so there is no distinct `"cancelled"` status value to rely on; the app treats the
cancel call's `ok` booleans, not a status string, as the confirmation.

### 10.6 `fetch_outputs` — success shape

```json
{ "prompt_id": "…", "out_dir": "C:\\…\\ace_out",
  "files": [ { "url": "http://127.0.0.1:8188/view?filename=ACE_Step1.5_xl_turbo_00001.mp3&subfolder=audio&type=output",
               "path": "C:\\…\\ace_out\\196a0dc9_000.mp3", "size": 293906 } ] }
```

`fetch_outputs` downloads each output into `out_dir` and returns `files: [{url, path, size}]` —
`path` is the local copy (named `<prompt_prefix>_<n>.<ext>`), `size` bytes. This is what the app
feeds the library's import step.

## 11. Models — verified 2026-08-24

Captured live for T-105 (all read-only; the one download was a bogus URL so nothing was written).

### 11.1 ⚠ `search_models` has THREE response shapes, not one

The same tool answers three ways depending on which of `query` / `folder` is set. A wrapper that
modells one shape reads empty out of the other two.

1. **List-folders** (`search_models()`, no args):
```json
{ "mode": "local", "url": "http://127.0.0.1:8188/models", "count": 27,
  "folders": [ { "name": "checkpoints", "subfolders": [] }, … ] }
```
2. **Folder** (`search_models(folder="checkpoints")`):
```json
{ "mode": "local", "url": "http://127.0.0.1:8188/models/checkpoints", "folder": "checkpoints",
  "total": 0, "shown": 0, "files": [ { "name": "…", "pathIndex": 0 } ] }
```
3. **Query** (`search_models(query="acestep")`):
```json
{ "mode": "local", "filters": { "text": "acestep", "type": null, "include_public": null },
  "total": 10, "shown": 10,
  "rows": [ { "name": "acestep_v1.5_xl_turbo_bf16.safetensors", "type": "diffusion_models",
              "tags": ["diffusion_models"], "base_model": null, "trained_words": null,
              "source_url": null, "preview_url": null, "size": null, "is_public": false,
              "id": null }, … ] }
```

Key distinctions: folder mode has `files` of `{name, pathIndex}` (**camelCase** `pathIndex`);
query mode has `rows` of `{name, type, tags, …}`. The query rows' registry fields (`base_model`,
`trained_words`, `source_url`, `preview_url`, `size`, `id`) are **always null** on the local
surface and `is_public` always false — they are cloud-registry metadata this install never
populates, so the wrapper drops them.

### 11.2 `download_model(wait=false)` — submit shape

```json
{ "download_id": "bb982d2f2b6e", "pid": 26296,
  "dest": "C:\\Comfy-Installs\\comfyUI\\ComfyUI\\models\\checkpoints\\test-model.safetensors",
  "total_bytes": null, "status": "starting" }
```

`download_id` is the handle `download` polls with. ⚠ **`filename` is effectively required** when
the URL does not end in the file name — comfy-cli rejects with `[missing_argument]` "Could not
determine a filename to save the model as". `relative_path` must start with `models` (e.g.
`models/checkpoints`).

### 11.3 `download(action="status"|"wait"|"cancel")` — one shape for all three

```json
{ "id": "bb982d2f2b6e", "status": "downloading", "completed_bytes": 0, "total_bytes": null,
  "percent": null, "elapsed_seconds": 7.5,
  "dest": "C:\\…\\test-model.safetensors", "error": null }
```

Terminal failure (`wait`):
```json
{ "id": "bb982d2f2b6e", "status": "failed", "completed_bytes": 0, "total_bytes": null,
  "percent": null, "elapsed_seconds": 8.0, "dest": "C:\\…\\test-model.safetensors",
  "error": "Download failed after 3 attempts: a network error occurred …" }
```

Status values observed: `starting` → `downloading` → `failed`. `"completed"` is **inferred** (needs
a real download — not reproduced; the bogus URL failed, and comfy-cli cleaned up its own partial
file, leaving `checkpoints` empty). `percent` and `total_bytes` are `null` until the server sends a
content length. `cancel` returns the same shape as `status` (the current state), not a distinct
confirmation — the same racy-cancel caveat as jobs §10.5.

## 12. Node registry (`nodes`) — verified 2026-08-24

Captured live for T-106. `nodes(action="get", name=<class>)` returns the full node schema from
the live `object_info` — the same source `validate_workflow` and `run_workflow`'s pre-validation
read. This is the authoritative list of what a graph will accept, which is why LoRA enumeration
reads it rather than `search_models(folder="loras")` (§4).

### 12.1 `nodes(action="get")` — the node schema shape

`nodes(action="get", name="LoraLoaderModelOnly")`:

```json
{
  "id": "LoraLoaderModelOnly",
  "name": "LoraLoaderModelOnly",
  "display_name": "Load LoRA",
  "description": "This LoRAs loader is used to modify the diffusion model…",
  "category": "model/loaders",
  "output_types": ["MODEL"],
  "output_node": false,
  "is_api_node": false,
  "deprecated": false,
  "pack": "core",
  "labels": [],
  "cloud_disabled": false,
  "inputs": [
    { "name": "model", "type": "MODEL", "required": true, "is_link": true,
      "section": "required", "choices": [],
      "options": { "min": null, "max": null, "step": null, "default": null } },
    { "name": "lora_name", "type": "COMBO", "required": true, "is_link": false,
      "section": "required", "choices": [ "ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors", … ],
      "options": { "min": null, "max": null, "step": null, "default": null } },
    { "name": "strength_model", "type": "FLOAT", "required": true, "is_link": false,
      "section": "required", "choices": [],
      "options": { "min": -100.0, "max": 100.0, "step": 0.01, "default": 1.0 } }
  ],
  "outputs": [ { "name": "MODEL", "type": "MODEL" } ]
}
```

Key facts the wrapper must encode:

- **`type` is the ComfyUI input type** (`MODEL`, `COMBO`, `FLOAT`, `INT`, `STRING`, `BOOLEAN`,
  `CONDITIONING`, `CLIP`, `LATENT`, …). `is_link: true` marks a linkable input (a graph edge);
  `is_link: false` marks a widget (a value the user sets). `section` is `"required"` or
  `"optional"`.
- **`choices` is non-empty only for `COMBO` inputs** — it is the live enum, and the source for
  `from_node_choices` (keyscale/language/timesignature) and for LoRA enumeration (`lora_name`).
- ⚠ **`options` is polymorphic and can exceed `i64`.** `default` is a string (`"en"`), a bool
  (`true`), a number (`0`, `120.0`), or `null`; `min`/`max` are numbers or `null`. The `INT`
  seed's `max` is `18446744073709551615` = `u64::MAX`, which does not fit in `i64` — so the
  wrapper models `options` fields as `Option<Value>`, not `f64`/`i64` (the same precision rule
  that made `Seed` its own profile type).
- `description` may be empty (`""`); `labels` is `[]` on core nodes.
- **Unknown class** fails with `[node_not_found]` ("Node class '<name>' not found in the loaded
  environment") — the usual `Ok(is_error: true)` shape, so the wrapper surfaces it as
  `ComfyError::Tool` with that code, not a decode error.

### 12.2 LoRA enumeration — the raw list, and why filtering is Phase 3

`nodes(action="get", name="LoraLoaderModelOnly")` → `inputs[].lora_name.choices` is the raw
installed-LoRA list (53 entries on this install; §4's 95 was an earlier snapshot — this box's
model set churns). The list is dominated by training noise: `loragoth\checkpoint-epoch-{15..300}\
adapter\adapter_model.safetensors` (20 epochs) plus a `training_state.pt` per epoch, a `final\`
directory, five real ACE-Step LoRA directories (9 adapter files), and two misfiled full video
models (`minimax_h3_fl2v_turbo_*_bf16.safetensors`).

**T-106 delivers the raw list** (`node_schema` + `choices_for`). The filtering/grouping — drop
`training_state.pt` and non-adapter files, group by directory, collapse epoch series to `final`,
dedupe case variants — is a UI design task that §4 assigns to **Phase 3** (the LoRA stack panel),
not here. The rules are fuzzy enough (how to tell a real adapter from a misfiled full model by
filename alone) that they need owner iteration alongside the picker UI.

## 13. `server_info` and `launch_comfyui` -- verified 2026-08-24

Captured live from **comfy-cli 1.16.0**. Fixture:
[testdata/mcp/server_info.json](../testdata/mcp/server_info.json) (home-directory path
replaced with `USER`; everything else verbatim). This is the whole basis of the wizard's
ComfyUI step (T-110).

### 13.1 `server_info` -- seven blocks, not three

The type written at T-101 modelled `server`/`hardware`/`workspace` as opaque `Value`s. The
live payload carries **seven** top-level blocks, and four of them matter:

| Block | What the wizard uses |
|---|---|
| `server` | `{"running": true, "url": "http://127.0.0.1:8188"}` -- the health pill's core fact |
| `hardware` | `gpu.vram_bytes` (17102733312 on this box) plus `ram_bytes`, `cpu`, `os`. This is the number a profile's `vram_gb_min` is checked against |
| `workspace` | `path` -- which ComfyUI install would start, for users with several |
| `compatibility` | `comfy_cli_version`, plus advisory `warnings` (a hard incompatibility raises before returning, so anything here is informational) |
| `freshness` | `core.outdated` -- the quiet "update available" badge. This box reported v0.33.3 against latest v0.33.4 |

Also present: `python` (version/executable) and `config` (comfy-cli's own ini path and
default workspace). Neither is needed yet.

**Trap: `freshness` is polymorphic.** An older comfy-cli answers `{"unsupported": true}`
with **no `core` block at all**. That means "could not check", not "up to date" -- rendering
it as an update badge gives the user a notice they can never clear, and failing to decode it
breaks the whole health pill. Both `unsupported` and `core` are therefore optional on one
struct.

**Trap: absent is not zero.** `hardware` is absent on comfy-cli builds that do not report
one, so `vram_bytes` is `Option`. A "0 GB VRAM" warning on a working machine reads as a
broken app, so unknown must stay unknown all the way to the UI.

**A missing `server` block means not running.** comfy-mcp answers happily while ComfyUI
itself is down -- that is precisely the degraded state the wizard exists to show, and it must
not be read as "unknown, probably fine".

### 13.2 `launch_comfyui` -- and the failure that is really a success

No arguments are passed. The tool accepts `extra_args`, but every network-exposing flag
(`--listen`, `--enable-cors-header`) publishes an **unauthenticated** ComfyUI API to anything
that can reach the machine, and comfy-mcp raises an elicitation for them. This app does not
offer them.

Success is a synthesised envelope, because `comfy launch` itself prints plain text. **The
tool's docstring says that envelope is `{"ok": true}`. It is not.** Captured live 2026-08-25:

```json
{ "background": true, "listen": "127.0.0.1", "port": 8188,
  "url": "http://127.0.0.1:8188", "pid": 23404 }
```

There is no `ok` key. A wrapper with `#[serde(default)] ok: bool` decodes this happily and
then reads `false` from every successful launch. **Success is the `Ok` arm; failure arrives
as an error, never as a falsy field.** (Recorded here from the docstring at T-110 and only
caught when a real launch was run at T-111 -- the failure path had been captured live, the
success path had not.)

Launching while something already holds the port fails, verified live:

```
comfy launch --background failed [port_in_use]: The 8188 port is already in use.
A new ComfyUI server cannot be launched.
hint: stop the process on that port or pass a different `--port`
```

That arrives as `ComfyError::Tool` with `code = "port_in_use"` (the existing `[slug]` parser
handles it). **It is not an error to show the user**: it means something is already serving,
so the honest response is to re-read `server_info` and report what is actually there.

**Call `server_info` first and only launch when `server.running` is false.**
## 14. Model readiness -- verified 2026-08-25

How the app decides whether a profile's models are installed. Every claim below was captured
live against comfy-cli 1.16.0, ComfyUI v0.33.3.

### 14.1 WARNING `search_models` needs a RUNNING ComfyUI

The tool docstring says "Freshness: LIVE -- re-read from disk every call". It does not read
the disk. With ComfyUI stopped:

```
comfy models list-folders failed [server_not_running]: failed to fetch
http://127.0.0.1:8188/models: <urlopen error [WinError 10061] No connection could be made
because the target machine actively refused it>
```

The existing `[slug]` parser handles the code. **Consequence for the wizard:** the models step
cannot answer anything until the ComfyUI step is green, so it needs an explicit "cannot check"
state that points back at that step. Reporting an empty install would tell a user with 18.5 GiB
of models on disk to download them again.

### 14.2 Nothing answers "which model files does this workflow need"

The two tools whose names suggest it do not:

| Tool | What it actually answers |
|---|---|
| `workflow_deps` | node classes to node **packs** (ComfyUI-Manager manifest) |
| `node_dependencies` | one pack's **Python** requirements against the venv |

The only signal is `local_check.errors`, and it is **prose**:

```
node 104: 'acestep_v1.5_xl_turbo_bf16.safetensors' not in 2 known options for unet_name
  (this install has: minimax_h3_fl2va_pruned_int8_convrot.safetensors, ...)
```

Deciding whether to start a multi-gigabyte download by parsing English is not acceptable.
**Therefore the profile declares its own files** (`comfy.models`), and readiness is exact
string matching against `search_models(folder=)`.

### 14.3 WARNING `local_check.summary` is wrong for missing models

Both templates below produce the same advice, and for a missing model it is misleading:

> this template needs a node class or an input option your ComfyUI install does not have --
> a template served from the gallery can be newer than your install. Update ComfyUI and its
> custom nodes (`update_comfyui`), or pick another template.

The actual problem in both cases is model files, which `update_comfyui` cannot fix and picking
another template does not address. **Never surface `local_check.summary` to the user.**

### 14.4 `runnable: false` does NOT mean "models missing"

The load-bearing case. With all three MiniMax files installed:

```json
{ "checked": true, "runnable": false, "error_count": 1,
  "errors": ["node 37:6: 'minimax_music3_dit_fp16.safetensors' not in 2 known options for
              unet_name (this install has: ..., minimax_music3_dit_int8_convrot.safetensors)"] }
```

The template pins the fp16 DiT; the int8 is installed; the profile's own `slot_overrides`
already corrects it (section 6). A models step driven off `runnable` tells a user with a
working install to download 2.3 GiB they have. **Readiness is decided from the declared file
list, never from `local_check`.**

### 14.5 The two shipped profiles' files, captured

ACE-Step 1.5 XL Turbo -- `Comfy-Org/ace_step_1.5_ComfyUI_files`, under `split_files/`.
Note it puts **nothing in `checkpoints`**; the empty `checkpoints` folder is a red herring.

| File | Folder | Bytes |
|---|---|---|
| `acestep_v1.5_xl_turbo_bf16.safetensors` | `diffusion_models` | 9,974,719,892 |
| `qwen_0.6b_ace15.safetensors` | `text_encoders` | 1,191,588,248 |
| `qwen_4b_ace15.safetensors` | `text_encoders` | 8,379,154,232 |
| `ace_1.5_vae.safetensors` | `vae` | 337,431,732 |

**Total 18.5 GiB.** The size must be shown before the user commits to it.

MiniMax Music 3 -- `Comfy-Org/MiniMax-Music-3`, 11.1 GiB across `diffusion_models`,
`text_encoders`, `vae` (the int8 variants the profile pins).

### 14.6 WARNING A repackaged repo's licence tag is not the model's licence

`Comfy-Org/MiniMax-Music-3` is tagged **Apache-2.0** on Hugging Face. The upstream
`MiniMaxAI/MiniMax-Music3` carries a bare `LICENSE` file with no SPDX tag -- a custom
community licence with an attribution obligation and a revenue threshold. The repackager's
tag describes the repackaging, not the weights. **Licence text shown to the user comes from
the profile**, which records the upstream terms, never from the download host.

### 14.7 There is no "update available" for model files

`search_models` returns **filenames only** -- no hash, no version, no timestamp (section 11.1).
Nothing in the local surface can tell a stale checkpoint from a current one. The quiet
"update available" badge is answerable for **ComfyUI core** (`freshness`, section 13.1) and
not for models. Do not invent it.

### 14.8 `stop_comfyui` shape

Like `launch_comfyui` (section 13.2), the synthesised envelope carries **no `ok` key**:

```json
{ "stopped": true, "host": "127.0.0.1", "port": 8188, "pid": 23404 }
```
