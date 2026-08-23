# MODELS.md — model profile registry (planning seed)

This file seeds the JSON profiles that will live in `profiles/` (schema: ARCHITECTURE.md §5). Each row becomes one profile file in Phase 1, authored **only** against a template that reports `runnable: true` on a real install.

**Status 2026-08-23:** ACE-Step 1.5 XL Turbo and MiniMax Music 3 were checked against a live local comfy-mcp — see **[MCP-SURFACE.md](MCP-SURFACE.md)** for the verified slot lists, LoRA reality, and which assumptions in this file turned out wrong. Rows below that are not marked verified are still intent, not data.

| id | Role | Lyrics | Negative | VRAM | License | Notes |
|---|---|---|---|---|---|---|
| `ace-step-1.5-turbo` ✅**verified** | **Default.** Full songs w/ vocals | structure-tagged, 51 langs, `[inst]` | **no** ⚠ | runs on 16 GB | Apache-2.0 | Template `audio_ace_step1_5_xl_turbo`, `runnable: true`. Split files (not AIO): `acestep_v1.5_xl_turbo_bf16` + `ace_1.5_vae` + `qwen_0.6b_ace15` + `qwen_4b_ace15`. 8 steps, no CFG. Rich musical controls (bpm/key/timesig) — MCP-SURFACE §3. **LoRA ecosystem — see below** |
| `minimax-music-3` ⚠**blocked** | Flagship quality; full songs up to 5 min | lyrics + structured caption | tbd | 8 GB (layer streaming) / 20–24 GB smooth | open weights, **conditional** ⚠ | Template `audio_minimax_music_3` **confirmed to exist** (2026-08-13, native/free, not API). Weights absent on the verification machine → profile blocked until authored where it runs. Qwen3-8B planner + diffusion transformer + vocoder + flow-matching VAE. Needs `minimax_music3_dit_fp16`, `..._text_encoder_pruned_int8_convrot`, `minimax_music3_dav`. License allows commercial use **with attribution**; separate agreement above ~$20M revenue — surface both in UI. ⚠ Do not confuse with MiniMax **H3** (video) |
| `stable-audio-open` | Instrumental / SFX / loops | none | yes | ~6 GB | Stability community | Duration-capped clips; great for beds & samples |
| `musicgen-stereo` | Simple instrumental, low-spec fallback | none | no | ~4 GB | CC-BY-NC weights ⚠ | Non-commercial weights — surface the warning in UI |
| `yue-7b` | Suno-like lyrics-to-song, "advanced" | full-song lyrics | no | 24 GB+ | Apache-2.0 (check weights) | Slow; only show when VRAM check passes |
| `diffrhythm-2` | Fast full songs | timestamped/plain lyrics | tbd | ~8 GB | check | Block flow matching; verify ComfyUI support level |
| `cover-art (sdxl or flux template)` | Album/single art | n/a | yes (sdxl) | 6–12 GB | varies | Reuses whatever image template the user picks in setup |

## Prefill examples (shipped in profiles' `prompt_guide`)

**Tags (ACE-Step):**
- `melancholic indie folk, acoustic guitar, soft male vocal, intimate, slow tempo`
- `synthwave, retro, 80s, dreamy, female vocal, driving beat, 105 bpm`
- `b-box, deep male voice, trap, hip-hop, super fast tempo`

**Negative:** ⚠ **not supported by ACE-Step 1.5** — its text encoder has no negative input (MCP-SURFACE §3). Keep the example (`low quality, noise, distortion, off-key, muffled`) for models that do accept one; the control only renders when the profile says the model supports it.

**Lyrics skeleton** (the shipped ACE-Step template uses **capitalized, numbered** tags — match it):
```
[Verse 1]
...
[Chorus]
...
[Verse 2]
...
[Chorus]
[Bridge]
...
[Outro]
```
Instrumental: lyrics field = `[inst]`.

