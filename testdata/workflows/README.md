# testdata/workflows — frozen ComfyUI workflows for offline tests

CI never has a running ComfyUI and must never reach the template gallery
(WORKFLOW.md §5). Anything that parses, edits or reasons about workflow JSON therefore
needs a real graph checked in. These are those graphs.

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

### Staleness
This is a **snapshot**. The gallery updates and comfy-cli caches templates with a 24-hour
TTL, so this file will drift from upstream. That is fine — its job is to be a stable input
for parser and graph-edit tests, not to mirror the current gallery. When a test needs
current gallery content, it is a live producer-run check, not a CI test.

### Not a fixture
Three `COMFY_MATCHTYPE_V3` warnings appear on validation (`ComfySwitchNode`'s wildcard type
against `AUDIO`). They are present in the official template too and do not block validation.
No generation has ever been run from this file.
