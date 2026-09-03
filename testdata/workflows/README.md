# testdata/workflows — frozen ComfyUI workflows for offline tests

CI never has a running ComfyUI and must never reach the template gallery
(WORKFLOW.md §5). Anything that parses, edits or reasons about workflow JSON therefore
needs a real graph checked in. These are those graphs.

## `ace_step_1_5_xl_turbo.json`

The `audio_ace_step1_5_xl_turbo` gallery template, **unmodified**, fetched 2026-08-27 against
ComfyUI v0.34.1. `local_check` reported `runnable: true` with zero errors.

### Why this one is worth freezing
It is the graph the pipeline's two hard edits are written against:

- **It ends in `SaveAudioMP3`** at node `107`, widgets `["audio/ACE_Step1.5_xl_turbo", "V0"]`
  -- the deprecated lossy node the save swap replaces (MCP-SURFACE 5, 16.1). Pair it with
  `minimax_music3_int8.json`, which ships the *modern* `SaveAudioAdvanced` **already set to
  `mp3`**: between them they cover both halves of the rule that the test is the **format
  value, not the node class** (MCP-SURFACE 16.3). A graph-edit test that uses only this file
  will pass while the app ships MP3 for MiniMax.
- **It contains no LoRA loader**, so applying one means *inserting* a node, not setting a
  value. The MODEL chain is `104 (UNETLoader) --260--> 78 (ModelSamplingAuraFlow) --175--> 3
  (KSampler)`, and the profile's `loras.attach_after` is `"104"`, so the splice goes between
  104 and 78 and must rewire link `260`. Links are
  `[link_id, src_node, src_slot, dst_node, dst_slot, type]`; `last_node_id` is 110 and
  `last_link_id` is 265, which is where fresh ids come from.
- Its slots are the **flat `A.name`** form, against MiniMax's `A/B.name` subgraph form. Both
  address styles are represented in `testdata/` on purpose.

### Regenerating it
```
fetch_template("audio_ace_step1_5_xl_turbo", out_path)   # no edits; expect runnable: true
```

## `minimax_music3_int8.json`

The `audio_minimax_music_3` gallery template (dated 2026-08-13), with **one** value changed:

| Address | Official | Here |
|---|---|---|
| `37/6.unet_name` | `minimax_music3_dit_fp16.safetensors` | `minimax_music3_dit_int8_convrot.safetensors` |

Provenance was checked, not assumed: a structural diff against a freshly fetched copy of the
template reported **exactly one difference**, the line above. `validate_workflow` returns
`valid: true` with zero errors against a real install holding the int8 weights.

### Why this one is worth freezing
It is the only workflow in the repo that uses **subgraphs**. Every slot inside it is
addressed `A/B.name` (`37/6.unet_name`, `37/13.caption`), where the ACE-Step template uses
plain `A.name`. Address parsing that handles only the flat form will pass every other test
and then fail on real user workflows — this fixture is what catches that offline.

It also carries the awkward shape worth testing against: **three independent seeds**
(`37/13.seed`, `37/9.seed`, `37/38.seed`) and **two duration fields that ship disagreeing**
(`37/13.max_duration` = 60, `37/15.seconds` = 120).

### Regenerating it
```
fetch_template("audio_minimax_music_3", out_path)
set_workflow_slot(out_path, [{"address": "37/6.unet_name",
                              "value": "minimax_music3_dit_int8_convrot.safetensors"}],
                  stdout=False)
validate_workflow(out_path)   # expect valid: true
```

## `flux2_klein_9b.json`

The `image_flux2_text_to_image_9b` gallery template, **unmodified**, fetched 2026-09-03 against
ComfyUI v0.34.3 / comfy-cli 1.16.0. `local_check` reported `runnable: true` with zero errors.
Its slot capture is `testdata/mcp/list_workflow_slots.flux2-klein-9b.json` (20 slots).

### Why this one is worth freezing
It is the **first image graph** in `testdata/`, and the only one carrying the shape that broke
role suggestion:

- **Two `CLIPTextEncode` nodes whose inputs are both named `text`, both `STRING`.**
  `75/74.text` feeds `CFGGuider.positive` (link 140) and `75/67.text` feeds `CFGGuider.negative`
  (link 141). Nothing in the slot list distinguishes them -- same input name, same widget type,
  same node class -- so a name-matching suggester offers **both** as `Tags` and, because both
  rank `Strong`, the import screen **pre-ticks both** and the emitted profile writes the user's
  prompt into the negative conditioning as well. This is the file that proves the polarity rule
  (T-505d-c) and the regression that would undo it.
- **`75/74.text` is boundary-fed, not inert.** Its link (162) originates at the subgraph's
  `inputNode` (`-10`), and the host node `75` holds the prompt in `widgets_values[0]` with
  `inputs[0].link = null` -- the MiniMax `37/38.seed` pattern. So it is a real write target and
  must stay offered; the fix must reclassify it, never drop it.
- **`SaveImage` sits at the top level** (node `9`) while every control lives one subgraph deep,
  the mirror of MiniMax's layout. `emit::detect_output_kind` (T-505d-b) reads it there.
- It has **no negative-named slot at all**, so `Role::Negative`'s name table finds nothing --
  the negative prompt is reachable only through graph polarity.

### Regenerating it
```
fetch_template("image_flux2_text_to_image_9b", out_path)   # expect runnable: true
list_workflow_slots(out_path)                              # 20 slots, the mcp/ fixture
```
Requires `flux-2-klein-base-9b-fp8.safetensors`, `qwen_3_8b_fp8mixed.safetensors` and
`full_encoder_small_decoder.safetensors` installed for the `runnable` check; parsing needs none
of them.

### Staleness
All three files here are **snapshots**. The gallery updates and comfy-cli caches templates with a 24-hour
TTL, so this file will drift from upstream. That is fine — its job is to be a stable input
for parser and graph-edit tests, not to mirror the current gallery. When a test needs
current gallery content, it is a live producer-run check, not a CI test.

**Checked 2026-08-27:** a structural diff of `minimax_music3_int8.json` against a freshly
fetched template found **619 leaf values on both sides and exactly one difference** -- the
documented `unet_name` override. Four days on, upstream has not moved. Worth re-running that
diff rather than assuming, in either direction.

### Not a fixture
Three `COMFY_MATCHTYPE_V3` warnings appear on validation (`ComfySwitchNode`'s wildcard type
against `AUDIO`). They are present in the official template too and do not block validation.
No generation has ever been run from this file.
