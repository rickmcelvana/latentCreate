# MODELS.md — model profile registry (planning seed)

This file seeds the JSON profiles that will live in `profiles/` (schema: ARCHITECTURE.md §5). Each row becomes one profile file in Phase 1 (T-105), with fields verified against the live ComfyUI template / model files at that time — **treat the table as intent, not verified data.**

| id | Role | Lyrics | Negative | VRAM | License | Notes |
|---|---|---|---|---|---|---|
| `ace-step-1.5` | **Default.** Full songs w/ vocals | structure-tagged, 50+ langs, `[inst]` | yes | ~8 GB | Apache-2.0 | AIO checkpoint `ace_step_1.5_turbo_aio.safetensors`; turbo = low step count |
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

**Negative (where supported):** `low quality, noise, distortion, off-key, muffled`

**Lyrics skeleton:**
```
[verse]
...
[chorus]
...
[verse]
...
[chorus]
...
[bridge]
...
[chorus]
[outro]
```
Instrumental: lyrics field = `[inst]`.

## Lyric-writing LLMs (suggestions, not requirements)

The app works with **any** OpenAI-compatible endpoint (ARCHITECTURE §4) — these are the models the setup wizard suggests, based on the owner's hands-on use for lyric writing. They are hints in the UI, never a gate, and the user's own choice always wins.

| Model | Suggest when | Notes |
|---|---|---|
| **Gemma 4 12B** | Default suggestion — ~8–12 GB VRAM class | Owner's pick: outperforms other models of its size for lyrics |
| **Gemma 4 26B / 31B** | User has the VRAM (~24 GB+) to run them | Also perform well; suggest as "if you can run it" |
| Any OpenAI-compatible model | Always available | Ollama, LM Studio, llama.cpp server, OpenRouter, vLLM, hosted APIs |

Wizard behavior: after `list_models` returns the endpoint's catalog, mark any Gemma 4 12B/26B/31B present with a "recommended for lyrics" chip and preselect the 12B if nothing is chosen yet. If none are installed, show the suggestion as help text with the user's own pull command — **never auto-pull an LLM**; that's the user's disk and bandwidth. Because these are suggestions and models move fast, the list lives in this file and the wizard reads it as data, not hardcoded strings.

## Upgrade / discovery UX (ARCHITECTURE §10)
- Each profile pins a *recommended* model file (name+hash when available). Setup compares against `list_local_models`: missing → "Install", older/different → quiet "Update available" chip, present → ✅.
- "Browse all models" expander runs live `search_models` for power users; installing something unprofiled offers the generic template path with a reduced param panel.
- New hot models over time: adding a profile JSON (repo PR or user-dir drop-in) is the entire integration story — this is deliberate.
