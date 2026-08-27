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
├── package.json              # root: owns the Tauri CLI + `npm run dev|build`
├── app/                      # React frontend (Vite), its own package.json
│   ├── src/
│   │   ├── state/            # Zustand stores (config.ts, lyrics.ts, audio.ts, library.ts, jobs.ts)
│   │   ├── views/            # Setup, LyricsStudio, AudioStudio, Library, CoverArt
│   │   ├── components/       # shared UI (Player, Visualizer, PromptDiff, ParamControl, …)
│   │   ├── bridge/           # ONLY place that calls Tauri invoke/listen (typed wrappers)
│   │   └── theme.css         # single source of styling truth
├── crates/
│   ├── create-core/          # domain types: Project, Track, LyricDoc, GenerationSpec,
│   │                         #   ModelProfile, Provenance. Plus the pure transforms over
│   │                         #   them: slot resolution (T-304) and the workflow graph edits
│   │                         #   slots cannot express (T-305). No I/O. Serde everywhere.
│   ├── mcp-bridge/           # MCP client (rmcp). Spawns/attaches local comfy-mcp (stdio).
│   │                         #   Typed wrappers per verified tool (docs/MCP-SURFACE.md).
│   ├── llm-bridge/           # LLM providers behind one trait. reqwest + SSE streaming.
│   └── library/              # on-disk store: projects, tracks, sidecars, config
├── src-tauri/                # Tauri shell: commands, events, single place wiring crates
│   ├── capabilities/         # Tauri permission sets (main window)
│   └── icons/                # desktop icon set (no android/ios -- desktop only)
├── profiles/                 # model capability profiles (JSON, shipped with app) — see §5
├── docs/                     # MCP-SURFACE.md (verified), RESEARCH.md, MODELS.md
└── tasks/                    # ROADMAP.md + phase-N.md briefs
```

## 3. mcp-bridge (Rust)

- MCP client over **stdio** (spawn `comfy-mcp` as child process; `COMFY_BIN` respected). Cloud (streamable HTTP to `https://cloud.comfy.org/mcp` + API key) is a **separate backend impl written later against a verified live endpoint**, not a transport flag — see §1's warning. Use the official Rust MCP SDK (`rmcp`); pin version in the brief that introduces it after verifying current API against docs (CONVENTIONS: never write third-party surfaces from memory).
- The trait below is a **design sketch, not a live contract** — deferred (see the note after it)
  and already drifted from the landed method set (`search_templates(query, limit) -> TemplateSearch`,
  batch `set_slots`, `list_slots -> SlotList`, plus `get_template`/`notes` which it omits).
  **Method names are ours, tool names are per-backend** (verified local names in parentheses):

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

**⚠ `ComfyBackend` deferred again (2026-08-24).** First deferred from T-101 to T-104; at T-104 it
is deferred once more, held off until a second backend (cloud) is verified. Three concrete
reasons: still a single impl (the original reason not to guess), the sketch above has already
drifted from landed code, and §1 shows local/cloud are different tool surfaces best shaped by
real divergence — the eventual seam is more likely `enum Backend { Local, Cloud }` than this
17-method trait. Until then `mcp-bridge` exposes `LocalComfy` concretely and Tauri managed state
holds `Arc<LocalComfy>`. Recorded in PROJECT.md's decisions log.

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

**Verified 2026-08-24 (docs/LLM-SURFACE.md) — the trait above is a sketch, and one finding
reshapes it.** Streaming does not deliver a single kind of text. Providers send
chain-of-thought in `delta.reasoning` (Ollama, OpenRouter, current vLLM) or
`delta.reasoning_content` (DeepSeek, older vLLM), and on a live capture the model this app
recommends for lyrics produced **163 characters of reasoning to 5 of content** for a
one-word answer. So `ChatDelta` is an enum, not a string: `Content` is the only variant
that may reach the user's document, `Reasoning` is status text, `Refusal` is the model
declining (its text never appears in `content`), and `Finished`/`Usage` are terminal. A
provider that merged the two text kinds would write the model's deliberation into the
user's song.

**⚠ T-109 answered the trait question, and the answer was no (2026-08-24).** The second
implementation turned out not to be an implementation of the same thing. `ollama_native`
**does not chat** — Ollama's own `/v1/chat/completions` already does that through
`openai_compat`, and a second path to the same tokens would be two things to keep correct.
What the native API adds is *facts about models*: which can chat at all (an embedding model
is indistinguishable on `/v1/models`), which emit reasoning, which run on someone else's
hardware, how much context they hold.

