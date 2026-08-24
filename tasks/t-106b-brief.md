# T-106b: `minimax-music-3` profile
**Depends:** T-106 | **Crate/dir:** `crates/create-core/` + `profiles/` | **Executor:** Aider

**Files to create:** `profiles/minimax-music-3.json`

**Files to modify:** `crates/create-core/src/profile.rs`

> The second seed profile. MiniMax Music 3 is the flagship-quality model (5-minute songs,
> sung vocals), and the first profile to exercise **subgraph slot addressing** (`37/...`),
> a **`caption`** input instead of `tags`, **three seeds + two duration fields** to fan out,
> and a **checkpoint-variant override** (the template hardcodes the fp16 DiT; the profile
> pins the int8 file). It also proves the save-node swap is conditional, not universal.

## Goal
A `minimax-music-3` profile JSON that loads under the existing `ModelProfile` schema, plus
one schema addition — `ComfySpec.slot_overrides` — so a profile can pin a slot value (the
checkpoint variant) that the template gets wrong. Tests assert the subgraph fan-out, the
override, the conditional save-node, and the conditional license.

## Verified, not recalled
All slot addresses and node facts captured live 2026-08-23/24 — **docs/MCP-SURFACE.md §6/§6a**
and the frozen `testdata/mcp/list_workflow_slots.minimax.json`. The reference code compiles,
is `cargo fmt`- and `clippy -D warnings`-clean, and all 24 scratch tests pass (5 new).

Facts the profile encodes:
- **`caption`, not `tags`** — `MiniMaxMusic3TextEncode` takes `caption` + `lyrics` (MCP-SURFACE §6a).
- **Three seeds** (`37/13.seed` text-encode, `37/9.seed` sampler, `37/38.seed` `SeedNode`) and
  **two duration fields that ship disagreeing** (`37/13.max_duration` = 60, `37/15.seconds` = 120)
  — one `seed` control and one `duration_s` control, fanned out.
- **The template hardcodes the fp16 DiT** (`minimax_music3_dit_fp16.safetensors`) but the int8
  file is what's installed; overriding `37/6.unet_name` makes `validate_workflow` clean (§6).
- **Already lossless** — the template ends in `SaveAudioAdvanced`, so the profile declares it and
  the pipeline's save-node swap stays conditional (§6a).
- **Conditional license** — "MiniMax-Music3 Community License": commercial use needs prominent
  attribution; >US$20M yearly revenue needs prior written authorization. Surfaced in
  `license`/`license_notes` (CONVENTIONS: per-model license shown wherever chosen).

## Reference code

### `profiles/minimax-music-3.json` — full file
```json
{
  "id": "minimax-music-3",
  "display_name": "MiniMax Music 3",
  "kind": "music",
  "license": "MiniMax-Music3 Community License",
  "license_notes": "Open weights, not OSI-open. Commercial use requires prominent 'MiniMax-Music3' attribution in the product UI; aggregate yearly revenue above US$20M needs prior written authorization from MiniMax.",
  "comfy": {
    "template": "audio_minimax_music_3",
    "workflow": null,
    "vram_gb_min": 16,
    "output": { "save_node": "SaveAudioAdvanced", "prefer_lossless": true },
    "slot_overrides": {
      "37/6.unet_name": { "type": "enum", "value": "minimax_music3_dit_int8_convrot.safetensors" }
    }
  },
  "inputs": {
    "caption": {
      "type": "text",
      "slots": ["37/13.caption"],
      "label": "Caption"
    },
    "lyrics": {
      "type": "lyrics",
      "slots": ["37/13.lyrics"],
      "structure_tags": ["[intro]", "[verse]", "[chorus]", "[bridge]", "[outro]"],
      "label": "Lyrics"
    },
    "negative": {
      "type": "unsupported",
      "reason": "MiniMaxMusic3TextEncode exposes no negative input."
    },
    "duration_s": {
      "type": "float",
      "slots": ["37/13.max_duration", "37/15.seconds"],
      "min": 10.0,
      "max": 300.0,
      "default": 120.0,
      "step": 1.0,
      "label": "Duration (s)"
    },
    "seed": { "type": "seed", "slots": ["37/13.seed", "37/9.seed", "37/38.seed"] }
  },
  "lyrics_contract": {
    "format": "structure-tagged",
    "notes": "Caption is a structured music description (Global Metadata -> Vocal Details -> Arrangement); lyrics carry [intro]/[verse]/[chorus]/[bridge]/[outro] tags."
  },
  "prompt_guide": {
    "tag_style": "structured caption: Global Metadata -> Vocal Details -> Arrangement",
    "examples": [
      {
        "tags": "Genre: synthwave. Mood: dreamy, nostalgic. Vocals: female, breathy. Arrangement: driving beat, 105 bpm, synth arpeggios",
        "lyrics": "[intro]\n[verse]\nNeon on the dashboard, midnight in the rain\n[chorus]\nDrive until the morning takes the weight away"
      }
    ]
  }
}
```

