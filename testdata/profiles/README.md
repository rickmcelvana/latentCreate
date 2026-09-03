# testdata/profiles — frozen model profiles for offline tests

Same rule as `testdata/workflows/`: CI has no ComfyUI and must never reach the gallery
(WORKFLOW.md §5), so anything that reasons about a profile needs a real one checked in.

## `ace-step-flat-inputs.json`

Not a profile — the **flattened input-name contract** for `ace-step-1.5-turbo`, pinned from both
sides of the language boundary (a Rust test asserts `flat_inputs()` produces exactly this list, a
frontend test asserts `panelModel()` produces it minus the unsupported entries). Its own `_why`
key carries the story: the two sides disagreed for four tasks and each half's tests passed
throughout.

## `flux2-klein-9b-image.json`

The profile the app itself emitted on 2026-09-03 when Flux.2 Klein 9B was adopted from the model
catalog (T-505d-d), copied out of the user's app data **verbatim** with exactly one rewrite:

| Field | As emitted | Here |
|---|---|---|
| `comfy.workflow` | `%APPDATA%\com.latentbeats.create\workflows\latentcreate-adopt-image-flux2-text-to-image-9b.json` | `testdata/workflows/flux2_klein_9b.json` |

The rewrite is so the fixture names nothing on one machine; both paths hold the same graph, and
the repo's copy is the frozen capture documented in `testdata/workflows/README.md`.

### Why this one is worth freezing

It is the only **image** profile in the repo, and the only one that is workflow-backed rather than
template-backed. What it pins:

- `kind: image` and `comfy.output: { save_node: "SaveImage", prefer_lossless: false }` — the
  emission T-505d-b generalised, and the reason the pipeline's lossless swap is a clean no-op for
  images.
- `tags -> 75/74.text` and `negative -> 75/67.text` — the conditioning polarity T-505d-c reads
  from the graph, recorded here as the outcome a regression would change.
- `comfy.template: null` with `comfy.workflow` set — an adopted profile is **copied, not
  referenced** (ARCHITECTURE §5b), which is the branch of `place_working_copy` no shipped profile
  exercises.
- `comfy.models: []` — every imported or adopted profile declares no model files, so its readiness
  is `Undeclared`. Presentation gap, tracked in T-507; nothing gates on it.
- **No size input.** The role table has no width/height role, and on this graph every size-shaped
  slot is inert anyway (docs/MCP-SURFACE.md §35.2).

### Regenerating it
Adopt `image_flux2_text_to_image_9b` from the Setup model catalog's Image tab, then copy the
emitted profile out of the app data dir and rewrite `comfy.workflow` as above.
