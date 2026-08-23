# ARCHITECTURE.md — latentCreate system design

*Interfaces defined here are contracts. Aider briefs reference sections of this file instead of restating them. Changes to this file happen in Claude sessions, never silently inside an executor run.*

## 1. Shape of the system

Desktop-only Tauri 2 app. React 19 + TypeScript + Vite + Zustand frontend, Rust backend. **latentCreate is an orchestrator, not a generator** — it owns no models and no inference. All generation is delegated:

```
┌─────────────────────────────────────────────────────────┐
│ latentCreate (Tauri desktop app)                        │
│                                                         │
│  React UI ── Tauri commands ──► Rust core               │
│                                   │                     │
│                    ┌──────────────┼───────────────┐     │
│                    ▼              ▼               ▼     │
│              mcp-bridge      llm-bridge      library    │
│              (MCP client)    (HTTP client)   (projects, │
│                    │              │           audio,    │
└────────────────────┼──────────────┼───────────sidecars)─┘
                     ▼              ▼
        comfy-mcp (stdio, local)   Ollama / OpenAI-compat /
        or cloud.comfy.org/mcp     OpenAI / Anthropic APIs
                     │
                     ▼
              ComfyUI (user's install or Comfy Cloud)
```

### Why MCP instead of ComfyUI's raw HTTP API
`comfy-mcp` gives us, through one protocol: model **search and download into ComfyUI** (`search_models`), curated **templates** (`search_templates`, `run_template`), workflow validation against the live node registry (`validate_workflow`), job lifecycle (`submit_workflow`, `wait_for_job`, `get_output`), saved workflows, and `launch_comfyui`/`stop_comfyui`. The raw `/prompt` API gives none of the discovery/management surface. MCP also makes local vs Comfy Cloud a transport swap, not two integrations. The raw API remains a documented fallback option (PROJECT.md OQ-3) behind the same trait, but is **not** built unless MCP proves insufficient.

The app is an **MCP client acting programmatically** — it calls tools directly (no LLM in the loop for generation). The lyric LLM is a separate, plain HTTP concern.

## 2. Crate / directory layout

```
latentCreate/
├── app/                      # React frontend (Vite)
│   ├── src/
│   │   ├── state/            # Zustand stores (config.ts, lyrics.ts, audio.ts, library.ts, jobs.ts)
│   │   ├── views/            # Setup, LyricsStudio, AudioStudio, Library, CoverArt
│   │   ├── components/       # shared UI (Player, Visualizer, PromptDiff, ParamControl, …)
│   │   ├── bridge/           # ONLY place that calls Tauri invoke/listen (typed wrappers)
│   │   └── theme.css         # single source of styling truth
├── crates/
│   ├── create-core/          # domain types: Project, Track, LyricDoc, GenerationSpec,
│   │                         #   ModelProfile, Provenance. No I/O. Serde everywhere.
│   ├── mcp-bridge/           # MCP client (rmcp). Spawns/attaches comfy-mcp (stdio) or
│   │                         #   connects to cloud MCP (HTTP). Typed wrappers per tool.
│   ├── llm-bridge/           # LLM providers behind one trait. reqwest + SSE streaming.
│   └── library/              # on-disk store: projects, tracks, sidecars, config
├── src-tauri/                # Tauri shell: commands, events, single place wiring crates
├── profiles/                 # model capability profiles (JSON, shipped with app) — see §5
├── docs/                     # RESEARCH.md, MODELS.md, session logs
└── tasks/                    # ROADMAP.md + phase-N.md briefs
```

## 3. mcp-bridge (Rust)

- MCP client over **stdio** (spawn `comfy-mcp` as child process; `COMFY_BIN` respected) or **streamable HTTP** (`https://cloud.comfy.org/mcp` + API key header). Use the official Rust MCP SDK (`rmcp`); pin version in the brief that introduces it after verifying current API against docs (CONVENTIONS: never write third-party surfaces from memory).
- One trait, two transports:

```rust
pub trait ComfyBackend: Send + Sync {
    async fn health(&self) -> Result<ServerInfo, ComfyError>;
    async fn search_models(&self, q: ModelQuery) -> Result<Vec<ModelHit>, ComfyError>;
    async fn install_model(&self, id: &str) -> Result<InstallHandle, ComfyError>; // download into ComfyUI
    async fn list_local_models(&self) -> Result<Vec<LocalModel>, ComfyError>;
    async fn search_templates(&self, q: &str) -> Result<Vec<TemplateInfo>, ComfyError>;
    async fn run_template(&self, t: &str, inputs: TemplateInputs) -> Result<JobId, ComfyError>;
    async fn submit_workflow(&self, wf: WorkflowJson) -> Result<JobId, ComfyError>;
    async fn job_status(&self, id: &JobId) -> Result<JobStatus, ComfyError>;
    async fn cancel_job(&self, id: &JobId) -> Result<(), ComfyError>;
    async fn get_output(&self, id: &JobId) -> Result<Vec<OutputFile>, ComfyError>;
    async fn launch_comfyui(&self) -> Result<(), ComfyError>;   // local only; cloud = no-op error
}
```

