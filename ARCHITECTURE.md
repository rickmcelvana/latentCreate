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
│              comfy-bridge    llm-bridge      library    │
│              (HTTP client)   (HTTP client)   (projects, │
│                    │              │           audio,    │
└────────────────────┼──────────────┼───────────sidecars)─┘
                     ▼              ▼
     ComfyUI's own HTTP API      Ollama / OpenAI-compat /
     (the user's install, local  OpenAI / Anthropic APIs
      or at an address they give)
```

### Why ComfyUI's own HTTP API, and not comfy-mcp (decided 2026-11-11, T-519)
Until T-519 the app spawned `comfy-mcp` over stdio, which ran every tool through comfy-cli. That
stack **could not work on any machine but the developer's**, and it is gone. Measured live
([docs/COMFY-HTTP-SURFACE.md](docs/COMFY-HTTP-SURFACE.md) section 1): comfy-cli is an undeclared
dependency, so a user who followed the app's own `pip install comfy-mcp` instruction got a pill
reading "Unreachable" with no next step; and comfy-cli resolves its own "workspace" independently of
the ComfyUI that is running, so on a Comfy Desktop install `download_model` wrote model files into
`~/Documents/comfy/ComfyUI/models` while ComfyUI read from another drive -- a download that
completed and could never read as installed.

ComfyUI itself serves everything the app used the MCP layer for: health (`/system_stats`), the node
registry (`/object_info`), jobs (`/prompt`, `/queue`, `/history`, `/interrupt`), outputs (`/view`),
model inventory (`/models/<folder>`) and the model directories it searches
(`/internal/folder_paths`). Talking to it directly means **no Python, pip, comfy-mcp or comfy-cli
on the user's machine**; the address of the ComfyUI is the only configuration, which also makes a
ComfyUI on another machine (T-515) a URL rather than a second product. What the app gives up is
comfy-cli's template fetching, frontend-to-API conversion and slot tooling -- replaced by shipping
each curated profile's workflow **already in API format** (§3a, §5), which is also what made every
slot write verifiable by reading the graph.

The comfy-mcp design notes, its verified tool surface and the gotchas it accumulated are kept as a
historical record in [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md). Its findings about **ComfyUI and
the models** (ACE-Step non-reproducibility, LoRA reachability, `/history` as the record of what ran)
still hold; its findings about comfy-mcp tools do not.

The app is a plain HTTP client of ComfyUI -- no LLM in the loop for generation. The lyric LLM is a separate HTTP concern.

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
│   │                         #   them: slot resolution (T-304), the workflow graph edits
│   │                         #   slots cannot express (T-305), and the audit of whether a
│   │                         #   slot write can reach the engine (T-306a). No I/O.
│   ├── comfy-bridge/         # ComfyUI HTTP client (reqwest) behind a transport seam, plus
│   │                         #   the model downloader. Verified surface: docs/COMFY-HTTP-SURFACE.md.
│   ├── llm-bridge/           # LLM providers behind one trait. reqwest + SSE streaming.
│   └── library/              # on-disk store: projects, tracks, sidecars, config
├── src-tauri/                # Tauri shell: commands, events, single place wiring crates
│   ├── capabilities/         # Tauri permission sets (main window)
│   └── icons/                # desktop icon set (no android/ios -- desktop only)
├── profiles/                 # model capability profiles (JSON, shipped with app) — see §5
│   └── workflows/            # each curated profile's workflow, in ComfyUI's API format
├── docs/                     # COMFY-HTTP-SURFACE.md (verified), MCP-SURFACE.md (historical), MODELS.md
└── tasks/                    # ROADMAP.md + phase-N.md briefs
```

## 3. comfy-bridge (Rust)

- **`ComfyHttp`** is a reqwest client over one base URL, behind a small **transport seam** (a trait
  taking method, path and body, returning status and bytes) so every call sequence is testable
  offline against fixtures captured live in `testdata/comfy-http/` (WORKFLOW section 5: no test may
  need a live ComfyUI). Methods are thin and typed, one per verified route:
  `system_stats`, `object_info(class)`, `models_in(folder)`, `folder_paths`, `submit(graph)`,
  `queue`, `history(id)`, `interrupt(id)`, `delete_pending(id)`, `view(output)`.
- **Errors say what to do next.** `ComfyError` separates *nobody answered at this address*
  (`Unreachable`, carrying the URL) from *ComfyUI refused the workflow* (`Rejected`, carrying the
  parsed `node_errors` so the message names the node and input) from everything else. Never a raw
  body in a user-facing string; the raw body goes to the session log (redacted).