**Beyond tags and lyrics** — ACE-Step 1.5 exposes real musical parameters the UI should surface rather than bury in prose: **bpm** (10–300), **key/scale** (34 options), **time signature** (2/3/4/6), **language** (51), plus LM-planner sampling controls (`cfg_scale`, `temperature`, `top_p`, `top_k`, `min_p`) behind an advanced disclosure. These also belong in the lyric-LLM's context — a lyric written for 95 BPM in E minor is a better lyric.

## LoRAs (ARCHITECTURE §5a)

The owner's own production workflow is **ACE-Step 1.5 turbo + custom-trained LoRAs**, which makes LoRA support a v1 requirement. **Verified against the live install 2026-08-23 — several earlier assumptions here were wrong; MCP-SURFACE §4 is the authority.** Corrected picture:

- **Enumeration:** `nodes(action="get", name="LoraLoaderModelOnly")` → `lora_name.choices`. Same list as `search_models(folder="loras")`, but the node schema is what the graph will accept.
- **How they attach:** core **`LoraLoaderModelOnly`** (`model` → `lora_name` → `strength_model` → MODEL), *not* a custom ACE-Step node with a `lora_info` output. Loader class stays a per-profile field anyway, since custom packs differ.
- **Strength:** node range is −100…100 (default 1.0, step 0.01); the UI offers ≈0–2 because that is the musically useful band.
- **The list is mostly noise.** 95 entries on the verification machine, ~9 usable: epoch checkpoints from training runs dominate, `training_state.pt` entries are listed but unloadable, one directory holds five adapters, case-variant duplicates appear, and unrelated video LoRAs share the folder. Filtering/grouping is a design task (ARCHITECTURE §5a).
- **Stacking:** multiple loaders chain; profile caps it (`max_stack`, default 4).
- **Training:** out of scope — ComfyUI packs (FL-AceStep-Training, SN_AceStepTrainer) already do it. We consume the output.
- **Reproducibility:** LoRA identity + strength + order in every provenance sidecar. People who train their own LoRAs care about this more than anyone.

The shipped turbo template contains **no** loader node, so applying LoRAs means splicing nodes into a per-job copy of the workflow — slots can set values but cannot add nodes.

## Lyric-writing LLMs (suggestions, not requirements)

The app works with **any** OpenAI-compatible endpoint (ARCHITECTURE §4) — these are the models the setup wizard suggests, based on the owner's hands-on use for lyric writing. They are hints in the UI, never a gate, and the user's own choice always wins.

| Model | Suggest when | Notes |
|---|---|---|
| **Gemma 4 12B** | Default suggestion — ~8–12 GB VRAM class | Owner's pick: outperforms other models of its size for lyrics |
| **Gemma 4 26B / 31B** | User has the VRAM (~24 GB+) to run them | Also perform well; suggest as "if you can run it" |
| Any OpenAI-compatible model | Always available | Ollama, LM Studio, llama.cpp server, OpenRouter, vLLM, hosted APIs |

Wizard behavior: after `list_models` returns the endpoint's catalog, mark any Gemma 4 12B/26B/31B present with a "recommended for lyrics" chip and preselect the 12B if nothing is chosen yet. If none are installed, show the suggestion as help text with the user's own pull command — **never auto-pull an LLM**; that's the user's disk and bandwidth. Because these are suggestions and models move fast, the list lives in this file and the wizard reads it as data, not hardcoded strings.

## Upgrade / discovery UX (ARCHITECTURE §10)
- Each profile pins a *recommended* model file (name+hash when available). Setup compares against the installed-model listing (`search_models(folder=…)`, or a template's own `local_check`): missing → "Install", older/different → quiet "Update available" chip, present → ✅.
- "Browse all models" expander runs live `search_models` for power users; installing something unprofiled offers the generic template path with a reduced param panel.
- New hot models over time: adding a profile JSON (repo PR or user-dir drop-in) is the entire integration story — this is deliberate.
