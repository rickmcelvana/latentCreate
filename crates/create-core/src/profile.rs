//! Model capability profiles -- the core abstraction of latentCreate.
//!
//! Profiles are data, not code. Everything the UI shows for a music or image model
//! comes from a JSON profile. This module contains the Serde schema and the verified
//! ACE-Step 1.5 XL Turbo fixture.

use crate::generation::InputValue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A ComfyUI workflow slot address: `"<node_id>.<input_name>"` (e.g. `"94.tags"`),
/// or `"<subgraph>/<node>.<input>"` for subgraph interiors. Produced by
/// `list_workflow_slots`; see docs/MCP-SURFACE.md section 2.
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
/// both `94.duration` and `98.seconds`. The UI shows one control; the pipeline
/// fans it out.
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
        /// Node **class** whose input supplies the live choices.
        ///
        /// Required in practice whenever `from_node_choices` is set, because a
        /// slot address names a node *instance* inside this profile's template
        /// (`"94.keyscale"`), and nothing can turn `94` into
        /// `TextEncodeAceStepAudio1.5` without reading the workflow file. The
        /// input name is the address's field part, so the class is the only
        /// thing that was ever missing.
        ///
        /// `Option` rather than required so a profile written before this
        /// field still loads; a profile that sets `from_node_choices` without
        /// it gets an empty picker that says why, never a guess.
        #[serde(default)]
        node: Option<String>,
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
    /// core ComfyUI ships `LoraLoaderModelOnly` (verified, docs/MCP-SURFACE.md section 4).
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

/// One model file this profile needs present in ComfyUI.
///
/// **Declared, not derived.** comfy-mcp has no tool that answers "which model
/// files does this workflow need": `workflow_deps` maps node classes to node
/// *packs*, and `node_dependencies` checks a pack's *Python* requirements
/// against the venv. The only signal is `local_check`'s prose errors --
/// `"node 104: 'acestep_v1.5_xl_turbo_bf16.safetensors' not in 2 known options
/// for unet_name"` -- and parsing English to decide whether to start a
/// multi-gigabyte download is not something this app will do (MCP-SURFACE 14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelFileSpec {
    /// Exact name, as ComfyUI lists it in `search_models(folder=)`. Compared
    /// verbatim: this is the string the workflow's enum slot holds.
    ///
    /// ComfyUI reports nested models as a relative path with the OS-native
    /// separator, so a file inside a sub-directory cannot be named portably
    /// here (MCP-SURFACE 11.1). Declare top-level files only.
    pub file: String,
    /// ComfyUI models sub-folder, e.g. `"diffusion_models"`. Not always
    /// `"checkpoints"` -- ACE-Step 1.5 ships as a split unet/vae/text-encoder
    /// set and puts nothing in `checkpoints` at all.
    pub folder: String,
    /// Direct download URL. `None` means this app cannot fetch the file and
    /// must instead tell the user the name and folder to place it in.
    #[serde(default)]
    pub source_url: Option<String>,
    /// Download size in bytes, so the total can be shown *before* the user
    /// commits to it. ACE-Step 1.5 XL Turbo is 18.5 GiB across four files.
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// Set only when this file's terms differ from the profile's own licence.
    #[serde(default)]
    pub license: Option<String>,
}

/// How this profile reaches ComfyUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComfySpec {
    /// Gallery template name, e.g. `"audio_ace_step1_5_xl_turbo"`.
    #[serde(default)]
    pub template: Option<String>,
    /// Path to a user-imported API-format workflow (ARCHITECTURE section 5b), when the
    /// profile does not use a gallery template.
    #[serde(default)]
    pub workflow: Option<String>,
    /// Rough VRAM floor in gibibytes, for warning before a doomed run.
    #[serde(default)]
    pub vram_gb_min: Option<u32>,
    /// Slot values pinned by the profile, applied to the fetched template before
    /// the user's inputs. This is how a profile targets a specific checkpoint
    /// variant: MiniMax Music 3's template hardcodes the fp16 DiT, so the profile
    /// overrides `37/6.unet_name` to the int8 file (MCP-SURFACE 6).
    #[serde(default)]
    pub slot_overrides: BTreeMap<SlotAddress, InputValue>,
    /// Every model file this profile needs. Empty means "not declared", which
    /// the UI reports as unknown -- never as ready.
    #[serde(default)]
    pub models: Vec<ModelFileSpec>,
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

