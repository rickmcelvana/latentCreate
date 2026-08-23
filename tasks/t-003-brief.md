# T-003: create-core profile schema
**Depends:** T-001 | **Crate:** `crates/create-core` | **Executor:** Aider

**Files to create/modify:**
`crates/create-core/Cargo.toml` (add deps),
`crates/create-core/src/lib.rs` (modify),
`crates/create-core/src/profile.rs` (new),
`profiles/ace-step-1.5-turbo.json` (new)

> **Scope note.** The original T-003 covered every domain type. It is split: this task
> is the **model profile schema only**. `Project`, `LyricDoc`, `Track`, `GenerationSpec`
> and `Provenance` are T-003b, briefed separately, so each run stays reviewable.

## Goal
Serde types for the model profile (ARCHITECTURE.md §5), plus the real ACE-Step 1.5 XL
Turbo profile as a fixture, with round-trip tests. This crate is pure data: **no I/O, no
async, no file loading** — that is `library`'s job in Phase 1.

## Dependencies to add
In `crates/create-core/Cargo.toml`, exactly these and nothing else:
```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }

[dev-dependencies]
serde_json = "1.0"
```

## Spec — reference implementation
Integrate this essentially verbatim into `profile.rs`, adapting only doc wording. Every
public item needs a `///` doc comment including units and ranges (CONVENTIONS.md).

```rust
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// A ComfyUI workflow slot address: `"<node_id>.<input_name>"` (e.g. `"94.tags"`),
/// or `"<subgraph>/<node>.<input>"` for subgraph interiors. Produced by
/// `list_workflow_slots`; see docs/MCP-SURFACE.md §2.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlotAddress(pub String);

/// What a model is for. Drives which studio can use the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Music,
    Image,
}

/// One user-facing control, bound to the slot address(es) it drives.
///
/// A single control may drive several slots: ACE-Step 1.5 turbo carries duration in
/// both `94.duration` and `98.seconds`, and separate planner/sampler seeds in
/// `94.seed` and `3.seed`. The UI shows one control; the pipeline fans it out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSpec {
    /// Free text (style tags, negative prompts).
    Text {
        slots: Vec<SlotAddress>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        advanced: bool,
    },
    /// Song lyrics, with the structure tags this model expects.
    Lyrics {
        slots: Vec<SlotAddress>,
        #[serde(default)]
        structure_tags: Vec<String>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        advanced: bool,
    },
    /// Whole-number control (bpm, step count).
    Int {
        slots: Vec<SlotAddress>,
        min: i64,
        max: i64,
        default: i64,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        advanced: bool,
    },
    /// Fractional control (duration in seconds, shift, sampler temperature).
    Float {
        slots: Vec<SlotAddress>,
        min: f64,
        max: f64,
        default: f64,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        advanced: bool,
    },
    /// Generation seed. Deliberately **not** a `Float`: ACE-Step's seed range runs to
    /// `u64::MAX` (18446744073709551615), which `f64` cannot represent exactly, and a
    /// silently rounded seed destroys reproducibility.
    Seed { slots: Vec<SlotAddress> },
    /// Fixed set of choices (key/scale, language, time signature).
    ///
    /// When `from_node_choices` is true the option list is read live from the node
    /// schema rather than duplicated here, so 34 key/scale and 51 language values stay
    /// correct across ComfyUI updates.
    Enum {
        slots: Vec<SlotAddress>,
        #[serde(default)]
        from_node_choices: bool,
        #[serde(default)]
        choices: Vec<String>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        advanced: bool,
    },
    /// Related controls shown together (e.g. the LM-planner sampling group).
    Group {
        members: BTreeMap<String, InputSpec>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        advanced: bool,
    },
    /// This model does not accept this input, and that was **verified**, not assumed.
    ///
    /// Declared rather than omitted so "we checked, ACE-Step has no negative prompt"
    /// is distinguishable from "nobody considered it". The UI renders no control.
    Unsupported {
        #[serde(default)]
        reason: Option<String>,
    },
}

/// Allowed LoRA strength range as offered in the UI.
///
/// Narrower than the node's own bounds on purpose: `LoraLoaderModelOnly.strength_model`
/// accepts -100.0..=100.0, but only roughly 0.0..=2.0 is musically useful.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrengthRange {
    pub min: f64,
    pub max: f64,
    pub default: f64,
    #[serde(default)]
    pub step: Option<f64>,
}

/// How LoRAs attach for this model. Absent when the model has no LoRA support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoraSupport {
    /// Node class that applies a LoRA. Per-profile because custom node packs differ;
    /// core ComfyUI ships `LoraLoaderModelOnly` (verified, docs/MCP-SURFACE.md §4).
    pub loader_node: String,
    /// Instance id of the node whose MODEL output the loader chain splices after.
    pub attach_after: String,
    /// ComfyUI models sub-folder to enumerate (usually `"loras"`).
    pub folder: String,
    pub strength: StrengthRange,
    /// How many LoRAs may be chained in the UI.
    pub max_stack: u8,
}

/// Where the generated file is written, and in what format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSpec {
    /// Save node the pipeline substitutes in. Templates ship deprecated `SaveAudioMP3`;
    /// latentCreate must not hand lossy audio to the mastering stage.
    pub save_node: String,
    #[serde(default = "default_true")]
    pub prefer_lossless: bool,
}

fn default_true() -> bool {
    true
}

/// How this profile reaches ComfyUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComfySpec {
    /// Gallery template name, e.g. `"audio_ace_step1_5_xl_turbo"`.
    #[serde(default)]
    pub template: Option<String>,
    /// Path to a user-imported API-format workflow (ARCHITECTURE §5b), when the
    /// profile does not use a gallery template.
    #[serde(default)]
    pub workflow: Option<String>,
    /// Rough VRAM floor in gibibytes, for warning before a doomed run.
    #[serde(default)]
    pub vram_gb_min: Option<u32>,
    pub output: OutputSpec,
}

/// What LyricsStudio must produce for this model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricsContract {
    /// `"structure-tagged"`, `"plain"`, or `"none"` for instrumental-only models.
    pub format: String,
    /// Slot whose live node choices list the supported languages.
    #[serde(default)]
    pub languages_from: Option<SlotAddress>,
    /// Token that requests a purely instrumental result, e.g. `"[inst]"`.
    #[serde(default)]
    pub instrumental_token: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// A worked example for the prefills and the lyric-LLM system prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptExample {
    pub tags: String,
    #[serde(default)]
    pub lyrics: Option<String>,
}

/// Guidance shown to the user and fed to the lyric LLM (ARCHITECTURE §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptGuide {
    #[serde(default)]
    pub tag_style: Option<String>,
    #[serde(default)]
    pub examples: Vec<PromptExample>,
}

/// Everything the UI needs to drive one model. Profiles are data, not code:
/// supporting a new model is a JSON file (ARCHITECTURE §5).
///
/// Unknown fields are **ignored deliberately** (no `deny_unknown_fields`) so a profile
/// written for a newer build still loads on an older one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub display_name: String,
    pub kind: ModelKind,
    /// SPDX id where one applies, else a short label. Shown wherever the model is
    /// chosen or installed -- some weights are open-with-conditions, not OSI-open.
    pub license: String,
    #[serde(default)]
    pub license_notes: Option<String>,
    pub comfy: ComfySpec,
    /// Absent when the model has no LoRA support.
    #[serde(default)]
    pub loras: Option<LoraSupport>,
    /// Semantic input name -> control. Ordered for stable serialisation.
    pub inputs: BTreeMap<String, InputSpec>,
    #[serde(default)]
    pub lyrics_contract: Option<LyricsContract>,
    #[serde(default)]
    pub prompt_guide: Option<PromptGuide>,
}
```

