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

For an app whose whole purpose is feeding a mixing/mastering chain, generating lossy MP3 is the wrong default. latentCreate should replace the save node with `SaveAudioAdvanced` writing a lossless format. Caveat found while verifying: `SaveAudioAdvanced.format` is typed `COMFY_DYNAMICCOMBO_V3` with `is_link: true` — a dynamic combo, not a static enum, so setting it is not a plain string write. **Open item for Phase 3:** determine how to set a V3 dynamic combo through `set_workflow_slot`, or whether the save node must be swapped by graph edit.

## 6. MiniMax Music 3 — template confirmed, weights absent here

`audio_minimax_music_3` exists as a **native, non-API (free/local) template**, dated **2026-08-13**, `open_source: true`, 1387 uses — confirming the owner's recollection.

It is **not runnable on this machine**: `local_check` reports 3 missing model options —
- `minimax_music3_dit_fp16.safetensors` (diffusion_models)
- `minimax_music3_text_encoder_pruned_int8_convrot.safetensors` (text_encoders)
- `minimax_music3_dav.safetensors` (vae)

What *is* installed is MiniMax **H3**, the video model (`minimax_h3_fl2va_*`, `minimax_h3_video_vae_*`, plus an H3 audio VAE and two H3 video LoRAs) — a different model that happens to share the brand. So Music 3 was presumably run on the owner's other PC.

Two caveats before concluding anything: ComfyUI core is **outdated** here (v0.33.2 vs v0.33.3), and `local_check` also emitted `COMFY_MATCHTYPE_V3` type-mismatch warnings, both consistent with a gallery template newer than the install. **Update ComfyUI first, then re-check** before treating the missing pieces as the whole story. Writing the `minimax-music-3` profile needs a machine where it actually runs.

## 7. Verification status of earlier assumptions

| Assumption | Status |
|---|---|
| Comfy MCP exposes model search/download, templates, job lifecycle | ✅ confirmed (different names) |
| Local/cloud = one surface, two transports | ❌ **wrong** — different tool sets |
| "Commonly changed node inputs" need per-profile node mapping | ⚠ superseded — slots do it natively |
| LoRA enumeration via node registry | ✅ confirmed via `nodes(action="get")` |
| LoRA loader is a custom ACE-Step node | ❌ wrong — core `LoraLoaderModelOnly` |
| ACE-Step supports negative prompts | ❌ wrong for 1.5 turbo — no negative input |
| MiniMax Music 3 has a native ComfyUI template | ✅ confirmed |
| ACE-Step turbo runs on consumer hardware here | ✅ `runnable: true` on a 16 GB card |
