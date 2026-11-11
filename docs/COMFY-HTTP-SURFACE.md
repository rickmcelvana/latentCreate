# COMFY-HTTP-SURFACE.md -- ComfyUI's own HTTP API, verified live

**Authoritative for `comfy-bridge`** (T-519). Every fact here was observed against a running
ComfyUI, not read from documentation. Fixtures captured from the same runs live in
`testdata/comfy-http/`. [MCP-SURFACE.md](MCP-SURFACE.md) is now a **historical record** of the
comfy-mcp era: its findings about *ComfyUI itself* (model behaviour, ACE-Step non-reproducibility,
what `/history` shows) still hold; its findings about comfy-mcp/comfy-cli tools no longer apply.

Verified 2026-11-11 against **ComfyUI 0.45.2**, Comfy Desktop install on Windows 11
(`G:\Comfy-Desktop`, shared models root), RTX 5060 Ti 16 GB, listening on `0.0.0.0:8188`.

## 1. Why the app stopped using comfy-mcp

Measured the same day (PROJECT.md session log 2026-11-11, T-519):

1. **comfy-mcp runs every tool through comfy-cli** (`comfy` on PATH or `COMFY_BIN`) and does not
   declare it as a dependency. On a machine without comfy-cli every tool fails with
   `comfy not found on PATH`; the app rendered that as "Unreachable" with no next step.
2. **comfy-cli resolves its own "workspace"**, independent of the ComfyUI that is actually running.
   With Comfy Desktop installed it reported workspace `~/Documents/comfy/ComfyUI` (which did not
   exist, `workspace_not_found`), so `download_model` wrote model files into
   `C:\Users\<user>\Documents\comfy\ComfyUI\models` while ComfyUI read `G:\...\ComfyUI-Shared\models`.
   The owner observed the consequence: the download finished, the row stayed "Not installed", and no
   Retry, reload or relaunch could ever fix it.
3. A user needed Python + `pip install comfy-mcp` + `pip install comfy-cli` + a correctly resolved
   workspace before the first generation. ComfyUI's own API needs none of them.

## 2. Reachability

- `GET /system_stats` -> `200` JSON. Use it as the health probe.
  - `system.comfyui_version` (`"0.45.2"`), `system.os`, `system.python_version`, `system.argv`.
  - `devices[]`: `name`, `type` (`"cuda"`), `vram_total` / `vram_free` in **bytes**.
  - `argv` carries absolute paths into the user's profile -- never copy it into a sidecar or log.
- Default port is **8188** (Desktop on this machine, portable, `python main.py`). Nothing answered
  on 8000 or 8189 here. The app probes a configured URL, else `127.0.0.1:8188`, else `:8000`
  (older Desktop builds' default -- not observed live, kept as a fallback probe only).
- `GET /` serves the web UI (`text/html`), so "something answered" is not "ComfyUI answered";
  only a decodable `/system_stats` counts.

## 3. Models

- `GET /models` -> folder names (`["checkpoints","configs","loras","vae","text_encoders",
  "diffusion_models", ...]`). Note: there is **no `unet` or `clip` key** -- those legacy names are
  folded into `diffusion_models` and `text_encoders`.
- `GET /models/<folder>` -> filenames as ComfyUI would accept them in a loader's combo (relative to
  the folder root; subdirectories included). This is the readiness inventory.
  - **The list refreshes live.** A file created in `ComfyUI-Shared\models\loras` appeared in the
    very next `GET /models/loras`, and disappeared when removed -- no restart, no rescan call.
- `GET /internal/folder_paths` -> `{folder: [absolute dir, ...]}` for every folder, in ComfyUI's
  search order. **This is the download destination.** On Desktop, for example:
  - `checkpoints`: `Shared\models\checkpoints`, `Installs\...\models\checkpoints`,
    `Shared\output\checkpoints`
  - `diffusion_models`: `Shared\models\unet`, `Shared\models\diffusion_models`, ...
  - `text_encoders`: `Shared\models\clip`, `Shared\models\text_encoders`, ...
  - Rule the app uses: the **first** directory whose last component equals the folder name
    (so `diffusion_models` lands in `...\diffusion_models`, not the legacy `...\unet`), else the
    first directory. `/internal/` is ComfyUI's label for "not a stable public API"; a missing or
    changed route must degrade to "Install unavailable -- download the file into your ComfyUI
    models folder", never to a guessed path.
- Downloads are therefore only possible when the paths ComfyUI reports exist **on this machine**
  -- i.e. a local ComfyUI. A remote ComfyUI gets readiness but no Install button.

## 4. Node registry

- `GET /object_info/<class>` -> `{class: schema}`; `GET /object_info` is everything (~2.2 MB,
  150 ms locally -- fetch per class, not wholesale).