`lib.rs` gains `pub mod profile;` and a `pub use profile::*;` re-export, keeping its
existing crate-level docs and its `test_crate_name_is_stable` test.

## The fixture: `profiles/ace-step-1.5-turbo.json`
Every value below was read from the live install (docs/MCP-SURFACE.md §3–4) — do not
invent, round, or "improve" any of it.

```json
{
  "id": "ace-step-1.5-turbo",
  "display_name": "ACE-Step 1.5 XL Turbo",
  "kind": "music",
  "license": "Apache-2.0",
  "license_notes": "Weights are Apache-2.0; no attribution requirement for output.",
  "comfy": {
    "template": "audio_ace_step1_5_xl_turbo",
    "workflow": null,
    "vram_gb_min": 8,
    "output": { "save_node": "SaveAudioAdvanced", "prefer_lossless": true }
  },
  "loras": {
    "loader_node": "LoraLoaderModelOnly",
    "attach_after": "104",
    "folder": "loras",
    "strength": { "min": 0.0, "max": 2.0, "default": 1.0, "step": 0.05 },
    "max_stack": 4
  },
  "inputs": {
    "tags": {
      "type": "text",
      "slots": ["94.tags"],
      "label": "Style tags"
    },
    "lyrics": {
      "type": "lyrics",
      "slots": ["94.lyrics"],
      "structure_tags": ["[Verse]", "[Chorus]", "[Bridge]", "[Outro]", "[inst]"],
      "label": "Lyrics"
    },
    "negative": {
      "type": "unsupported",
      "reason": "TextEncodeAceStepAudio1.5 exposes no negative input, and turbo runs at cfg 1 where one would have no effect."
    },
    "duration_s": {
      "type": "float",
      "slots": ["94.duration", "98.seconds"],
      "min": 10.0,
      "max": 300.0,
      "default": 120.0,
      "step": 1.0,
      "label": "Duration (s)"
    },
    "seed": { "type": "seed", "slots": ["94.seed", "3.seed"] },
    "bpm": {
      "type": "int",
      "slots": ["94.bpm"],
      "min": 10,
      "max": 300,
      "default": 120,
      "label": "BPM"
    },
    "keyscale": {
      "type": "enum",
      "slots": ["94.keyscale"],
      "from_node_choices": true,
      "label": "Key"
    },
    "timesignature": {
      "type": "enum",
      "slots": ["94.timesignature"],
      "from_node_choices": true,
      "label": "Time signature"
    },
    "language": {
      "type": "enum",
      "slots": ["94.language"],
      "from_node_choices": true,
      "label": "Language"
    },
    "steps": {
      "type": "int",
      "slots": ["3.steps"],
      "min": 1,
      "max": 100,
      "default": 8,
      "label": "Steps",
      "advanced": true
    },
    "shift": {
      "type": "float",
      "slots": ["78.shift"],
      "min": 0.0,
      "max": 10.0,
      "default": 3.0,
      "step": 0.1,
      "label": "Shift",
      "advanced": true
    },
    "planner": {
      "type": "group",
      "label": "Planner sampling",
      "advanced": true,
      "members": {
        "cfg_scale": {
          "type": "float",
          "slots": ["94.cfg_scale"],
          "min": 0.0,
          "max": 100.0,
          "default": 2.0,
          "step": 0.1
        },
        "temperature": {
          "type": "float",
          "slots": ["94.temperature"],
          "min": 0.0,
          "max": 2.0,
          "default": 0.85,
          "step": 0.01
        },
        "top_p": {
          "type": "float",
          "slots": ["94.top_p"],
          "min": 0.0,
          "max": 2000.0,
          "default": 0.9,
          "step": 0.01
        },
        "top_k": {
          "type": "int",
          "slots": ["94.top_k"],
          "min": 0,
          "max": 100,
          "default": 0
        },
        "min_p": {
          "type": "float",
          "slots": ["94.min_p"],
          "min": 0.0,
          "max": 1.0,
          "default": 0.0,
          "step": 0.001
        }
      }
    }
  },
  "lyrics_contract": {
    "format": "structure-tagged",
    "languages_from": "94.language",
    "instrumental_token": "[inst]",
    "notes": "Short tag combinations beat prose. Vocal-style cues (e.g. 'deep male voice') belong in tags, not lyrics."
  },
  "prompt_guide": {
    "tag_style": "comma-separated short tags",
    "examples": [
      {
        "tags": "synthwave, retro, 80s, dreamy, female vocal, driving beat, 105 bpm",
        "lyrics": "[Verse]\nNeon on the dashboard, midnight in the rain\n[Chorus]\nDrive until the morning takes the weight away"
      },
      {
        "tags": "melancholic indie folk, acoustic guitar, soft male vocal, intimate, slow tempo",
        "lyrics": null
      }
    ]
  }
}
```