- **Finding ComfyUI.** `ComfyConfig.url` when the user set one; otherwise probe
  `http://127.0.0.1:8188`, then `:8000`. Only a decodable `/system_stats` counts as found -- the web
  UI also answers `/`. The found address is shown on the Setup card. The app **does not start
  ComfyUI**: there is no portable way to, and the card says how ("Start ComfyUI, then Retry").
- **Model downloads are the app's own** (`comfy-bridge::download`): stream the profile's curated
  `source_url` to `<dir>/<file>.partial`, check `size_bytes`, rename into place. `<dir>` comes from
  `/internal/folder_paths` (first directory whose last component is the folder name, else the
  first), and Install is offered **only when that directory exists on this machine** -- i.e. a
  local ComfyUI. Because ComfyUI's model lists refresh live (COMFY-HTTP-SURFACE 3), the row reads
  Installed on the next readiness check, with no restart.
- **Jobs.** Rust polls `/queue` and `/history/<id>` once a second per live job and pushes
  `job://progress`, `job://done`, `job://failed`, `job://cancelled`; the UI never polls Rust. Status
  words are the ones the frontend already renders: `pending`, `running`, `completed`, `error`,
  `cancelled`. Cancel is `/interrupt` for a running job and `/queue {delete}` for a pending one; a
  deleted pending job never reaches history, so the pump remembers that it asked. A job absent from
  both for three consecutive polls and not cancelled is reported as lost ("ComfyUI restarted or the
  queue was cleared -- generate again").
- **Outputs come over HTTP** (`/view`), never by path, so ingest works against a remote ComfyUI.
- All requests and results are logged (redacted) to the rotating session log for diagnostics.

### 3a. Slots: how parameters actually reach the graph
A workflow is ComfyUI's **API format** (`{node_id: {class_type, inputs}}`). A slot address is
`<node_id>.<input_name>`, split at the **first** `.` because node ids never contain one and input
names can (`111.format.quality`). Flattened subgraph nodes carry `parent:child` ids (`76:3.seed`).

**A slot write is valid exactly when the input exists and holds a literal.** A link
(`["109", 0]`) means another node supplies that value and the write would be ignored, so
`create-core::audit_slots` refuses it before anything is sent; a missing node or input is refused
too. In API format this check is complete: frontend-only `PrimitiveNode`s have already been inlined
as literals and backend `PrimitiveInt`s are real nodes, so the comfy-mcp era's "applied is not
effective" trap (MCP-SURFACE 18.1) cannot hide. Writes are plain JSON assignments of
`InputValue::to_slot_value`, never `serde_json::to_value` (which would send the adjacent tag).

Graph *structure* changes -- inserting a LoRA loader, making the save node lossless -- are the
exception and are graph edits (§5a, §7), applied after the slot writes.

**Two-way mapping rule:** one UI control may drive several slots. ACE-Step 1.5 turbo carries
duration twice (`94.duration` and `98.seconds`), and profiles map one semantic input to a **list**
of addresses while the UI shows one control. Its seed is `109.value`, the `PrimitiveInt` that feeds
both samplers -- in API format `3.seed` and `94.seed` are visibly links, so the audit refuses them.

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

A profile binds **semantic input -> slot address(es)** on a workflow it ships with, in ComfyUI's API format (§3a). Values below are the *verified* ACE-Step 1.5 turbo surface, not illustrative guesses.

```jsonc
{
  "id": "ace-step-1.5-turbo",
  "display_name": "ACE-Step 1.5 XL Turbo",
  "kind": "music",                       // music | image
  "license": "Apache-2.0",
  "comfy": {
    "workflow": "workflows/ace-step-1.5-turbo.json", // API-format graph, relative to this file
    "template": "audio_ace_step1_5_xl_turbo",  // informational: the gallery template it came from
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
    "seed":       { "type": "seed",   "slots": ["109.value"] },                // the PrimitiveInt that
                                                                               // drives 94.seed and
                                                                               // 3.seed; writing those
                                                                               // two directly is inert
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

**`from_node_choices: true`** means the UI reads the option list from the live node schema (`/object_info/<class>`) rather than duplicating 34 key/scale or 51 language values into the profile — enums stay correct across ComfyUI updates for free.

**`slot_overrides`** pins slot values the profile applies to its workflow before the user's inputs — the mechanism for a profile to target a specific checkpoint variant. MiniMax Music 3's template hardcodes the fp16 DiT, so its profile overrides `37:6.unet_name` to the int8 file (MCP-SURFACE §6). Values are typed `InputValue` (a COMBO override is `{"type": "enum", "value": "..."}`), not bare strings.

**Advanced inputs** (`advanced: true`) live behind a disclosure so the default panel stays uncrowded: tags, lyrics, duration, bpm, key, seed.

**Two schema rules that come from Rust, decided in T-003:**
- **`int` and `float` are separate input types**, not one `number`. Seeds are the reason: ACE-Step's seed range runs to `u64::MAX` (18446744073709551615), which `f64` cannot represent exactly, so a single float-backed numeric type would silently corrupt seeds and break reproducibility. `seed` is its own type carrying `u64`.
- **Unsupported inputs are declared, not omitted** (`"type": "unsupported"` with a `reason`). Omission cannot distinguish "we checked, this model has no negative prompt" from "nobody thought about it" -- and this project has already been wrong once by assuming a capability existed.

Maps are `BTreeMap`, so serialised profiles and provenance sidecars have stable key order and diff cleanly in git.

Shipped audio profiles: **ace-step-1.5-turbo** (default; Apache-2.0, lyrics+vocals, LoRA ecosystem), **ace-step-1.5-xl-base** and **ace-step-1.5-xl-sft** (T-513; Apache-2.0, the non-distilled variants), **minimax-music-3**. **Stable Audio is backlog** (PROJECT.md decisions log 2026-09-05: SA 3.0's template is slot-unreliable, SA Open 1.0's `t5-base` encoder has no ungated ComfyUI source). Also backlog/excluded from the planned list: **musicgen** (CC-BY-NC, non-commercial), **yue** (24 GB+, not a gallery template — needs a hand-authored workflow), **diffrhythm**. Plus five commercial-safe image profiles for cover art (T-511): Flux.1 Schnell FP8, Chroma1 HD, SDXL 1.0, SD 3.5 Large FP8, Qwen-Image. The gallery also carries ACE-Step split variants and **v1 M2M editing + instrumentals** templates — the M2M one is the natural home for the backlog's audio-to-audio flows.

**Loading, and checking a profile against its workflow (T-107, re-based on T-519).**
`library::profiles` merges the shipped `profiles/` directory with the user's, keyed by id, user
winning collisions wholesale; loading never fails, and every unreadable or malformed file becomes a
`ProfileWarning` the UI can show. `comfy.workflow` resolves **relative to the directory the profile
was loaded from** (`profiles/workflows/` for shipped ones, `<app data>/profiles/workflows/` for
imports). Because the workflow ships beside the profile, the check that every address the profile
names (`ModelProfile::slot_addresses()`: inputs with groups walked, `slot_overrides` keys,
`lyrics_contract.languages_from`) exists **and holds a literal** runs in `create-core`'s own test
suite against every shipped profile -- no server, no fetched template, no drift between a profile
and the graph it writes into.

**Profile authoring rule:** a curated profile's workflow is the gallery template converted by the
live ComfyUI frontend (the same code path as **Export (API)**), saved to `profiles/workflows/`, and
its addresses read off that file. Profiles are never authored from documentation or model cards —
the 2026-08-23 verification found several documented assumptions false (docs/MCP-SURFACE.md §7).
A new or updated model is: convert, map, run once live, commit the JSON pair.

### 5a. LoRAs (first-class, not an afterthought)
Custom-trained LoRAs are core to how serious users work with ACE-Step (the owner's own workflow is ACE-Step 1.5 turbo + custom LoRAs), so they are a first-class input, not an advanced escape hatch:
- **Discovery (verified):** `/object_info/LoraLoaderModelOnly` returns `lora_name` as a combo whose options are the installed LoRA paths -- the same list as `/models/loras`. This is the authoritative list because it is what the graph will accept.
- **UI:** a LoRA stack panel in AudioStudio — up to `max_stack` entries, each a picker + strength slider, reorderable and individually bypassable. Hidden entirely when the profile has no `loras` block.
- **The picker is a real design problem, not a dropdown.** On the owner's install the raw list was 95 entries of which ~9 were usable (**re-read 2026-08-27: now 53 entries, ~10 usable, and the case-variant directory is no longer present** — MCP-SURFACE §16.5; the shape of the problem is unchanged): training-run epoch checkpoints dominate, `training_state.pt` files appear but are not loadable, one directory holds five different adapters, and case-variant duplicates appear on Windows. The picker must filter non-adapters, group by directory, collapse epoch series to `final`/latest behind an expander, dedupe case variants, and support user-assigned display names and favorites. Full evidence: docs/MCP-SURFACE.md §4.
- **Provenance:** the stack (file identity + strength + order) is recorded per track (§8). A LoRA-generated track that can't be reproduced from its sidecar is a bug, not a nuance.
- **Training is out of scope** — ComfyUI custom nodes already train ACE-Step LoRAs. This app consumes LoRAs, it does not train them.

### 5b. Custom workflow import (user profiles)
Users with a working ComfyUI workflow — including LoRA wiring no shipped profile covers — import it
rather than being forced onto our templates. Flow: in ComfyUI, **Workflow > Export (API)** downloads
a file -> pick it in Setup -> the app reads every node's **literal** inputs as candidate slots
(class, input name, current value; links are never candidates, which is what made the old "inert
seed" trap impossible) -> a mapping screen asks which input receives tags, lyrics, negative,
duration, seed, steps, CFG, with candidates pre-suggested -> numeric bounds and enum options are read
from `/object_info` -> saved as a user profile plus a copy of the workflow, indistinguishable from
shipped ones thereafter. This is the pressure-release valve that keeps the profile abstraction from
becoming a cage.

**Why Export (API) and not Save (owner decision 2026-11-11).** Export downloads a file the user can
find; a saved workflow lives several folders deep in ComfyUI's user directory, where most people
never look. And API format is what ComfyUI executes, so what the user mapped is exactly what runs.
A frontend-format file is refused with the menu path to the right export, not half-imported.

**Two rules carried over from the comfy-mcp era, both still load-bearing.** Emission detects the
output kind from the graph's save node (`SaveImage` -> `kind: image`), and the role suggester reads
**conditioning polarity** -- which encoder feeds a sampler's `positive` versus `negative` input --
rather than input names, because an image graph's two encoders are both `CLIPTextEncode` exposing
`text`, and a name-only match feeds the user's prompt into the negative side. In API format the
polarity is a direct read: follow the sampler's `positive` link to its source node.

**Readiness of an imported profile.** `comfy.models` is populated from the graph's loader inputs
(literal filenames), each resolved to its folder against the live `/models/<folder>` inventory, so
an imported profile whose files are installed reads Ready. `source_url`/`size_bytes` stay `None` --
the app knows where a file *is*, never claims it can fetch someone else's weights.

## 6. Lyric generation & prompt optimization

- **System prompt is assembled, not hardcoded**: role ("You are a professional songwriter/producer writing for {model.display_name}") + the target profile's `lyrics_contract` + `prompt_guide` + hard rules (output ONLY lyrics in the required format; obey structure tags; match requested language; no meta-commentary).
- **Brief form** (all prefilled with strong examples): theme/story, genre & style tags, mood, structure (e.g. V-C-V-C-B-C), language, point of view, era/references, explicit allowed y/n, target duration (constrains lyric length).
- Output lands in a versioned editor (each generation/edit = a `LyricDoc` version; cheap full-copy versions, no diffing engine). Approve → available in AudioStudio.
- **Prompt optimization (lyrics brief AND audio tags): opt-in per use, always consented.** The optimizer LLM call returns a rewritten prompt; UI shows original vs optimized side-by-side with an inline word-diff; user Accepts / Edits / Reverts. The *user-approved* text is what gets sent and what gets stored in provenance, flagged `optimized: true|false`. Never auto-apply.
- **What the lyric optimizer rewrites is the assembled user message** (T-210), not the lyrics and not the form fields: the labelled lines are the prompt, so they are what the user is asked to accept. The optimizer prompt declares Theme / Genre and style tags / Mood / Era and references rewritable and Structure / Language / Point of view / Explicit content allowed / Target duration fixed, and **nothing enforces that but the diff** -- a rewritten settings line arrives as a highlighted change the user must accept. `<PromptDiff>` is provider- and domain-agnostic (two texts, Accept / Edit / Revert) and is the component Phase 3's audio tags reuse.

## 7. Audio generation pipeline

1. AudioStudio renders controls from the selected profile's `inputs` (§5) — unsupported controls simply don't render (e.g. **no negative box for ACE-Step 1.5**, which has no such input).
1a. **Generate assembles the spec in the frontend, and two rules govern it** (T-309d).
`specInputs` tags each value from its control's declared *kind* (never from `typeof`), `specLoras`
carries the stack **including bypassed rows** so a bypass stays distinguishable from a delete,
and the `LyricRef` is attached **only when the submitted lyric text is byte-identical to the
approved version's** -- the spec carries text and ref side by side with nothing downstream
reconciling them, so a ref naming v2 beside v3's words would be a sidecar that lies. The
lyrics and seed controls are located by kind, not by the names `lyrics` and `seed`, so a
custom-imported workflow (5b) does not silently lose lyric provenance. Generate is **not**
gated on the Setup card having connected first: `generate_audio` resolves the ComfyUI address
itself, so such a gate would leave the button dead on every cold start.
2. On Generate: load the profile's API-format workflow into memory → audit every resolved address (§3a) → write the `GenerationSpec` values (profile id, all input values, LoRA stack, lyric doc version ref, seed) into their addresses → graph edits → `POST /prompt`. **Every job is its own in-memory copy**, and the exact graph submitted is written to `jobs/<id>/workflow.json` as the record of what ran.
   - **`GenerationSpec.title` (T-409) is the one field that drives no slot.** It is the song title the user named at generation; it is copied to `Track.title` at ingest and becomes the exported file's default name (sanitised). A **snapshot**, not a link to the source `LyricDoc` — retitling the doc later never retitles a track already made (ARCHITECTURE 8, the sidecar is the only truth). `None` is an untitled track, which the Library renders as the id. `#[serde(default)]`, so every pre-T-409 sidecar stays readable.
2a. ⚠ **A slot write is only real if the input holds a literal** (§3a). In the comfy-mcp era ACE-Step's `3.seed` and `94.seed` accepted writes the engine ignored -- every track would have used the template's seed while provenance recorded the user's (MCP-SURFACE §18.1). In API format they are visibly links from `PrimitiveInt` 109, the profile writes `109.value`, and `create-core::audit_slots` refuses any address that is missing or link-fed. Values reach the wire through `InputValue::to_slot_value`, never `serde_json::to_value`, which would send the adjacent tag.
3. **Graph edits** happen on that working copy before the run, for the two cases slots cannot express: splicing `LoraLoaderModelOnly` nodes after the profile's `attach_after` node, and making the save node write lossless. Shipping lossy MP3 into a mastering chain would undercut the whole suite.
   - **The test is the format value, not the node class** (verified 2026-08-27, MCP-SURFACE §16.3). ACE-Step's template ships `SaveAudioMP3`; MiniMax's ships `SaveAudioAdvanced` **already set to `mp3`/`V0`**. A check that only asks "is this the modern node" passes MiniMax and ships MP3 — the outcome this rule exists to prevent.
   - **`format` is a graph edit, not a slot** (COMFY-HTTP-SURFACE 4, 7): it is a `COMFY_DYNAMICCOMBO_V3`, and in API format its sub-inputs are dotted sibling keys (`"format.quality": "V0"`). Lossless is: set `format` to `"flac"` and **remove every `format.*` key** -- `flac` has no sub-inputs, and a stale `format.quality` beside it is a key the node does not declare. Verified live: the edited graph wrote a real FLAC.
   - **`flac` is the only lossless option** the node offers — there is no WAV — and it writes **16-bit/48 kHz with no bit-depth control**. The UI must not offer 24-bit.
   - `filename_prefix` on the swapped node is still a normal slot, so only the format needs the graph edit.
4. **ComfyUI validates on submit.** `POST /prompt` refuses an invalid graph with `400` and per-node errors (`value_not_in_list`, `value_smaller_than_min`, `missing_node_type`, ...) before any GPU time, and the app turns those into one sentence naming the node and input. Like comfy-mcp's validator before it, this is **blind to reachability** -- a LoRA loader spliced in but feeding nothing validates and runs with no LoRA applied (MCP-SURFACE §17.1) -- so reachability of every graph edit is asserted in `create-core`'s own tests, before submission.
5. Job lifecycle streamed to a **queue panel** (pending/running/failed with error text) by the Rust pump over `/queue` and `/history` (§3). Multiple queued jobs allowed; batch = N seeds of the same spec.
   - **Built in the frontend, not in Rust** (T-312). `state/generate.ts::specsFor` varies the panel's *values* and re-calls `specFor`, and the store awaits one `generate_audio` per spec, registering each prompt id as it lands. There is no batch command: each call already mints its own working copy, its own `PendingTrack` and its own pump, so N calls are N independent jobs, and sequential submission is what keeps `submittedAt` — the key the live queue sorts by — in execution order. `GenerationSpec::with_seed` exists in `create-core` and is **not** on this path.
   - ⚠ **The submitter must register the returned `prompt_id` with the frontend jobs store.** `generate_audio` starts the pump itself rather than going through `run_workflow`, and the store's reducer ignores events for ids it does not know -- correctly, or a foreign job would invent an entry. Both halves are right alone and deaf together: without `useJobsStore.register`, a generation runs to completion on the GPU and every progress, done and failed event is discarded, leaving an empty queue and no error anywhere (T-309d).
   - A known, accepted gap: the pump can emit before `register` runs, so an early status may be missed. Terminal events always arrive after, so nothing hangs. Closing it means a new `job://queued` event from the backend, for a cosmetic gain.
6. On completion: each output named in `/history` is downloaded over `/view` into the library (§8) with a **provenance sidecar** that includes the resolved slot values actually submitted, not just the UI values.

7. **Cover art rides this same pipeline** (T-506b). `generate_image` and `generate_audio` are two
   commands over one `build_and_submit`, which needs no change for an image profile: the lossless
   swap is a clean no-op at `prefer_lossless: false`, the LoRA splice is skipped for a profile with
   no `loras` block, and the slot writes land (verified live -- [docs/COMFY-HTTP-SURFACE.md](docs/COMFY-HTTP-SURFACE.md) section 6).
   Two things differ, and both are decided by the **pending record's `ModelKind`, never by a file
   extension**: outputs download into `tracks/` or `art/`, and ingest files a `Track` or an
   `Artwork`, emitting `track://saved` or `art://saved`. A profile queued by the wrong command is
   refused **before anything connects** -- otherwise it would run to completion, report Done, and
   file nothing, because the other kind's ingest skips what it does not recognise.

## 8. Library, provenance, storage

- App data dir, Tauri `app_config_dir()` under identifier **`com.latentbeats.create`**
  (`%APPDATA%\com.latentbeats.create\` on Windows). **Verified on disk 2026-08-29** -- there is
  no `library/` level, and the identifier is not `latentCreate`; both were wrong here for months,
  and every path below is now what `library::projects::project_dir` actually builds:

```
<app config dir>/
├── config.json                # non-secret config (secrets -> OS keychain)
├── session.log                # + session.log.1
├── jobs/<job-id>/workflow.json  # per-job working copy; the record of what was submitted
└── projects/<project-slug>/
    ├── project.json           # name, created, lyric doc ids, track ids, album lists
    ├── lyrics/<doc-id>.json   # one LyricDoc, every version inline (see below)
    ├── tracks/<track-id>.flac # (or wav/mp3 as produced)
    ├── tracks/<track-id>.json # SIDECAR = the whole Track record, incl. Provenance:
    │                          #   model profile+licence, template, the GenerationSpec
    │                          #   the user chose, the RESOLVED slot values actually
    │                          #   submitted, LoRA stack (file+strength+order), seed,
    │                          #   lyric version ref, optimized flags, comfy server
    │                          #   info, timestamps, duration
    ├── art/<art-id>.png     # cover art; ids are `ar-0001`, minted from
    └── art/<art-id>.json    #   `Project::next_art_seq` (T-506a)
                             # a track's chosen cover is an ArtId in its own
                             # sidecar; an album's is in project.json (T-506e-a)
```

**Export writes tags; the library's own file never carries them (T-516).** `export_track` copies
the audio and then tags **the copy** with title, artist, album, track number, year, a comment
naming the model and its licence, and the cover as an embedded `CoverFront` picture. The file under
`tracks/` is left byte-for-byte as generated, because it is the artifact the provenance sidecar
claims to describe -- tagging it would silently change the thing the recipe reproduces. The values
are decided by `create_core::export::tags_for` (pure, so every rule is tested without a FLAC) and
written by `lofty`; the album and track number come from the **first album list containing the
track**, in the project's order, since an export names a track and nothing in the request says which
release is meant. The artist is the only tag the app cannot infer and is the one thing configured,
in `config.export.artist`. **Tagging never fails an export**: a deleted cover, an unreadable image
or a format `lofty` will not write becomes a warning beside a file the user still gets.

**Send-to does not tag** (owner decision, 2026-09-06). `sendto.rs` hands a path to Latent Mixing /
Mastering, which are mid-flight; revisit after that work settles, in the version after this one.

  `config.json` sits **beside** `projects/`, not inside it. The `library` crate is the code that
  owns this tree; it is not a directory in it.

- JSON files, no database. Human-readable, git-able, trivially portable. Revisit only if scanning gets slow (thousands of tracks).
- **One file per lyric document** (2026-08-25, Phase 2 boundary): `lyrics/<doc-id>.json`
  holds the whole `LyricDoc` with every version inline, and `project.json` holds only the
  ordered doc ids. The earlier sketch here was `lyrics/<v>.md` per version, written before
  `LyricDoc` existed; it would put a version's text in one file and its `source`,
  `created_at` and `approved` flag in another -- the same two-files-disagreeing hazard the
  track rule below exists to prevent, for a few KB of saving.
- **One source of truth per track** (T-003b): `project.json` holds an *ordered list of track ids* and the album lists; everything about a track — title, file, duration, provenance — lives only in its sidecar. Duplicating a title into both files would guarantee the two drift apart on the first rename.
- **Artwork follows the track rule exactly** (T-506a): `project.json` holds an ordered list
  of `ArtId`s and nothing else; `art/<id>.json` is the whole `Artwork` record -- title, file,
  pixel size, and the same `Provenance` a track carries. `Provenance` is **reused, not forked**:
  it records a generated *asset*, and nothing in it is audio-specific. Both new `Project` fields
  (`art`, `next_art_seq`) are serde-defaulted, so every project written before cover art
  existed still loads.
- **A cover is a pointer, not provenance** (T-506e-a): `Track.cover` sits beside `title` in the
  track sidecar, and `AlbumList.cover` in `project.json` because an album has no file of its own.
  Neither goes inside `Provenance`, which records what *made* an asset and is never rewritten --
  a cover is something the user changes at will, and nothing about reproducing a track depends on
  it. That is also why `delete_art` **clears** cover references rather than refusing the delete,
  the opposite of `lyrics::delete_doc`: a `LyricRef` is part of a recipe that must stay
  reproducible, a cover is the user's current arrangement, and `delete_track` already clears the
  same way for album membership.
- **Provenance stores both levels**: the `GenerationSpec` the user chose (semantic, e.g. `duration_s = 120`) *and* the resolved slot values actually submitted (e.g. `94.duration = 120`, `98.seconds = 120`). The first powers "re-use these settings"; the second is what makes a track reproducible and is the only record of what the graph really received.
- Track actions: play, delete (to OS trash, not hard delete), rename, add-to-album-list, export/reveal, **Send to** mixer/mastering.
- **Send to**: v1 opens `https://app.latentmixer.com` / `https://app.latentmastering.com` in the browser and reveals the file for drag-in. The real handoff protocol is **owned by the mixing/mastering repos**; latentCreate adopts it when it exists rather than designing it (PROJECT.md decisions log, 2026-08-23). **It did not arrive during Phase 4** -- the phase-start check on 2026-08-30 found both siblings web-first with no landed protocol -- so the link-out plus reveal-file is what shipped, and it stays until they publish one.

## 9. Player & visualizer

- Playback in the webview: `<audio>`/Web Audio graph → `AnalyserNode` FFT → canvas spectrum + waveform. **Read-only visualizer, zero custom DSP** — `AnalyserNode` provides the data; we only draw. No Rust audio path, no realtime constraints.
- Visual language follows **`latentbeats.com`**, the umbrella brand (`../website/latentbeats.com/css/style.css`): deep blue-black ground `#0a0e1a`, accent `#58a6ff`, 12px radii, meters-as-decoration. The suite moved violet -> blue in Aug 2026; the site carries the newest tokens, the two sibling apps still carry the older GitHub-dark ground. latentCreate tracks the site (T-001) — it has no legacy screens to migrate, and the accent is identical either way. The sibling repos' viz code is closed-source; the owner may port pieces they own outright, but default is a clean-room reimplementation (it's ~200 lines against AnalyserNode) so this repo stays unencumbered. Anything ported must be listed in THIRD-PARTY-LICENSES bookkeeping.

## 10. Setup & configuration flow (first-run wizard, revisitable in Settings)

Step order, each with live health checks and "why/how" help text:
1. **ComfyUI** — find the user's ComfyUI (the address they set, else `127.0.0.1:8188`, then `:8000`; §3), and show the address, version and VRAM. An address field lets a user point at a ComfyUI on another port or another machine. Nothing to install: the only prerequisite is a running ComfyUI. When none answers, the card says so and what to do ("Start ComfyUI, then Retry"); the app does not try to start it.
2. **Models** (§10a) — the curated **installable list**: every shipped profile, **audio and image**, shows "Installed" ✅ / "Not installed" by matching its declared files against ComfyUI's own `/models/<folder>` lists, with one-click install (the app downloads into the folder ComfyUI reports, with progress; offered only for a ComfyUI on this machine). The step is split **Audio | Image** by profile kind. **It is also where the default model for each studio is chosen** (T-514): every row is radio-selectable and writes `default_profile_id` / `default_image_profile_id`. This is where a user brings a model in without hand-importing a workflow; anything off the curated list is imported via §5b, whose importer lives on this step too.
3. **Lyrics LLM (optional, skippable)** — provider pick, base URL/key, `list_models`, test call. Models the suite recommends for lyrics (docs/MODELS.md — currently Gemma 4 12B, and 26B/31B for high-VRAM users) get a "recommended for lyrics" chip in the list and the 12B is preselected when nothing is chosen; the recommendation list is read as data, and the app never auto-pulls an LLM.
4. **Cover art** — image models are installed on the Models step (2), not a separate step; the CoverArt view generates artwork over the chosen image profile (T-506).

Config edits never block the main UI after first run; degraded states show as status pills (e.g. "ComfyUI offline — reconnect") not modal walls.

### 10a. The model catalog — a curated installable list

**The reason the Setup page exists beyond LLM selection: a user brings new audio and image models into ComfyUI from the app.** There is no model hub to search, so the app can only install models whose URLs it **curates**; the catalog is the curated installable set, and nothing else (2026-09-05 pivot, PROJECT.md decisions log).

- **The catalog *is* the Setup Models step.** Every shipped profile (`profiles/*.json`, audio and image) is rendered there with a readiness pill and one-click Install, split `Audio | Image` by `profile.kind`. Readiness is exact filename membership in ComfyUI's `/models/<folder>` list. Install streams each file's hand-verified, **ungated** `source_url` into the directory ComfyUI itself reports for that folder (§3), so the file lands where ComfyUI reads and the row turns Installed without a restart. This replaced comfy-mcp's `download_model`, which wrote into comfy-cli's own workspace -- on a Comfy Desktop install, a different drive from the one ComfyUI read (T-519).
- **Curated set (T-511, T-513).** Four audio (ACE-Step 1.5 Turbo / XL Base / XL SFT, MiniMax Music 3) and five commercial-safe image models (Flux.1 Schnell FP8, Chroma1 HD, SDXL 1.0, SD 3.5 Large FP8, Qwen-Image). Each `ModelFileSpec` carries `file`/`folder`/`source_url`/`size_bytes`; the licence is shown per row (CONVENTIONS) and every curated model is commercial-use-safe. **Adding a model** is a new profile plus its converted workflow (§5's authoring rule).
- **Anything off the curated list** is brought in by **import-your-own-workflow** (§5b) -- an Export (API) file and a role-mapping screen.

This ships **no models** -- the app fetches only at the user's click, from URLs it curated, into the user's own ComfyUI. The paid hosted/cloud tier remains a documented later addition (backlog, ROADMAP). **History:** T-504/T-505 built a gallery-browse-and-adopt catalog; **T-512 (2026-09-05) removed it**.

## 11. Frontend rules

- Zustand stores per domain; components never call `invoke` directly — only through `app/src/bridge/` typed wrappers.
- Single `theme.css`; every className styled there; dark professional theme; **no UI framework**. Spacious layout — one primary action per view, progressive disclosure for advanced params.
- Views: `Setup`, `LyricsStudio`, `AudioStudio`, `Library`, `CoverArt` behind a left nav rail. Lyric→Audio handoff is a store action, not navigation state.
- **Choosing vs swapping (T-514).** Setup is where a model or project is **chosen, installed and read about**; every other screen only **swaps** between what already exists. Setup renders the one shared `.picker-list` / `.picker-row` (radio, readiness pill, licence, and — via `ProfilePickerRow`'s `children` — the file list, download progress and Install). Audio, Cover Art, Lyrics and Library render `QuickSwap`, a **name-only** `<select>` over what is **installed**; licence, VRAM claim and readiness are deliberately not repeated there. Both write the **same** config field, so no two screens can disagree about the selection. Rules live in `state/` (`installedOptions`/`optionsNote`, `lyricsOptions`, `nowPlayingLabel`) because vitest runs in `node` with no DOM — a view that derives its own is untestable by the whole gate.
- **Anything the webview loads must be named in `app.security.csp`** (`src-tauri/tauri.conf.json`).
  A generated file reaches the webview as a `convertFileSrc` asset URL -- `asset:` on macOS and
  Linux, `http://asset.localhost` on Windows -- and every element type needs its own directive:
  `media-src` for the `<audio>` element, `img-src` for `<img>`. **A dev run never applies the CSP at
  all**: Tauri injects it only into responses its own protocol serves, and `npm run dev` loads the
  frontend from Vite's dev server instead. A missing directive therefore passes the gate, the tests
  and every click-through, and fails for the first time in a packaged build -- which is exactly how
  T-517's missing `img-src` shipped in v0.1.0. `src-tauri`'s
  `test_csp_covers_every_asset_protocol_directive` is the only guard there can be; extend it the
  next time a new kind of element starts loading through the asset protocol.

## 12. Non-goals (v1)

- No model inference in-app, ever. No bundled models or weights.
- No mixing/mastering DSP — that's the other apps.
- No mobile/web build. Desktop only (Windows first, then macOS/Linux CI builds).
- No workflow *editor* — ComfyUI is the editor; we surface parameters, not graphs.
- No account system, telemetry, or server component.
