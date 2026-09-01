# latentCreate

[![CI](https://github.com/rickmcelvana/latentCreate/actions/workflows/ci.yml/badge.svg)](https://github.com/rickmcelvana/latentCreate/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

**Open-source desktop app for AI music creation. Bring your own models.**

latentCreate is a Tauri 2 desktop app that orchestrates the tools people already use to make
AI music — a ComfyUI install (driven through [Comfy MCP](https://docs.comfy.org/agent-tools/mcp))
for audio/image generation, and an LLM of your choice for lyric writing — behind one clean,
professional UI.

**latentCreate ships no AI models and performs no inference.** Everything runs on *your* local
ComfyUI / LLM server, or on APIs you configure with your own keys. Models and weights obtained
through the app carry their own licenses, which you are responsible for reviewing.

## What it does

- **Guided setup** — detect or configure your local ComfyUI + Comfy MCP, pick an LLM provider,
  and install the model files a profile needs straight into ComfyUI, one click per model.
- **Lyrics Studio** — a structured brief (genre, mood, theme, structure, language) is sent to your
  LLM, which writes lyrics in the exact format your music model expects (`[verse]` / `[chorus]`
  tags and so on). Edit, version, approve, send to audio.
- **Audio Studio** — style tags, lyrics, and the commonly-tweaked workflow parameters (duration,
  seed, BPM, key, steps, CFG) surfaced as real controls. Queue jobs, watch live progress, hear the
  result.
- **Your LoRAs, first-class** — stack your custom-trained LoRAs with per-LoRA strength, or import
  your own ComfyUI workflow as a profile. Every track's LoRA stack is saved with it, so results
  stay reproducible.
- **Library & Player** — playback with a spectrum visualizer, full provenance (every prompt, seed,
  and model saved per track), albums, rename/export/delete, and **Send to** the Latent Mixing /
  Mastering apps.
- **Cover art (optional)** — generate single/album art through the same ComfyUI connection.
- **Prompt optimization with consent** — the app can tighten a prompt for the target model, but it
  always shows the diff first; you approve, edit, or revert. Your words are never silently
  rewritten.

Music models are described by **JSON profiles** that ship with the app and can be extended by
dropping in your own — adding a new model is a data change, not a code change. See
[ARCHITECTURE.md](ARCHITECTURE.md) (section 5) and [docs/MODELS.md](docs/MODELS.md).

## Requirements

### To run the app

- **Desktop OS** — Windows 10/11, macOS, or Linux. Desktop only; no mobile/web build.
- **Music generation** — your own ComfyUI with `comfy-mcp` installed, or Comfy Cloud MCP with an
  API key.
- **Lyric writing (optional)** — Ollama, or any OpenAI-compatible endpoint (LM Studio,
  llama.cpp's server, OpenRouter, vLLM, …). Skippable — you can also write lyrics by hand.

### To build from source

- **Rust** stable (MSRV 1.88), via [rustup](https://rustup.rs/)
- **Node.js** 20 or newer (CI uses 22) and npm
- **Platform build dependencies** (Tauri 2 prerequisites):

| Platform | Required |
|---|---|
| Windows | Microsoft C++ Build Tools (Visual Studio Build Tools with the *Desktop development with C++* workload); WebView2 runtime (preinstalled on Windows 10/11) |
| macOS | Xcode Command Line Tools (`xcode-select --install`) |
| Linux | `libwebkit2gtk-4.1-dev` and the packages listed in [Building for production](#building-for-production) |

## Development

```bash
npm install     # installs the root and the app workspace in one step
npm run dev     # desktop app (Tauri + Vite, hot reload) — run from the repo ROOT, not app/
npm run gate    # everything CI runs, in CI's order
```

`npm run gate` chains `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --workspace`, `tsc -b`, oxlint, vitest, and `vite build` — a green gate locally means
a green pipeline. (The frontend half alone is `npm run gate:app`, the Rust half
`npm run gate:rust`.)

## Building for production

A production build is `tauri build`:

```bash
npm install   # once, if you haven't already
npm run build # runs `tauri build`
```

This compiles the frontend (`beforeBuildCommand` runs `tsc -b && vite build` automatically — no
separate frontend build step), then builds the Rust workspace in release mode (LTO on) and bundles
native installers. Output goes to `src-tauri/target/release/` — the `latentCreate` binary — with
installers under `src-tauri/target/release/bundle/`:

- **Windows** — NSIS `.exe` installer
- **macOS** — `.app` bundle and `.dmg`
- **Linux** — `.deb`, `.rpm`, and AppImage

The first build downloads the bundler tooling for your platform automatically. `tauri build`
also embeds the shipped `profiles/*.json` and `LICENSE` into the bundle.

### Linux prerequisites

Install the WebKitGTK and related packages once before building (verified against the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)):

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

> Note: CI compiles and tests the Rust workspace on all three OSes but does not run
> `tauri build` — a green `npm run gate` is not a check of bundling. Run `npm run build` locally
> to verify a distributable installer.

## Configuration & data

Everything the app writes lives under the OS app-data directory for the identifier
`com.latentbeats.create`:

| | Location |
|---|---|
| Windows | `%APPDATA%\com.latentbeats.create\` |
| macOS | `~/Library/Application Support/com.latentbeats.create/` |
| Linux | `~/.config/com.latentbeats.create/` |

Inside it: `config.json` (non-secret config), `session.log`, and `projects/<slug>/` holding each
project's `project.json`, `lyrics/`, and `tracks/` (audio plus a per-track provenance sidecar).
API keys are **never** written to disk — they live in the OS keychain.

## Documentation

- [PROJECT.md](PROJECT.md) — living project state, decisions log, and open questions
- [ARCHITECTURE.md](ARCHITECTURE.md) — system design and interface contracts
- [docs/MODELS.md](docs/MODELS.md) — the model-profile registry
- [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) — the verified Comfy MCP surface
- [tasks/ROADMAP.md](tasks/ROADMAP.md) — build order and phase status

## Status

**Pre-alpha.** Phases 0–3 (setup, lyrics, audio pipeline, workflow import) are complete; Phase 4
(Library & Player) is in progress. See [PROJECT.md](PROJECT.md) for the live state.

## Contributing

Start with [AGENTS.md](AGENTS.md), then [WORKFLOW.md](WORKFLOW.md) for how work is briefed,
executed, and reviewed. Pull requests are welcome; anything non-trivial should first be discussed
as a task in the roadmap.

## License

[Apache-2.0](LICENSE). latentCreate ships no models — see [NOTICE](NOTICE) for how third-party
model licenses apply to what you generate.