So `OllamaNative` is **an enrichment layer over an endpoint that happens to be Ollama**,
not a peer of `OpenAiCompat`. Forcing it into a `LlmProvider` trait would mean a
`stream_chat` that returns an error — the shape of a wrong abstraction. The trait, if it is
ever written, is for providers that *chat*: `openai_compat`, `anthropic`, `openai`. There is
still exactly one of those, so it stays deferred, now for a stronger reason than "only one
impl" — the obvious second candidate proved to be a different kind of thing. `anthropic`
is what will finally settle it, because it genuinely chats with a different wire format.

Consequence for the wizard: the user configures **one** OpenAI-compatible base URL, and the
app probes `/api/version` on its parent to decide whether the enrichment is available.
Everything still works when it is not; the picker simply shows less.

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
    "negative":   { "type": "unsupported",                    // verified: no negative input exists
                    "reason": "TextEncodeAceStepAudio1.5 exposes no negative input" },
    "duration_s": { "type": "float",  "slots": ["94.duration", "98.seconds"],  // BOTH, kept in sync
                    "min": 10, "max": 300, "default": 120 },
    "seed":       { "type": "seed",   "slots": ["94.seed", "3.seed"] },        // planner + sampler
    "bpm":        { "type": "int",    "slots": ["94.bpm"], "min": 10, "max": 300, "default": 120 },
    "keyscale":   { "type": "enum",   "slots": ["94.keyscale"], "from_node_choices": true },
    "timesig":    { "type": "enum",   "slots": ["94.timesignature"], "from_node_choices": true },
    "language":   { "type": "enum",   "slots": ["94.language"], "from_node_choices": true },
    "steps":      { "type": "int",    "slots": ["3.steps"], "min": 1, "max": 100, "default": 8,
                    "advanced": true },
    "shift":      { "type": "float",  "slots": ["78.shift"], "default": 3, "advanced": true },
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

**`slot_overrides`** pins slot values the profile applies to the fetched template before the user's inputs — the mechanism for a profile to target a specific checkpoint variant. MiniMax Music 3's template hardcodes the fp16 DiT, so its profile overrides `37/6.unet_name` to the int8 file (MCP-SURFACE §6). Values are typed `InputValue` (a COMBO override is `{"type": "enum", "value": "..."}`), not bare strings.

**Advanced inputs** (`advanced: true`) live behind a disclosure so the default panel stays uncrowded: tags, lyrics, duration, bpm, key, seed.

**Two schema rules that come from Rust, decided in T-003:**
- **`int` and `float` are separate input types**, not one `number`. Seeds are the reason: ACE-Step's seed range runs to `u64::MAX` (18446744073709551615), which `f64` cannot represent exactly, so a single float-backed numeric type would silently corrupt seeds and break reproducibility. `seed` is its own type carrying `u64`.
- **Unsupported inputs are declared, not omitted** (`"type": "unsupported"` with a `reason`). Omission cannot distinguish "we checked, this model has no negative prompt" from "nobody thought about it" -- and this project has already been wrong once by assuming a capability existed.

Maps are `BTreeMap`, so serialised profiles and provenance sidecars have stable key order and diff cleanly in git.

Initial profiles to ship (docs/MODELS.md): **ace-step-1.5-turbo** (default; verified runnable, Apache-2.0, lyrics+vocals, LoRA ecosystem), **minimax-music-3** (template verified to exist; profile blocked on a machine where it runs), **stable-audio-open** (instrumental/SFX, no lyrics), **musicgen** (instrumental, simple), **yue** (24 GB+ VRAM, "advanced"), **diffrhythm**. Plus one image profile for cover art. The gallery also carries ACE-Step base/SFT/split variants and **v1 M2M editing + instrumentals** templates — the M2M one is the natural home for the backlog's audio-to-audio flows.

**Loading, and checking a profile against its template (T-107).** `library::profiles`
merges the shipped `profiles/` directory with the user's, keyed by id, user winning
collisions wholesale; loading never fails, and every unreadable or malformed file becomes
a `ProfileWarning` the UI can show. The check that a profile's slot addresses actually
exist in its template is deliberately split across the crates that already own each half:
`ModelProfile::slot_addresses()` in `create-core` collects every address the profile names
(inputs with groups walked, `slot_overrides` keys, and `lyrics_contract.languages_from`),
and `mcp-bridge`'s `SlotList::missing` does the comparison against a fetched template. They
meet in `src-tauri`. `library` therefore never depends on `mcp-bridge`, and a wrong address
— which ComfyUI would otherwise absorb by running the template's own defaults — is
reportable without a live server in any test.

**Profile authoring rule:** a profile is only written against a template whose `local_check` reports `runnable: true` on a real install, with its slot list read from `list_workflow_slots`. Profiles are never authored from documentation or model cards — the 2026-08-23 verification found several documented assumptions false (docs/MCP-SURFACE.md §7).

