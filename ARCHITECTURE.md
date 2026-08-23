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
        [cloud MCP: later, §1]     OpenAI / Anthropic APIs
                     │
                     ▼
              ComfyUI (the user's own install)
```

### Why MCP instead of ComfyUI's raw HTTP API
`comfy-mcp` gives us, through one protocol: model **search and download into ComfyUI** (`search_models`, `download_model`), curated **templates** (`search_templates`, `fetch_template`), **parameter access via slots** (`list_workflow_slots`/`set_workflow_slot` — see §3a), workflow validation against the live node registry (`validate_workflow`, and `local_check` on template fetch), job lifecycle (`run_workflow`, `job`, `fetch_outputs`), the live node registry (`nodes`), and `launch_comfyui`/`stop_comfyui`/`update_comfyui`. The raw `/prompt` API gives none of the discovery/management surface. The raw API remains a documented fallback option (PROJECT.md OQ-3) behind the same trait, but is **not** built unless MCP proves insufficient.

⚠ **Verified 2026-08-23 (docs/MCP-SURFACE.md): local and cloud are NOT the same tool surface.** The tool names above are the *local* ones, observed live; the cloud MCP documentation lists different names for the same jobs (`run_template`/`submit_workflow`, `wait_for_job`, `get_output`, `install_model`, `cql`…). The earlier claim that local vs cloud is "a transport swap, not two integrations" was wrong. `ComfyBackend` is still the right seam, but each backend maps its own tool names, and **the cloud backend must be verified against a live cloud endpoint before it is written** — it cannot be derived from the local one. v1 targets local; cloud follows once verified.

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
│   ├── mcp-bridge/           # MCP client (rmcp). Spawns/attaches local comfy-mcp (stdio).
│   │                         #   Typed wrappers per verified tool (docs/MCP-SURFACE.md).
│   ├── llm-bridge/           # LLM providers behind one trait. reqwest + SSE streaming.
│   └── library/              # on-disk store: projects, tracks, sidecars, config
├── src-tauri/                # Tauri shell: commands, events, single place wiring crates
├── profiles/                 # model capability profiles (JSON, shipped with app) — see §5
├── docs/                     # MCP-SURFACE.md (verified), RESEARCH.md, MODELS.md
└── tasks/                    # ROADMAP.md + phase-N.md briefs
```

## 3. mcp-bridge (Rust)

- MCP client over **stdio** (spawn `comfy-mcp` as child process; `COMFY_BIN` respected). Cloud (streamable HTTP to `https://cloud.comfy.org/mcp` + API key) is a **separate backend impl written later against a verified live endpoint**, not a transport flag — see §1's warning. Use the official Rust MCP SDK (`rmcp`); pin version in the brief that introduces it after verifying current API against docs (CONVENTIONS: never write third-party surfaces from memory).
- The trait is semantic; **method names are ours, tool names are per-backend** (verified local names in parentheses):

```rust
pub trait ComfyBackend: Send + Sync {
    async fn health(&self) -> Result<ServerInfo, ComfyError>;              // server_info
    async fn stats(&self) -> Result<SystemStats, ComfyError>;              // system_stats (VRAM gating)
    async fn search_models(&self, q: ModelQuery) -> Result<Vec<ModelHit>, ComfyError>;
    async fn list_models_in(&self, folder: &str) -> Result<Vec<String>, ComfyError>; // search_models(folder=)
    async fn install_model(&self, spec: &ModelSpec) -> Result<DownloadId, ComfyError>; // download_model
    async fn download_status(&self, id: &DownloadId) -> Result<DownloadState, ComfyError>; // download
    async fn search_templates(&self, q: &str) -> Result<Vec<TemplateInfo>, ComfyError>;
    async fn fetch_template(&self, name: &str, out: &Path) -> Result<LocalCheck, ComfyError>;
    async fn list_slots(&self, wf: &Path) -> Result<Vec<Slot>, ComfyError>;      // list_workflow_slots
    async fn set_slot(&self, wf: &Path, addr: &str, v: SlotValue) -> Result<(), ComfyError>;
    async fn node_schema(&self, class: &str) -> Result<NodeSchema, ComfyError>;  // nodes(action="get")
    async fn validate(&self, wf: &Path) -> Result<Validation, ComfyError>;
    async fn run(&self, wf: &Path) -> Result<JobId, ComfyError>;                // run_workflow(wait=false)
    async fn job_status(&self, id: &JobId) -> Result<JobStatus, ComfyError>;     // job(action="status")
    async fn cancel_job(&self, id: &JobId) -> Result<(), ComfyError>;            // job(action="cancel")
    async fn outputs(&self, id: &JobId) -> Result<Vec<OutputFile>, ComfyError>;  // fetch_outputs
    async fn launch_comfyui(&self) -> Result<(), ComfyError>;   // local only; cloud = typed error
}
```

### 3a. Slots: how parameters actually reach the graph
`list_workflow_slots` returns every tweakable widget as a stable address (`node_id.input_name`, or `A/B.name` inside subgraphs) with its current value; `set_workflow_slot` writes one. **The app never parses or rewrites graph JSON to change a parameter.** Verified surface and gotchas: docs/MCP-SURFACE.md §2–3.

Graph *structure* changes — inserting a LoRA loader, swapping the save node — are the exception and do require editing the workflow file (§5a, §7).

**Two-way mapping rule:** one UI control may drive several slots. ACE-Step 1.5 turbo exposes duration twice (`94.duration` and `98.seconds`) and two independent seeds (`94.seed` planner, `3.seed` sampler). Profiles map one semantic input to a **list** of addresses; the UI shows one control. Hiding exactly this kind of trap is the app's reason to exist.

- Long jobs: poll `job_status` (`job(action="status")`; `"wait"`/`"watch"` also exist) on a tokio task; progress re-emitted to the frontend as Tauri events (`job://progress`, `job://done`, `job://failed`). The UI never polls Rust; Rust pushes.
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

A profile binds **semantic input -> slot address(es)** on a named template. Values below are the *verified* ACE-Step 1.5 turbo surface (docs/MCP-SURFACE.md §3), not illustrative guesses.

```jsonc
{
  "id": "ace-step-1.5-turbo",
  "display_name": "ACE-Step 1.5 XL Turbo",
  "kind": "music",                       // music | image
  "license": "Apache-2.0",
  "comfy": {
    "template": "audio_ace_step1_5_xl_turbo",  // verified gallery name
    "workflow": null,                    // or a user-imported workflow path (§5b)
    "vram_gb_min": 8,                    // runs on a 16 GB card; floor to be measured
    "output": {                          // §7: shipped template saves lossy MP3 -- we override
      "save_node": "SaveAudioAdvanced",
      "prefer_lossless": true
    }
  },
  "loras": {                             // omit entirely for models without LoRA support
    "supported": true,
    "loader_node": "LoraLoaderModelOnly",   // core node, verified; NOT a custom ACE node
    "attach_after": "104",                  // UNETLoader instance the loader is spliced after
    "folder": "loras",
    "strength": { "min": 0.0, "max": 2.0, "default": 1.0, "step": 0.05 },  // node allows -100..100
    "max_stack": 4
  },
  "inputs": {                            // each maps to ONE OR MORE slot addresses (§3a)
    "tags":       { "type": "text",   "slots": ["94.tags"],   "label": "Style tags" },
    "lyrics":     { "type": "lyrics", "slots": ["94.lyrics"],
                    "structure_tags": ["[Verse]","[Chorus]","[Bridge]","[Outro]","[inst]"] },
    "negative":   { "supported": false },                     // verified: no negative input exists
    "duration_s": { "type": "number", "slots": ["94.duration", "98.seconds"],  // BOTH, kept in sync
                    "min": 10, "max": 300, "default": 120 },
    "seed":       { "type": "seed",   "slots": ["94.seed", "3.seed"] },        // planner + sampler
    "bpm":        { "type": "number", "slots": ["94.bpm"], "min": 10, "max": 300, "default": 120 },
    "keyscale":   { "type": "enum",   "slots": ["94.keyscale"], "from_node_choices": true },
    "timesig":    { "type": "enum",   "slots": ["94.timesignature"], "from_node_choices": true },
    "language":   { "type": "enum",   "slots": ["94.language"], "from_node_choices": true },
    "steps":      { "type": "number", "slots": ["3.steps"], "min": 1, "max": 100, "default": 8,
                    "advanced": true },
    "shift":      { "type": "number", "slots": ["78.shift"], "default": 3, "advanced": true },
    "planner":    { "type": "group",  "advanced": true,      // LM-planner sampling controls
                    "members": { "cfg_scale": ["94.cfg_scale"], "temperature": ["94.temperature"],
                                 "top_p": ["94.top_p"], "top_k": ["94.top_k"], "min_p": ["94.min_p"] } }
  },
  "lyrics_contract": {                   // what LyricsStudio must produce for this model
    "format": "structure-tagged",
    "languages_from": "94.language",     // read the real enum from the node schema
    "instrumental_token": "[inst]",
    "notes": "Short tag combos work best; vocal style tags (e.g. 'deep male voice') go in tags, not lyrics."
  },
  "prompt_guide": {                      // feeds both prefills and the LLM system prompt (§6)
    "tag_style": "comma-separated short tags",
    "examples": [ { "tags": "synthwave, retro, 80s, female vocal, dreamy", "lyrics": "[Verse]…" } ]
  }
}
```

**`from_node_choices: true`** means the UI reads the option list from the live node schema (`nodes(action="get")`) rather than duplicating 34 key/scale or 51 language values into the profile — enums stay correct across ComfyUI updates for free.

**Advanced inputs** (`advanced: true`) live behind a disclosure so the default panel stays uncrowded: tags, lyrics, duration, bpm, key, seed.

Initial profiles to ship (docs/MODELS.md): **ace-step-1.5-turbo** (default; verified runnable, Apache-2.0, lyrics+vocals, LoRA ecosystem), **minimax-music-3** (template verified to exist; profile blocked on a machine where it runs), **stable-audio-open** (instrumental/SFX, no lyrics), **musicgen** (instrumental, simple), **yue** (24 GB+ VRAM, "advanced"), **diffrhythm**. Plus one image profile for cover art. The gallery also carries ACE-Step base/SFT/split variants and **v1 M2M editing + instrumentals** templates — the M2M one is the natural home for the backlog's audio-to-audio flows.

**Profile authoring rule:** a profile is only written against a template whose `local_check` reports `runnable: true` on a real install, with its slot list read from `list_workflow_slots`. Profiles are never authored from documentation or model cards — the 2026-08-23 verification found several documented assumptions false (docs/MCP-SURFACE.md §7).

### 5a. LoRAs (first-class, not an afterthought)
Custom-trained LoRAs are core to how serious users work with ACE-Step (the owner's own workflow is ACE-Step 1.5 turbo + custom LoRAs), so they are a first-class input, not an advanced escape hatch:
- **Discovery (verified):** `nodes(action="get", name="LoraLoaderModelOnly")` returns `lora_name` as a COMBO whose `choices` are the installed LoRA paths. This is the authoritative list because it is what the graph will accept.
- **UI:** a LoRA stack panel in AudioStudio — up to `max_stack` entries, each a picker + strength slider, reorderable and individually bypassable. Hidden entirely when the profile has no `loras` block.
- **The picker is a real design problem, not a dropdown.** On the owner's install the raw list is 95 entries of which ~9 are usable: training-run epoch checkpoints dominate, `training_state.pt` files appear but are not loadable, one directory holds five different adapters, and case-variant duplicates appear on Windows. The picker must filter non-adapters, group by directory, collapse epoch series to `final`/latest behind an expander, dedupe case variants, and support user-assigned display names and favorites. Full evidence: docs/MCP-SURFACE.md §4.
- **Provenance:** the stack (file identity + strength + order) is recorded per track (§8). A LoRA-generated track that can't be reproduced from its sidecar is a bug, not a nuance.
- **Training is out of scope** — ComfyUI custom nodes already train ACE-Step LoRAs. This app consumes LoRAs, it does not train them.

### 5b. Custom workflow import (user profiles)
Users with a working ComfyUI workflow — including LoRA wiring no shipped profile covers — import it rather than being forced onto our templates. Flow: pick an exported **API-format** workflow JSON → `validate_workflow` against the live node registry → a mapping screen asks which node input receives tags, lyrics, negative, duration, seed, steps, CFG (candidates pre-suggested by node class and input name) → saved as a user profile, indistinguishable from shipped ones thereafter. This is the pressure-release valve that keeps the profile abstraction from becoming a cage.

## 6. Lyric generation & prompt optimization

- **System prompt is assembled, not hardcoded**: role ("You are a professional songwriter/producer writing for {model.display_name}") + the target profile's `lyrics_contract` + `prompt_guide` + hard rules (output ONLY lyrics in the required format; obey structure tags; match requested language; no meta-commentary).
- **Brief form** (all prefilled with strong examples): theme/story, genre & style tags, mood, structure (e.g. V-C-V-C-B-C), language, point of view, era/references, explicit allowed y/n, target duration (constrains lyric length).
- Output lands in a versioned editor (each generation/edit = a `LyricDoc` version; cheap full-copy versions, no diffing engine). Approve → available in AudioStudio.
- **Prompt optimization (lyrics brief AND audio tags): opt-in per use, always consented.** The optimizer LLM call returns a rewritten prompt; UI shows original vs optimized side-by-side with an inline word-diff; user Accepts / Edits / Reverts. The *user-approved* text is what gets sent and what gets stored in provenance, flagged `optimized: true|false`. Never auto-apply.

## 7. Audio generation pipeline

1. AudioStudio renders controls from the selected profile's `inputs` (§5) — unsupported controls simply don't render (e.g. **no negative box for ACE-Step 1.5**, which has no such input).
2. On Generate: `fetch_template` to a per-job working copy → apply the `GenerationSpec` (profile id, all input values, LoRA stack, lyric doc version ref, seed) by `set_slot` per mapped address → `run(wf)`. **Every job gets its own workflow file**; the app never mutates a shared one (the MCP docs warn about TOCTOU on shared paths).
3. **Graph edits** happen on that working copy before the run, for the two cases slots cannot express: splicing `LoraLoaderModelOnly` nodes after the profile's `attach_after` node, and replacing the template's deprecated `SaveAudioMP3` with `SaveAudioAdvanced` writing lossless. Shipping lossy MP3 into a mastering chain would undercut the whole suite. (Open: `SaveAudioAdvanced.format` is a V3 dynamic combo — mechanism TBD, MCP-SURFACE §5.)
4. `validate_workflow` on the edited copy before submitting — cheap, and it catches a bad splice before a GPU-minutes-long failure.
5. Job lifecycle streamed to a **queue panel** (pending/running/progress %/failed with error text) via `job(action="status"|"watch")`. Multiple queued jobs allowed; batch = N seeds of the same spec.
6. On completion: `fetch_outputs` → audio copied into the library (§8) with a **provenance sidecar** that includes the resolved slot values actually submitted, not just the UI values.

## 8. Library, provenance, storage

- App data dir (`%APPDATA%/latentCreate` / platform equivalents):

```
library/
├── projects/<project-slug>/
│   ├── project.json           # name, created, lyric doc versions, track refs, album lists
│   ├── lyrics/<v>.md
│   ├── tracks/<track-id>.flac # (or wav/mp3 as produced)
│   ├── tracks/<track-id>.json # PROVENANCE SIDECAR: model profile+version, template id,
│   │                          #   every input value, LoRA stack (file+strength+order),
│   │                          #   seed, lyric version ref, negative, optimized flags,
│   │                          #   comfy server info, timestamps, duration
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
2. **Music models** — installed models (`search_models(folder=…)`) filtered against shipped profiles → show "ready" ✅ / "not installed" with one-click install (`download_model` + `download` progress streamed; note it refuses outright when a remote ComfyUI target is configured). Curated catalog first (profiles we ship), full `search_models` search behind an "advanced" expander. Upgrades: profile knows current recommended file; if local differs, show non-nagging "update available".
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
