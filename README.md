# latentCreate

**Open-source desktop app for AI music creation — the front door to the Latent suite.**

latentCreate is a Tauri desktop app that orchestrates the tools people already use to make AI music — ComfyUI (via [Comfy MCP](https://docs.comfy.org/agent-tools/mcp)) for audio/image generation and an LLM of your choice (Ollama, OpenAI-compatible, Anthropic) for lyric writing — behind one clean, professional UI.

**latentCreate ships no AI models.** Everything runs on *your* local ComfyUI / LLM server, or on APIs you configure with your own keys.

## What it does

- **Guided setup** — detect or configure your local ComfyUI + Comfy MCP, pick an LLM provider, discover and download music models (ACE-Step 1.5, Stable Audio Open, MusicGen, …) straight into ComfyUI.
- **Lyrics Studio** — structured brief (genre, mood, theme, structure, language) → LLM writes lyrics in the exact format your music model expects (`[verse]`/`[chorus]` tags etc.). Edit, version, approve, send to audio.
- **Audio Studio** — style tags, lyrics, negative prompt (when the model supports it), and the commonly-tweaked workflow parameters (duration, seed, steps, CFG) surfaced as real controls. Queue jobs, watch progress, hear results.
- **Library & Player** — playback with a spectrum visualizer, full provenance (every prompt/seed/model saved per track), delete/keep/album lists, and **Send to** [Latent Mixing](https://app.latentmixer.com) / [Latent Mastering](https://app.latentmastering.com).
- **Cover Art (optional)** — generate single/album art through the same ComfyUI connection.
- **Prompt optimization with consent** — the app can tighten your prompts for the target model, but always shows you the diff; you approve, edit, or revert. Your words are never silently rewritten.

## Requirements (user-provided)

| Need | Options |
|---|---|
| Music generation | Local ComfyUI + `comfy-mcp`, **or** Comfy Cloud MCP (API key) |
| Lyric writing (optional) | Ollama, any OpenAI-compatible endpoint, OpenAI, Anthropic — or bring your own lyrics. Suggested local models: **Gemma 4 12B**, or 26B/31B if you have the VRAM ([why](docs/MODELS.md)) |
| Cover art (optional) | Any image model in your ComfyUI |

## Status

**Planning / pre-alpha.** Start with [PROJECT.md](PROJECT.md) (living state), [ARCHITECTURE.md](ARCHITECTURE.md) (design), and [tasks/ROADMAP.md](tasks/ROADMAP.md) (build order). Contributors and coding agents: read [AGENTS.md](AGENTS.md) first.

## License

[Apache-2.0](LICENSE). latentCreate ships no models — see [NOTICE](NOTICE) for how third-party model licenses apply to what you generate.