### 5a. LoRAs (first-class, not an afterthought)
Custom-trained LoRAs are core to how serious users work with ACE-Step (the owner's own workflow is ACE-Step 1.5 turbo + custom LoRAs), so they are a first-class input, not an advanced escape hatch:
- **Discovery (verified):** `nodes(action="get", name="LoraLoaderModelOnly")` returns `lora_name` as a COMBO whose `choices` are the installed LoRA paths. This is the authoritative list because it is what the graph will accept.
- **UI:** a LoRA stack panel in AudioStudio — up to `max_stack` entries, each a picker + strength slider, reorderable and individually bypassable. Hidden entirely when the profile has no `loras` block.
- **The picker is a real design problem, not a dropdown.** On the owner's install the raw list was 95 entries of which ~9 were usable (**re-read 2026-08-27: now 53 entries, ~10 usable, and the case-variant directory is no longer present** — MCP-SURFACE §16.5; the shape of the problem is unchanged): training-run epoch checkpoints dominate, `training_state.pt` files appear but are not loadable, one directory holds five different adapters, and case-variant duplicates appear on Windows. The picker must filter non-adapters, group by directory, collapse epoch series to `final`/latest behind an expander, dedupe case variants, and support user-assigned display names and favorites. Full evidence: docs/MCP-SURFACE.md §4.
- **Provenance:** the stack (file identity + strength + order) is recorded per track (§8). A LoRA-generated track that can't be reproduced from its sidecar is a bug, not a nuance.
- **Training is out of scope** — ComfyUI custom nodes already train ACE-Step LoRAs. This app consumes LoRAs, it does not train them.

### 5b. Custom workflow import (user profiles)
Users with a working ComfyUI workflow — including LoRA wiring no shipped profile covers — import it rather than being forced onto our templates. Flow: pick an exported **API-format** workflow JSON → `validate_workflow` against the live node registry → a mapping screen asks which node input receives tags, lyrics, negative, duration, seed, steps, CFG (candidates pre-suggested by node class and input name) → saved as a user profile, indistinguishable from shipped ones thereafter. This is the pressure-release valve that keeps the profile abstraction from becoming a cage.

## 6. Lyric generation & prompt optimization

- **System prompt is assembled, not hardcoded**: role ("You are a professional songwriter/producer writing for {model.display_name}") + the target profile's `lyrics_contract` + `prompt_guide` + hard rules (output ONLY lyrics in the required format; obey structure tags; match requested language; no meta-commentary).
- **Brief form** (all prefilled with strong examples): theme/story, genre & style tags, mood, structure (e.g. V-C-V-C-B-C), language, point of view, era/references, explicit allowed y/n, target duration (constrains lyric length).
- Output lands in a versioned editor (each generation/edit = a `LyricDoc` version; cheap full-copy versions, no diffing engine). Approve → available in AudioStudio.
- **Prompt optimization (lyrics brief AND audio tags): opt-in per use, always consented.** The optimizer LLM call returns a rewritten prompt; UI shows original vs optimized side-by-side with an inline word-diff; user Accepts / Edits / Reverts. The *user-approved* text is what gets sent and what gets stored in provenance, flagged `optimized: true|false`. Never auto-apply.
- **What the lyric optimizer rewrites is the assembled user message** (T-210), not the lyrics and not the form fields: the labelled lines are the prompt, so they are what the user is asked to accept. The optimizer prompt declares Theme / Genre and style tags / Mood / Era and references rewritable and Structure / Language / Point of view / Explicit content allowed / Target duration fixed, and **nothing enforces that but the diff** -- a rewritten settings line arrives as a highlighted change the user must accept. `<PromptDiff>` is provider- and domain-agnostic (two texts, Accept / Edit / Revert) and is the component Phase 3's audio tags reuse.

## 7. Audio generation pipeline

1. AudioStudio renders controls from the selected profile's `inputs` (§5) — unsupported controls simply don't render (e.g. **no negative box for ACE-Step 1.5**, which has no such input).
2. On Generate: `fetch_template` to a per-job working copy → apply the `GenerationSpec` (profile id, all input values, LoRA stack, lyric doc version ref, seed) by `set_slot` per mapped address → `run(wf)`. **Every job gets its own workflow file**; the app never mutates a shared one (the MCP docs warn about TOCTOU on shared paths).
3. **Graph edits** happen on that working copy before the run, for the two cases slots cannot express: splicing `LoraLoaderModelOnly` nodes after the profile's `attach_after` node, and making the save node write lossless. Shipping lossy MP3 into a mastering chain would undercut the whole suite.
   - **The test is the format value, not the node class** (verified 2026-08-27, MCP-SURFACE §16.3). ACE-Step's template ships `SaveAudioMP3`; MiniMax's ships `SaveAudioAdvanced` **already set to `mp3`/`V0`**. A check that only asks "is this the modern node" passes MiniMax and ships MP3 — the outcome this rule exists to prevent.
   - **`format` is set by graph edit, never by slot** (MCP-SURFACE §16.1): it is a `COMFY_DYNAMICCOMBO_V3` that `list_workflow_slots` does not surface and `set_workflow_slot` rejects with `[workflow_slot_invalid]`. It is a positional entry in the node's `widgets_values`, and **the array length varies by format** — `flac` has no sub-widget (2 entries), `mp3`/`opus` carry a `quality` sub-combo (3). Truncate; do not overwrite in place.
   - **`flac` is the only lossless option** the node offers — there is no WAV — and it writes **16-bit/48 kHz with no bit-depth control**. The UI must not offer 24-bit.
   - `filename_prefix` on the swapped node is still a normal slot, so only the format needs the graph edit.
4. `validate_workflow` on the edited copy before submitting — cheap, and it catches a bad splice before a GPU-minutes-long failure.
5. Job lifecycle streamed to a **queue panel** (pending/running/progress %/failed with error text) via `job(action="status"|"watch")`. Multiple queued jobs allowed; batch = N seeds of the same spec.
6. On completion: `fetch_outputs` → audio copied into the library (§8) with a **provenance sidecar** that includes the resolved slot values actually submitted, not just the UI values.

## 8. Library, provenance, storage

- App data dir (`%APPDATA%/latentCreate` / platform equivalents):

```
library/
├── projects/<project-slug>/
│   ├── project.json           # name, created, lyric doc versions, track refs, album lists
│   ├── lyrics/<doc-id>.json   # one LyricDoc, every version inline (see below)
│   ├── tracks/<track-id>.flac # (or wav/mp3 as produced)
│   ├── tracks/<track-id>.json # SIDECAR = the whole Track record, incl. Provenance:
│   │                          #   model profile+licence, template, the GenerationSpec
│   │                          #   the user chose, the RESOLVED slot values actually
│   │                          #   submitted, LoRA stack (file+strength+order), seed,
│   │                          #   lyric version ref, optimized flags, comfy server
│   │                          #   info, timestamps, duration
│   └── art/<img-id>.png (+ .json sidecar)
└── config.json                # non-secret config (secrets → OS keychain)
```

- JSON files, no database. Human-readable, git-able, trivially portable. Revisit only if scanning gets slow (thousands of tracks).
- **One file per lyric document** (2026-08-25, Phase 2 boundary): `lyrics/<doc-id>.json`
  holds the whole `LyricDoc` with every version inline, and `project.json` holds only the
  ordered doc ids. The earlier sketch here was `lyrics/<v>.md` per version, written before
  `LyricDoc` existed; it would put a version's text in one file and its `source`,
  `created_at` and `approved` flag in another -- the same two-files-disagreeing hazard the
  track rule below exists to prevent, for a few KB of saving.
- **One source of truth per track** (T-003b): `project.json` holds an *ordered list of track ids* and the album lists; everything about a track — title, file, duration, provenance — lives only in its sidecar. Duplicating a title into both files would guarantee the two drift apart on the first rename.
- **Provenance stores both levels**: the `GenerationSpec` the user chose (semantic, e.g. `duration_s = 120`) *and* the resolved slot values actually submitted (e.g. `94.duration = 120`, `98.seconds = 120`). The first powers "re-use these settings"; the second is what makes a track reproducible and is the only record of what the graph really received.
- Track actions: play, delete (to OS trash, not hard delete), rename, add-to-album-list, export/reveal, **Send to** mixer/mastering.
- **Send to**: v1 opens `https://app.latentmixer.com` / `https://app.latentmastering.com` in the browser and reveals the file for drag-in. The real handoff protocol is **owned by the mixing/mastering repos** and will exist before this repo's Phase 4; latentCreate adopts it then rather than designing it (PROJECT.md decisions log, 2026-08-23).

## 9. Player & visualizer

- Playback in the webview: `<audio>`/Web Audio graph → `AnalyserNode` FFT → canvas spectrum + waveform. **Read-only visualizer, zero custom DSP** — `AnalyserNode` provides the data; we only draw. No Rust audio path, no realtime constraints.
- Visual language follows **`latentbeats.com`**, the umbrella brand (`../website/latentbeats.com/css/style.css`): deep blue-black ground `#0a0e1a`, accent `#58a6ff`, 12px radii, meters-as-decoration. The suite moved violet -> blue in Aug 2026; the site carries the newest tokens, the two sibling apps still carry the older GitHub-dark ground. latentCreate tracks the site (T-001) — it has no legacy screens to migrate, and the accent is identical either way. The sibling repos' viz code is closed-source; the owner may port pieces they own outright, but default is a clean-room reimplementation (it's ~200 lines against AnalyserNode) so this repo stays unencumbered. Anything ported must be listed in THIRD-PARTY-LICENSES bookkeeping.

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
