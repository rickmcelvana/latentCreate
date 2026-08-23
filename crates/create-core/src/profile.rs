//! Model capability profiles — the core abstraction of latentCreate.
//!
//! Profiles are data, not code. Everything the UI shows for a music or image model
//! comes from a JSON profile. This module contains the Serde schema and the verified
//! ACE-Step 1.5 XL Turbo fixture.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

#[cfg(test)]
mod tests {
    use super::*;

    const ACE_STEP_FIXTURE: &str = include_str!("../../../profiles/ace-step-1.5-turbo.json");

    #[test]
    fn test_profile_roundtrip_ace_step_fixture() {
        let first: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let json = serde_json::to_string_pretty(&first).unwrap();
        let second: ModelProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_fixture_matches_verified_slot_addresses() {
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();

        let duration = profile.inputs.get("duration_s").expect("duration_s input");
        match duration {
            InputSpec::Float { slots, .. } => {
                assert_eq!(slots.len(), 2);
                assert!(slots.contains(&SlotAddress("94.duration".to_string())));
                assert!(slots.contains(&SlotAddress("98.seconds".to_string())));
            }
            other => panic!("duration_s was not Float: {:?}", other),
        }

        let seed = profile.inputs.get("seed").expect("seed input");
        match seed {
            InputSpec::Seed { slots } => {
                assert_eq!(slots.len(), 2);
                assert!(slots.contains(&SlotAddress("94.seed".to_string())));
                assert!(slots.contains(&SlotAddress("3.seed".to_string())));
            }
            other => panic!("seed was not Seed: {:?}", other),
        }
    }

    #[test]
    fn test_negative_is_declared_unsupported() {
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let negative = profile.inputs.get("negative").expect("negative input");
        match negative {
            InputSpec::Unsupported { reason } => {
                let reason = reason.as_ref().expect("negative should have a reason");
                assert!(!reason.is_empty());
                assert!(reason.contains("negative"));
            }
            other => panic!("negative was not Unsupported: {:?}", other),
        }
    }

    #[test]
    fn test_seed_max_roundtrips_exactly() {
        // Three assertions, because the point is *why* Seed is its own variant.
        //
        // 1. f64 genuinely cannot hold the integers ACE-Step accepts as seeds.
        //    2^53 + 1 is the smallest integer f64 rounds away, and ACE-Step's seed
        //    range runs three orders of magnitude past it, to u64::MAX.
        let lossy: u64 = 9_007_199_254_740_993; // 2^53 + 1
        assert_ne!(
            lossy as f64 as u64, lossy,
            "f64 must be shown to lose integers in the seed range"
        );

        // 2. The schema therefore keeps seeds out of the float path entirely.
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        assert!(
            matches!(profile.inputs.get("seed"), Some(InputSpec::Seed { .. })),
            "seed must be InputSpec::Seed, never Float -- a rounded seed is an              unreproducible track"
        );

        // 3. And a u64 seed survives JSON unchanged, which is what the pipeline relies on.
        let max = u64::MAX;
        let parsed: u64 = serde_json::from_str(&serde_json::to_string(&max).unwrap()).unwrap();
        assert_eq!(
            parsed, max,
            "u64::MAX must not be rounded by JSON serialization"
        );
    }

    #[test]
    fn test_profile_without_loras_block_deserializes() {
        let minimal = r#"
        {
            "id": "minimal",
            "display_name": "Minimal Profile",
            "kind": "music",
            "license": "MIT",
            "comfy": {
                "output": { "save_node": "SaveAudio" }
            },
            "inputs": {}
        }
        "#;
        let profile: ModelProfile = serde_json::from_str(minimal).unwrap();
        assert!(profile.loras.is_none());
    }

    #[test]
    fn test_unknown_fields_are_ignored() {
        let minimal = r#"
        {
            "id": "minimal",
            "display_name": "Minimal Profile",
            "kind": "music",
            "license": "MIT",
            "comfy": {
                "output": { "save_node": "SaveAudio" }
            },
            "inputs": {},
            "future_field": "whatever"
        }
        "#;
        let profile: ModelProfile = serde_json::from_str(minimal).unwrap();
        assert_eq!(profile.id, "minimal");
    }

    #[test]
    fn test_group_members_parse_nested_specs() {
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let planner = profile.inputs.get("planner").expect("planner input");
        match planner {
            InputSpec::Group { members, .. } => {
                assert_eq!(members.len(), 5);
                let top_k = members.get("top_k").expect("top_k member");
                assert!(matches!(top_k, InputSpec::Int { .. }));
            }
            other => panic!("planner was not Group: {:?}", other),
        }
    }
}