/// Guidance shown to the user and fed to the lyric LLM (ARCHITECTURE section 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptGuide {
    #[serde(default)]
    pub tag_style: Option<String>,
    #[serde(default)]
    pub examples: Vec<PromptExample>,
}

/// Everything the UI needs to drive one model. Profiles are data, not code:
/// supporting a new model is a JSON file (ARCHITECTURE section 5).
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

impl InputSpec {
    /// Adds every slot address this control writes to `into`, descending into
    /// group members. [`InputSpec::Unsupported`] writes nothing by definition.
    fn collect_slots(&self, into: &mut BTreeSet<SlotAddress>) {
        match self {
            InputSpec::Text { slots, .. }
            | InputSpec::Lyrics { slots, .. }
            | InputSpec::Int { slots, .. }
            | InputSpec::Float { slots, .. }
            | InputSpec::Seed { slots }
            | InputSpec::Enum { slots, .. } => {
                into.extend(slots.iter().cloned());
            }
            InputSpec::Group { members, .. } => {
                for member in members.values() {
                    member.collect_slots(into);
                }
            }
            InputSpec::Unsupported { .. } => {}
        }
    }
}

impl ModelProfile {
    /// Every slot address this profile names, de-duplicated and sorted.
    ///
    /// Three sources, because all three must exist in the template or the
    /// profile is broken in a way the user sees only at generation time: the
    /// `inputs` it writes (group members included), the `slot_overrides` it
    /// pins, and the `lyrics_contract.languages_from` address it reads the
    /// live language list from.
    ///
    /// Pair with `SlotList::missing` in `mcp-bridge` to check a profile
    /// against a fetched template; nothing here touches ComfyUI.
    pub fn slot_addresses(&self) -> BTreeSet<SlotAddress> {
        let mut addresses = BTreeSet::new();
        for input in self.inputs.values() {
            input.collect_slots(&mut addresses);
        }
        addresses.extend(self.comfy.slot_overrides.keys().cloned());
        if let Some(contract) = &self.lyrics_contract {
            if let Some(languages_from) = &contract.languages_from {
                addresses.insert(languages_from.clone());
            }
        }
        addresses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACE_STEP_FIXTURE: &str = include_str!("../../../profiles/ace-step-1.5-turbo.json");
    const MINIMAX_FIXTURE: &str = include_str!("../../../profiles/minimax-music-3.json");

    #[test]
    fn test_profile_roundtrip_ace_step_fixture() {
        let first: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let json = serde_json::to_string_pretty(&first).unwrap();
        let second: ModelProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_profile_roundtrip_minimax_fixture() {
        let first: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
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
                assert_eq!(slots, &[SlotAddress("109.value".to_string())]);
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

    /// Protects: the checkpoint-variant override -- the profile pins the int8 DiT
    /// over the template's hardcoded fp16 filename (MCP-SURFACE 6).
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

    /// Protects: the save-node swap is conditional -- MiniMax's template already
    /// uses `SaveAudioAdvanced`, so the profile declares it (not a deprecated
    /// node) and the pipeline must not intervene (MCP-SURFACE 6a).
    #[test]
    fn test_minimax_fixture_declares_lossless_output() {
        let profile: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
        assert_eq!(profile.comfy.output.save_node, "SaveAudioAdvanced");
        assert!(profile.comfy.output.prefer_lossless);
    }

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

    /// Every slot address MCP-SURFACE 3 records from the live
    /// `audio_ace_step1_5_xl_turbo` template (verified 2026-08-23,
    /// `local_check: runnable: true`). Not the full 33 -- the documented
    /// subset, which covers everything the shipped profile drives.
    const VERIFIED_ACE_STEP_SLOTS: &[&str] = &[
        "107.filename_prefix",
        "107.quality",
        "109.value",
        "3.cfg",
        "3.denoise",
        "3.sampler_name",
        "3.scheduler",
        "3.seed",
        "3.steps",
        "78.shift",
        "94.bpm",
        "94.cfg_scale",
        "94.duration",
        "94.generate_audio_codes",
        "94.keyscale",
        "94.language",
        "94.lyrics",
        "94.min_p",
        "94.seed",
        "94.tags",
        "94.temperature",
        "94.timesignature",
        "94.top_k",
        "94.top_p",
        "98.seconds",
    ];

    /// Protects: the collector descends into groups and skips `Unsupported`.
    /// Asserted as an exact set, so a group whose members stop being walked
    /// (the LM-planner's five controls) fails, and so does an `Unsupported`
    /// input that starts contributing a phantom address.
    #[test]
    fn test_slot_addresses_walk_groups_and_skip_unsupported() {
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let addresses: BTreeSet<String> =
            profile.slot_addresses().into_iter().map(|a| a.0).collect();

        let expected: BTreeSet<String> = [
            "109.value",
            "3.steps",
            "78.shift",
            "94.bpm",
            "94.cfg_scale",
            "94.duration",
            "94.keyscale",
            "94.language",
            "94.lyrics",
            "94.min_p",
            "94.tags",
            "94.temperature",
            "94.timesignature",
            "94.top_k",
            "94.top_p",
            "98.seconds",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(addresses, expected);
    }

    /// Protects: a pinned checkpoint variant is checked like any other
    /// address. `37/6.unet_name` appears in no input -- only in
    /// `slot_overrides` -- and an override the template lacks fails at
    /// generation time, which is exactly what this collector exists to
    /// prevent.
    #[test]
    fn test_slot_addresses_include_slot_overrides() {
        let profile: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
        let addresses = profile.slot_addresses();
        assert!(addresses.contains(&SlotAddress("37/6.unet_name".to_string())));
        assert!(profile
            .inputs
            .values()
            .all(|input| !format!("{input:?}").contains("37/6.unet_name")));
    }

    /// Protects: `lyrics_contract.languages_from` is collected even when no
    /// input writes it. The app reads the live language list from that
    /// address; if it does not exist the language picker is empty, silently.
    #[test]
    fn test_slot_addresses_include_languages_from() {
        let json = r#"
        {
            "id": "reads-languages",
            "display_name": "Reads Languages",
            "kind": "music",
            "license": "MIT",
            "comfy": { "output": { "save_node": "SaveAudioAdvanced" } },
            "inputs": {
                "tags": { "type": "text", "slots": ["94.tags"] }
            },
            "lyrics_contract": {
                "format": "plain",
                "languages_from": "94.language"
            }
        }
        "#;
        let profile: ModelProfile = serde_json::from_str(json).unwrap();
        let addresses = profile.slot_addresses();
        assert!(addresses.contains(&SlotAddress("94.language".to_string())));
        assert_eq!(addresses.len(), 2);
    }

    /// Protects: every address the shipped ACE-Step profile names exists in
    /// the live-captured slot list. A typo in the profile JSON -- `94.tag`
    /// for `94.tags` -- fails here instead of producing a track generated
    /// from the template's default prompt.
    #[test]
    fn test_shipped_ace_step_addresses_all_exist_in_the_verified_template() {
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let known: BTreeSet<&str> = VERIFIED_ACE_STEP_SLOTS.iter().copied().collect();
        let missing: Vec<String> = profile
            .slot_addresses()
            .into_iter()
            .filter(|a| !known.contains(a.0.as_str()))
            .map(|a| a.0)
            .collect();
        assert!(
            missing.is_empty(),
            "addresses not in the template: {missing:?}"
        );
    }
}