## Tests (in `profile.rs`, `#[cfg(test)] mod tests`)
Load the fixture with `include_str!("../../../profiles/ace-step-1.5-turbo.json")`.

- `test_profile_roundtrip_ace_step_fixture` — parse the fixture, serialise, re-parse, and
  assert the two `ModelProfile` values are equal.
- `test_fixture_matches_verified_slot_addresses` — assert `duration_s` carries **both**
  `94.duration` and `98.seconds`, and `seed` carries **both** `94.seed` and `3.seed`.
  This is the trap the whole slot design exists to hide; a regression here is silent.
- `test_negative_is_declared_unsupported` — `inputs["negative"]` matches
  `InputSpec::Unsupported { .. }` with a non-empty reason.
- `test_seed_max_roundtrips_exactly` — serialise and re-parse `u64::MAX` as a seed value
  through `serde_json` and assert it is unchanged. (Guards the reason `Seed` is not a
  float.)
- `test_profile_without_loras_block_deserializes` — a minimal profile JSON with no
  `loras` key parses, with `loras == None`.
- `test_unknown_fields_are_ignored` — the same minimal JSON plus
  `"future_field": "whatever"` still parses, proving forward compatibility.
- `test_group_members_parse_nested_specs` — `inputs["planner"]` is a `Group` with five
  members, and `top_k` is an `Int`.

## Acceptance criteria
- [ ] `cargo test -p create-core` passes with all seven named tests
- [ ] `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean
- [ ] Every public item has a `///` doc comment
- [ ] `npm run gate` green from the repo root
- [ ] No dependencies beyond the two listed above
- [ ] No changes outside the listed files

## Out of scope
Loading profiles from disk, merging shipped and user profile directories, validating slot
addresses against a live ComfyUI, `Project`/`Track`/`GenerationSpec`/`Provenance` (T-003b),
and any UI.

## Notes for the executor
- `serde(tag = "type")` on `InputSpec` is what makes `"type": "float"` select the variant.
- Do **not** add `deny_unknown_fields` anywhere: forward compatibility is deliberate.
- `BTreeMap`, not `HashMap` — stable key order keeps sidecars diffable.
- The fixture is verified data. If a value looks wrong, stop and ask rather than adjust it.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file crates/create-core/Cargo.toml --file crates/create-core/src/lib.rs --file crates/create-core/src/profile.rs --file profiles/ace-step-1.5-turbo.json
```
