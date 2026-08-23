# RESEARCH.md — findings that shaped the plan (2026-08-23)

Snapshot of external facts the architecture depends on. Re-verify anything here before building against it (the MCP surface especially is young and moving).

> ⚠ **Superseded in part.** Section 1 below was written from the *cloud* MCP docs. On 2026-08-23 the **local** comfy-mcp was inspected directly and its tool surface differs materially. For anything about tool names, slots, LoRAs, or the ACE-Step/MiniMax templates, **[MCP-SURFACE.md](MCP-SURFACE.md) is the authority**; this file remains the record of the pre-verification research.

## 1. Comfy MCP (docs.comfy.org/agent-tools/mcp)

Two deployments, one tool surface:
- **Local:** `pip install comfy-mcp` (needs Python 3.10+, `comfy-cli>=1.14.0`, a ComfyUI install). Runs as a stdio MCP server, sees the user's actual models/LoRAs/custom nodes. Free. `COMFY_BIN` env var for non-standard paths. Claude Code registration: `claude mcp add comfy-mcp -- comfy-mcp`.
- **Cloud:** `https://cloud.comfy.org/mcp` — OAuth or API key; discovery tools free, generation needs a subscription. Note: device-code OAuth for headless is not available yet → our app should use **API key** for cloud.

Tools we build on (names as documented):
- Discovery: `search_templates`, `search_models`, `search_nodes`, `get_template`, `get_node`, `cql`
- Generation: `run_template`, `submit_workflow`, `upload_file`
- Jobs: `get_job_status`, `wait_for_job`, `get_output`, `submit_batch`, `cancel_job`
- Workflows: `list_saved_workflows`, `save_workflow`, `run_saved_workflow`, `share_workflow`, `import_shared_workflow`
- Local-only: `launch_comfyui`, `stop_comfyui`, `validate_workflow`
- Account: `get_billing_status`, `get_server_info`

Implications taken into the architecture: model download/upgrade UX rides on `search_models`; "commonly changed node inputs" ride on templates + our profiles rather than raw graph editing; `validate_workflow` (local) lets us sanity-check embedded workflows against the user's node registry before submitting.

**Open verification item (feeds T-101):** exact input/output JSON schemas per tool — enumerate live via MCP `tools/list` against a running `comfy-mcp` before writing the typed Rust wrappers. Do not trust this summary for schemas.

## 2. Music model landscape (early 2026)

- **ACE-Step 1.5** (Jan 2026, Apache-2.0) — the default choice. Hybrid LM-planner + diffusion renderer; full songs with vocals from tags + structure-tagged lyrics; ~10 s generation on consumer GPUs; 50+ languages; LoRA/ControlNet ecosystem. Native ComfyUI support with an all-in-one checkpoint (`ace_step_1.5_turbo_aio.safetensors` → `models/checkpoints/`) or split files (diffusion model + Qwen 0.6b/1.7b text encoder + VAE).
- **MiniMax Music 3** (released 2026-08-13/14, weights on Hugging Face under MiniMaxAI) — the current "hot" open-weights model. Composes/arranges/performs a complete song up to 5 minutes in one generation from lyrics + a structured caption; architecture is a Qwen3-8B LM planner plus diffusion transformer, vocoder, and flow-matching VAE. Runs from ~8 GB VRAM with layer streaming, 20–24 GB for smooth full-precision. Notable for vocals that read as performed rather than synthesized. **License is open-weights-with-conditions, not OSI-open:** commercial use permitted with attribution to MiniMax Music 3, separate agreement required above ~$20M revenue — the app must surface attribution and the threshold, since users will ship these tracks commercially. ComfyUI support level needs verification before the profile is written.
- **YuE 7B** — closest open model to Suno quality for lyrics-to-song; needs 24 GB+ VRAM and patience → profile it, label "advanced".
- **DiffRhythm 2** — fast full-song via block flow matching; worth a profile.
- **Stable Audio Open 1.5** — instrumental/SFX, no lyrics; clean training-data licensing (free commercial < $1M revenue). Good instrumental profile.
- **MusicGen (Stereo)** — older, simple, instrumental; lowest-spec fallback profile.
- Architecture families: LM-only autoregressive over codec tokens (YuE, HeartMuLa, Muse, Khala) vs hybrid planner+diffusion (ACE-Step 1.5, LeVo 2). Our profile schema stays agnostic to this.

## 3. Prompting practices (drives prefills + LLM system prompts)

- **Style/tags field:** short comma-separated tags beat prose — genre, era, mood, instrumentation, vocal descriptor, tempo (e.g. `synthwave, retro, 80s, dreamy, female vocal, 105 bpm`). Scenario+atmosphere combos work well.
- **Lyrics field (ACE-Step-style):** structure tags `[verse]`, `[chorus]`, `[bridge]`, `[outro]`; `[inst]` for instrumental; vocal-technique cues like `a cappella`, `b-box, deep male voice, trap, super fast tempo` belong in tags.
- **Negative prompts:** supported by some ComfyUI audio workflows via the negative conditioning input; capability is per-model → the `inputs.negative.supported` profile flag.
- **Lyric-LLM system prompt** should pin: target model name + its lyric contract, output-only-lyrics rule, structure-tag list, language, approximate line budget derived from target duration. (ARCHITECTURE §6.)

## 3a. LoRAs in ComfyUI (ACE-Step)

- LoRA folders (containing `adapter_config.json`) live in `ComfyUI/models/loras/` or `ComfyUI/models/Ace-Step1.5/loras/`. An ACE-Step LoRA Loader node exposes them as a dropdown and its `lora_info` output connects to the generation node (TextToMusic etc.); strength default 1.0.
- Training happens in ComfyUI via custom node packs (`filliptm/ComfyUI-FL-AceStep-Training`, `Starnodes2024/Comfyui_SN_AceStepTrainer`) — dataset building, tiled VAE encoding for long audio, live loss charts. **Out of scope for latentCreate**; we consume trained LoRAs.
- **Verification item (Phase 1):** how to enumerate installed LoRAs over MCP. Expected path is reading the loader node's combo values from the live registry (`get_node`, or `cql`) since ComfyUI's `/object_info` surfaces filenames as enum values. Confirm against a running `comfy-mcp` before writing the picker.

## 4. Sources

- https://docs.comfy.org/agent-tools/mcp.md
- https://docs.comfy.org/tutorials/audio/ace-step/ace-step-v1-5
- https://blog.comfy.org/p/ace-step-15-is-now-available-in-comfyui
- https://comfyui-wiki.com/en/tutorial/advanced/audio/ace-step/ace-step-v1 (prompting tag examples)
- https://github.com/ace-step/ACE-Step
- https://dev.to/czmilo/ace-step-15-the-complete-2026-guide-to-open-source-ai-music-generation-522e
- https://boppy.me/blog/best-open-source-ai-music-models (2026 landscape/architecture families)
- https://www.it-jim.com/blog/best-open-source-ai-music-generator/
- https://www.spheron.network/blog/deploy-open-source-ai-music-generation-gpu-cloud-2026/
- https://www.minimax.io/blog/minimax-music-3-0-next-generation-open-weights-production-ready-versatile-music-model (MiniMax Music 3, vendor announcement)
- https://www.marktechpost.com/2026/08/17/minimax-releases-minimax-music3/ (MiniMax Music 3, architecture + license summary)
- https://github.com/filliptm/ComfyUI-FL-AceStep-Training (ACE-Step LoRA loader/trainer nodes, lora dir conventions)
- https://github.com/Starnodes2024/Comfyui_SN_AceStepTrainer (ACE-Step LoRA training pipeline)