- Input specs live under `input.required` / `input.optional`, each `[type, options?]`:
  - numbers: `["INT", {"default", "min", "max", "step"}]`, `["FLOAT", {...}]`. KSampler's seed max
    is `18446744073709551615` -- the u64 rule (ARCHITECTURE 5) still applies.
  - **combos come in two shapes**: legacy `[["a","b"], {...}]` (KSampler `sampler_name`,
    `LoraLoaderModelOnly.lora_name`) and `["COMBO", {"options": [...]}]`. Read both.
  - **`COMFY_DYNAMICCOMBO_V3`** (SaveAudioAdvanced `format`): `options` is a list of
    `{key, inputs}` -- `flac` has no sub-inputs, `mp3` and `opus` each carry a `quality` combo.

## 5. Workflows: the API format

- **Export (API)** in ComfyUI, or the frontend's `app.graphToPrompt().output`, produces
  `{node_id: {class_type, inputs, _meta: {title}}}`. An input is either a **literal** or a
  **link** `[source_node_id, output_index]`.
- **Frontend-only nodes vanish** (`PrimitiveNode`, `Reroute`, notes): their values are inlined as
  literals into every input they fed. Backend primitives (`PrimitiveInt`, `PrimitiveFloat`,
  `PrimitiveBoolean`) **survive** as real nodes. So in API format "is this write effective?" has a
  one-line answer: **the input exists and holds a literal**. A link means another node supplies the
  value and a write would be wrong (the 18.1 class of bug cannot hide here).
- **Subgraphs are flattened** with ids `"<parent>:<child>"` (Qwen-Image: `76:3` KSampler,
  `76:6` positive `CLIPTextEncode`, `76:58` `EmptySD3LatentImage`). MiniMax's former `37/13.caption`
  is `37:13.caption`.
- **Dynamic-combo sub-inputs are dotted keys**: SaveAudioAdvanced carries
  `"format": "mp3", "format.quality": "V0"`. Node ids never contain `.`, input names can, so an
  address splits at its **first** `.`.
- All nine curated templates were converted through the live frontend and every profile address
  was checked: **eight profiles map unchanged** (every address is a literal), and Qwen-Image's four
  subgraph-boundary addresses remap to `76:6.text`, `76:3.seed`, `76:58.width`, `76:58.height`.

## 6. Jobs

- `POST /prompt` `{"prompt": <api graph>, "client_id": "<uuid>"}`:
  - `200` `{"prompt_id", "number", "node_errors": {}}` -- queued.
  - `400` on validation failure, **before any GPU time**:
    `{"error": {"type": "prompt_outputs_failed_validation", ...}, "node_errors": {"<id>": {"errors":
    [{"type": "value_smaller_than_min" | "value_not_in_list" | ..., "message", "details",
    "extra_info": {"input_name", "received_value"}}], "class_type"}}}`.
  - `400` `{"error": {"type": "missing_node_type", "message": "Node 'KSampler' not found. The
    custom node may not be installed.", "extra_info": {"node_id", "class_type", "node_title"}},
    "node_errors": {}}` for an unknown class.
- `GET /queue` -> `{"queue_running": [[number, prompt_id, graph, extra, outputs], ...],
  "queue_pending": [...]}`. Index 1 is the prompt id.
- `GET /history/<prompt_id>` -> `{}` until finished (also for an unknown id), then
  `{prompt_id: {prompt, outputs, status, meta}}`.
  - success: `status = {"status_str": "success", "completed": true, "messages": [...,
    ["execution_success", {...}]]}`.
  - node failure: `status_str: "error"`, a `["execution_error", {"node_id", "node_type",
    "exception_message", "exception_type", "traceback", ...}]` message. **Never surface
    `traceback` or `current_inputs`** -- absolute paths and object reprs.
  - interrupted: `status_str: "error"` with `["execution_interrupted", {"node_id", "node_type"}]`.
- Observed ordering: a job left `queue_running` and appeared in history **in the same poll**;
  there is no observed gap, but the pump tolerates one (three consecutive misses before "lost").
- Cancel:
  - running -> `POST /interrupt {"prompt_id"}` -> `200`, empty body; history then shows
    `execution_interrupted`.
  - pending -> `POST /queue {"delete": [prompt_id]}` -> `200`, empty body; the job **never
    reaches history**, so the app must remember that it asked.
  - A job absent from both queue and history that was *not* cancelled means ComfyUI restarted or
    the queue was cleared elsewhere.

## 7. Outputs

- `history.outputs = {"<save node id>": {"images": [{"filename", "subfolder", "type": "output",
  ...}]}}`; audio save nodes report under `"audio"` (verified: ACE-Step XL Turbo, 10 s, written
  as `ace_00001.flac` after the lossless edit below). `type: "temp"` entries are previews -- skip.
- **The lossless edit in API format is two key operations** on the save node: set `format` to
  `"flac"` and remove every `format.*` key. Verified live: the job succeeded and `/view` returned a
  file starting `fLaC` (602 253 bytes for 10 s).
- `GET /view?filename=&subfolder=&type=output` returns the bytes (PNG verified, 405 428 bytes for
  512x512). Downloading over HTTP rather than copying a path is what makes a remote ComfyUI work
  for ingest at all.
- The probe runs wrote into `<output>/latentcreate_probe/`; the app's own jobs keep the profile's
  `filename_prefix`.
