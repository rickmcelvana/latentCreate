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
  `(transport, Option<ChildStderr>)` to drain on an owned task. Tracked as T-102b.

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