- Long jobs: poll `job_status` (or `wait_for_job` with timeout) on a tokio task; progress re-emitted to the frontend as Tauri events (`job://progress`, `job://done`, `job://failed`). The UI never polls Rust; Rust pushes.
- All tool-call payloads and results are logged (redacted) to a rotating session log for the diagnostics pane.

## 4. llm-bridge (Rust)

```rust
pub trait LlmProvider: Send + Sync {
    async fn list_models(&self) -> Result<Vec<String>, LlmError>;
    fn stream_chat(&self, req: ChatRequest) -> BoxStream<'_, Result<ChatDelta, LlmError>>;
}
```

Implementations, in priority order: `openai_compat` (base URL + optional key — covers Ollama's OpenAI endpoint, LM Studio, llama.cpp server, OpenRouter, vLLM), `ollama_native` (nicer model listing/pull status), `anthropic`, `openai`. **`openai_compat` is the universal baseline; the others are conveniences.** Keys stored via OS keychain (`keyring` crate), never in plaintext config. Streaming deltas forwarded to the frontend as Tauri events so lyrics render token-by-token.

## 5. Model capability profiles — the core abstraction

The app is model-agnostic. Everything the UI shows for a music model comes from a **profile** (JSON in `profiles/`, user-extensible in the app data dir; app merges both, user dir wins). Profiles are data, not code — supporting a new model means writing a JSON file.

```jsonc
{
  "id": "ace-step-1.5",
  "display_name": "ACE-Step 1.5",
  "kind": "music",                       // music | image
  "license": "Apache-2.0",
  "comfy": {
    "templates": ["ace_step_1_5_t2m"],   // preferred: MCP template ids
    "workflow": null,                    // or an embedded API-format workflow JSON
    "models_needed": [                   // for setup: what to search/install via MCP
      { "search": "ace_step_1.5_turbo_aio", "dest_hint": "checkpoints" }
    ],
    "vram_gb_min": 8
  },
  "inputs": {                            // drives AudioStudio's param panel (see §7)
    "tags":     { "type": "text",   "label": "Style tags", "prefill_key": "tags" },
    "lyrics":   { "type": "lyrics", "structure_tags": ["[verse]","[chorus]","[bridge]","[inst]","[outro]"] },
    "negative": { "type": "text",   "supported": true, "prefill_key": "negative" },
    "duration_s": { "type": "number", "min": 10, "max": 240, "default": 120 },
    "seed":     { "type": "seed" },
    "steps":    { "type": "number", "min": 1, "max": 100, "default": 27 },
    "cfg":      { "type": "number", "min": 1, "max": 15, "default": 5, "step": 0.5 }
  },
  "lyrics_contract": {                   // what LyricsStudio must produce for this model
    "format": "structure-tagged",        // structure-tagged | plain | none (instrumental-only models)
    "languages": ["en","es","ja", "…50+"],
    "instrumental_token": "[inst]",
    "notes": "Short tag combos work best; vocal style tags (e.g. 'deep male voice') go in tags, not lyrics."
  },
  "prompt_guide": {                      // feeds both prefills and the LLM system prompt (§6)
    "tag_style": "comma-separated short tags",
    "examples": [ { "tags": "synthwave, retro, 80s, female vocal, dreamy", "lyrics": "[verse]…" } ]
  }
}
```

Initial profiles to ship (research: docs/MODELS.md): **ace-step-1.5** (default; Apache-2.0, lyrics+vocals, fast on consumer GPUs), **stable-audio-open** (instrumental/SFX, no lyrics), **musicgen** (instrumental, simple), **yue** (lyrics-to-song, 24 GB+ VRAM, marked "advanced"), **diffrhythm** (fast full songs). Plus one image profile (user's choice of SDXL/Flux template) for cover art.

## 6. Lyric generation & prompt optimization

- **System prompt is assembled, not hardcoded**: role ("You are a professional songwriter/producer writing for {model.display_name}") + the target profile's `lyrics_contract` + `prompt_guide` + hard rules (output ONLY lyrics in the required format; obey structure tags; match requested language; no meta-commentary).
- **Brief form** (all prefilled with strong examples): theme/story, genre & style tags, mood, structure (e.g. V-C-V-C-B-C), language, point of view, era/references, explicit allowed y/n, target duration (constrains lyric length).
- Output lands in a versioned editor (each generation/edit = a `LyricDoc` version; cheap full-copy versions, no diffing engine). Approve → available in AudioStudio.
- **Prompt optimization (lyrics brief AND audio tags): opt-in per use, always consented.** The optimizer LLM call returns a rewritten prompt; UI shows original vs optimized side-by-side with an inline word-diff; user Accepts / Edits / Reverts. The *user-approved* text is what gets sent and what gets stored in provenance, flagged `optimized: true|false`. Never auto-apply.

## 7. Audio generation pipeline

1. AudioStudio renders controls from the selected profile's `inputs` (§5) — unsupported controls simply don't render (e.g. no negative box for models without it).
2. On Generate: build `GenerationSpec` (profile id, all input values, lyric doc version ref, seed) → `mcp-bridge.run_template` (or `submit_workflow` when the profile embeds a workflow).
3. Job lifecycle streamed to a **queue panel** (pending/running/progress %/failed with error text). Multiple queued jobs allowed; batch = N seeds of the same spec.
4. On completion: `get_output` → audio copied into the library (§8) with a **provenance sidecar**.

## 8. Library, provenance, storage

- App data dir (`%APPDATA%/latentCreate` / platform equivalents):

```
library/
├── projects/<project-slug>/
│   ├── project.json           # name, created, lyric doc versions, track refs, album lists
│   ├── lyrics/<v>.md
│   ├── tracks/<track-id>.flac # (or wav/mp3 as produced)
│   ├── tracks/<track-id>.json # PROVENANCE SIDECAR: model profile+version, template id,
│   │                          #   every input value, seed, lyric version ref, negative,
│   │                          #   optimized flags, comfy server info, timestamps, duration
│   └── art/<img-id>.png (+ .json sidecar)
└── config.json                # non-secret config (secrets → OS keychain)
```

- JSON files, no database. Human-readable, git-able, trivially portable. Revisit only if scanning gets slow (thousands of tracks).
- Track actions: play, delete (to OS trash, not hard delete), rename, add-to-album-list, export/reveal, **Send to** mixer/mastering.
- **Send to**: v1 opens `https://app.latentmixer.com` / `https://app.latentmastering.com` in the browser and reveals the file for drag-in. The real handoff protocol is **owned by the mixing/mastering repos** and will exist before this repo's Phase 4; latentCreate adopts it then rather than designing it (PROJECT.md decisions log, 2026-08-23).

## 9. Player & visualizer

- Playback in the webview: `<audio>`/Web Audio graph → `AnalyserNode` FFT → canvas spectrum + waveform. **Read-only visualizer, zero custom DSP** — `AnalyserNode` provides the data; we only draw. No Rust audio path, no realtime constraints.
- Visual language matches Latent Mixing/Mastering (dark, violet accent, meters-as-decoration). The sibling repos' viz code is closed-source; the owner may port pieces they own outright, but default is a clean-room reimplementation (it's ~200 lines against AnalyserNode) so this repo stays unencumbered. Anything ported must be listed in THIRD-PARTY-LICENSES bookkeeping.

## 10. Setup & configuration flow (first-run wizard, revisitable in Settings)

Step order, each with live health checks and "why/how" help text:
1. **ComfyUI** — choose Local or Cloud. Local: detect `comfy-mcp` on PATH (offer install instructions `pip install comfy-mcp` if missing), detect/launch ComfyUI (`launch_comfyui`), show server info. Cloud: API key entry (keychain), verify with a discovery call.
2. **Music models** — `list_local_models` filtered against shipped profiles → show "ready" ✅ / "not installed" with one-click `install_model` (progress streamed). Curated catalog first (profiles we ship), full `search_models` search behind an "advanced" expander. Upgrades: profile knows current recommended file; if local differs, show non-nagging "update available".
3. **Lyrics LLM (optional, skippable)** — provider pick, base URL/key, `list_models`, test call. Models the suite recommends for lyrics (docs/MODELS.md — currently Gemma 4 12B, and 26B/31B for high-VRAM users) get a "recommended for lyrics" chip in the list and the 12B is preselected when nothing is chosen; the recommendation list is read as data, and the app never auto-pulls an LLM.
4. **Cover art (optional, skippable)** — pick an installed image model/template.

Config edits never block the main UI after first run; degraded states show as status pills (e.g. "ComfyUI offline — reconnect") not modal walls.

## 11. Frontend rules

- Zustand stores per domain; components never call `invoke` directly — only through `app/src/bridge/` typed wrappers.
- Single `theme.css`; every className styled there; dark professional theme; **no UI framework**. Spacious layout — one primary action per view, progressive disclosure for advanced params.
- Views: `Setup`, `LyricsStudio`, `AudioStudio`, `Library`, `CoverArt` behind a left nav rail. Lyric→Audio handoff is a store action, not navigation state.

## 12. Non-goals (v1)

- No model inference in-app, ever. No bundled models or weights.
- No mixing/mastering DSP — that's the other apps.
- No mobile/web build. Desktop only (Windows first, then macOS/Linux CI builds).
- No workflow *editor* — ComfyUI is the editor; we surface parameters, not graphs.
- No account system, telemetry, or server component.