### `crates/create-core/src/profile.rs` — three edits

**Edit 1 — import `InputValue`** (top of file, after the module doc):
```rust
use crate::generation::InputValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
```

**Edit 2 — add `slot_overrides` to `ComfySpec`** (between `vram_gb_min` and `output`):
```rust
    /// Slot values pinned by the profile, applied to the fetched template before
    /// the user's inputs. This is how a profile targets a specific checkpoint
    /// variant: MiniMax Music 3's template hardcodes the fp16 DiT, so the profile
    /// overrides `37/6.unet_name` to the int8 file (MCP-SURFACE §6).
    #[serde(default)]
    pub slot_overrides: BTreeMap<SlotAddress, InputValue>,
```

**Edit 3 — add the `MINIMAX_FIXTURE` const and five tests** (in `mod tests`):
```rust
    const MINIMAX_FIXTURE: &str = include_str!("../../../profiles/minimax-music-3.json");
```
```rust
    #[test]
    fn test_profile_roundtrip_minimax_fixture() {
        let first: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
        let json = serde_json::to_string_pretty(&first).unwrap();
        let second: ModelProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(first, second);
    }
```
```rust
    /// Protects: the MiniMax profile's subgraph slot addresses parse and fan out
    /// correctly -- three seeds, two duration fields, and a `caption` (not `tags`)
    /// input. This is the first profile exercising `A/B.name` addressing.
    #[test]
    fn test_minimax_fixture_uses_subgraph_addresses_and_fan_out() {
        let profile: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();

        let caption = profile.inputs.get("caption").expect("caption input");
        match caption {
            InputSpec::Text { slots, .. } => {
                assert_eq!(slots, &[SlotAddress("37/13.caption".to_string())]);
            }
            other => panic!("caption was not Text: {:?}", other),
        }

        let duration = profile.inputs.get("duration_s").expect("duration_s input");
        match duration {
            InputSpec::Float { slots, .. } => {
                assert_eq!(slots.len(), 2);
                assert!(slots.contains(&SlotAddress("37/13.max_duration".to_string())));
                assert!(slots.contains(&SlotAddress("37/15.seconds".to_string())));
            }
            other => panic!("duration_s was not Float: {:?}", other),
        }

        let seed = profile.inputs.get("seed").expect("seed input");
        match seed {
            InputSpec::Seed { slots } => {
                assert_eq!(slots.len(), 3);
                assert!(slots.contains(&SlotAddress("37/13.seed".to_string())));
                assert!(slots.contains(&SlotAddress("37/9.seed".to_string())));
                assert!(slots.contains(&SlotAddress("37/38.seed".to_string())));
            }
            other => panic!("seed was not Seed: {:?}", other),
        }
    }
```
```rust
    /// Protects: the checkpoint-variant override -- the profile pins the int8 DiT
    /// over the template's hardcoded fp16 filename (MCP-SURFACE §6).
    #[test]
    fn test_minimax_fixture_pins_the_int8_checkpoint() {
        let profile: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
        let overrides = &profile.comfy.slot_overrides;
        assert_eq!(
            overrides.get(&SlotAddress("37/6.unet_name".to_string())),
            Some(&InputValue::Enum(
                "minimax_music3_dit_int8_convrot.safetensors".to_string()
            ))
        );
    }
```
```rust
    /// Protects: the save-node swap is conditional -- MiniMax's template already
    /// uses `SaveAudioAdvanced`, so the profile declares it (not a deprecated
    /// node) and the pipeline must not intervene (MCP-SURFACE §6a).
    #[test]
    fn test_minimax_fixture_declares_lossless_output() {
        let profile: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
        assert_eq!(profile.comfy.output.save_node, "SaveAudioAdvanced");
        assert!(profile.comfy.output.prefer_lossless);
    }
```
```rust
    /// Protects: the license is surfaced as open-with-conditions, not OSI-open --
    /// the UI must show it wherever the model is chosen (CONVENTIONS).
    #[test]
    fn test_minimax_fixture_surfaces_the_conditional_license() {
        let profile: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
        assert!(profile.license.contains("Community License"));
        let notes = profile.license_notes.as_ref().expect("license notes");
        assert!(notes.contains("attribution"));
        assert!(notes.contains("20"));
    }
```

## Tests
Five new tests in `profile.rs`. Per test, the invariant:

- `test_profile_roundtrip_minimax_fixture` — **protects:** the fixture deserialises and
  re-serialises to itself (the CONVENTIONS boundary round-trip rule).
- `test_minimax_fixture_uses_subgraph_addresses_and_fan_out` — **protects:** subgraph
  `A/B.name` addresses parse, and the fan-out is right — `caption` (not `tags`), two duration
  slots, three seed slots.
- `test_minimax_fixture_pins_the_int8_checkpoint` — **protects:** the `slot_overrides` field
  carries the int8 DiT override, typed `Enum` (a COMBO value).
- `test_minimax_fixture_declares_lossless_output` — **protects:** the save-node is
  `SaveAudioAdvanced` (already lossless), so the pipeline's swap stays conditional.
- `test_minimax_fixture_surfaces_the_conditional_license` — **protects:** the license is
  open-with-conditions and the notes name the attribution + revenue obligations.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] `cargo clippy -p create-core --all-targets -- -D warnings` clean
- [ ] All five named tests present and passing; the pre-existing 19 tests still pass
- [ ] No changes outside the two listed files
- [ ] No new dependencies

## Out of scope
The `slot_overrides` *consumer* — applying these overrides in the §7 pipeline is T-107's
profile loader / T-301's pipeline. This task only adds the schema field and the profile data.
The other seed profiles (stable-audio-open, musicgen, yue, diffrhythm, cover-art). Any
frontend or Tauri command. The LoRA picker (Phase 3).

## Notes for the executor
- `slot_overrides` is `BTreeMap<SlotAddress, InputValue>` — `InputValue` is the tagged enum
  from `generation.rs` (`{ "type": "enum", "value": "..." }`), so the override value is typed,
  not a bare string. `unet_name` is a COMBO, hence `Enum`.
- `InputValue` lives in `crate::generation`; the import is `use crate::generation::InputValue;`.
- The `MINIMAX_FIXTURE` const goes next to `ACE_STEP_FIXTURE`; the five tests go at the end of
  `mod tests`, after `test_group_members_parse_nested_specs`.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
`generation.rs` is `--read`: the new code constructs `InputValue::Enum` and the `ComfySpec`
field is typed `BTreeMap<SlotAddress, InputValue>`.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/create-core/src/generation.rs --file crates/create-core/src/profile.rs --file profiles/minimax-music-3.json
```
