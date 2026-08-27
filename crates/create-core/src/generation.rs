use crate::profile::{InputSpec, ModelProfile, SlotAddress};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A concrete value for one input, tagged so it survives a JSON round trip.
///
/// Adjacently tagged on purpose: untagged, a JSON `3` could deserialise as `Int`,
/// `Float` or `Seed`, and a seed silently demoted to an `Int` would make a track
/// unreproducible -- the one thing provenance must never allow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum InputValue {
    /// Free text.
    Text(String),
    /// Whole number.
    Int(i64),
    /// Fractional number.
    Float(f64),
    /// Generation seed. `u64` because ACE-Step's range reaches `u64::MAX` (T-003).
    Seed(u64),
    /// A choice from a fixed set (key/scale, language, time signature).
    Enum(String),
    /// On/off toggle.
    Bool(bool),
}

/// Why a spec could not be turned into slot values.
///
/// Every variant names the offending input, because these are read by a user
/// who is looking at a form with that label on it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ResolveError {
    /// The spec carries an input this profile does not declare.
    ///
    /// An error rather than a shrug: silently dropping an unknown name is how
    /// a user's duration fails to apply with nothing on screen to explain it.
    #[error("{profile_id} has no input named {input}")]
    UnknownInput { profile_id: String, input: String },
    /// The spec carries a value for an input this model does not accept.
    #[error("{profile_id} does not support {input}: {reason}")]
    Unsupported {
        profile_id: String,
        input: String,
        reason: String,
    },
    /// The value's type does not match what the profile declares.
    #[error("{input} expects {expected}, got {actual}")]
    TypeMismatch {
        input: String,
        expected: &'static str,
        actual: &'static str,
    },
    /// A number outside the profile's declared range.
    #[error("{input} must be between {min} and {max}, got {value}")]
    OutOfRange {
        input: String,
        min: f64,
        max: f64,
        value: f64,
    },
    /// A choice the profile does not list.
    #[error("{input} does not accept {value}")]
    NotAChoice { input: String, value: String },
}

impl InputValue {
    /// The bare JSON value a slot write carries.
    ///
    /// **Not `serde_json::to_value(self)`.** `InputValue` is adjacently tagged, so
    /// that yields `{"type":"seed","value":42}` -- an object where the slot wants a
    /// number. comfy-mcp rejects it with `[workflow_slot_invalid]` (`expected INT,
    /// got dict`), for STRING slots too, so the mistake fails closed rather than
    /// corrupting a run; it still fails every generation.
    ///
    /// The tag exists so a value survives the round trip through provenance
    /// (T-003); it must be dropped on the way to the wire.
    pub fn to_slot_value(&self) -> serde_json::Value {
        match self {
            InputValue::Text(s) | InputValue::Enum(s) => serde_json::Value::String(s.clone()),
            InputValue::Int(i) => serde_json::Value::from(*i),
            InputValue::Float(f) => serde_json::Value::from(*f),
            InputValue::Seed(s) => serde_json::Value::from(*s),
            InputValue::Bool(b) => serde_json::Value::Bool(*b),
        }
    }

    /// The variant name, for error messages.
    fn kind(&self) -> &'static str {
        match self {
            InputValue::Text(_) => "text",
            InputValue::Int(_) => "int",
            InputValue::Float(_) => "float",
            InputValue::Seed(_) => "seed",
            InputValue::Enum(_) => "enum",
            InputValue::Bool(_) => "bool",
        }
    }
}

/// One LoRA in the stack. Order is this value's position in `GenerationSpec::loras`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoraRef {
    /// Exactly as ComfyUI lists it in `lora_name`, including any sub-directory
    /// e.g. `"ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors"`.
    /// Never normalise the separators; this string is passed back verbatim.
    pub file: String,
    /// Applied strength; UI range is usually 0.0..=2.0 (profile decides).
    pub strength: f64,
    /// Bypassed entries stay in the list so the user can toggle without losing them.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Which lyric document and version a generation used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricRef {
    /// The lyric document.
    pub doc_id: LyricDocId,
    /// 1-based version number within that document.
    pub version: u32,
}

/// Opaque id for a lyric document. Filesystem-safe and sortable; `library` mints it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LyricDocId(pub String);

/// Everything needed to run one generation: the semantic choices, before they are
/// fanned out to slot addresses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationSpec {
    /// `ModelProfile::id` this spec was built against.
    pub profile_id: String,
    /// Semantic input name (`"tags"`, `"duration_s"`, `"seed"`) -> chosen value.
    pub inputs: BTreeMap<String, InputValue>,
    /// LoRAs to apply, in order.
    #[serde(default)]
    pub loras: Vec<LoraRef>,
    /// Lyrics to render, when the model accepts them.
    #[serde(default)]
    pub lyrics: Option<LyricRef>,
}

impl GenerationSpec {
    /// Conventional key under which the seed is stored in `inputs`.
    pub const SEED_KEY: &'static str = "seed";

    /// The seed, if one is set. `None` when the profile has no seed input.
    pub fn seed(&self) -> Option<u64> {
        match self.inputs.get(Self::SEED_KEY) {
            Some(InputValue::Seed(s)) => Some(*s),
            _ => None,
        }
    }

    /// This spec with a different seed, leaving every other input untouched.
    ///
    /// This is how a batch works: one spec, N seeds (ARCHITECTURE 7).
    pub fn with_seed(&self, seed: u64) -> Self {
        let mut next = self.clone();
        next.inputs
            .insert(Self::SEED_KEY.to_string(), InputValue::Seed(seed));
        next
    }

    /// The LoRAs that will actually be applied, in order, skipping bypassed entries.
    pub fn active_loras(&self) -> impl Iterator<Item = &LoraRef> {
        self.loras.iter().filter(|l| l.enabled)
    }
}

/// The slot values actually submitted to ComfyUI, after fan-out.
pub type ResolvedSlots = BTreeMap<SlotAddress, InputValue>;

impl ModelProfile {
    /// Every input this profile declares, keyed by the name a spec uses.
    ///
    /// Group members are dotted (`"planner.temperature"`) so a member can never
    /// collide with a top-level name, and so an error message says which group
    /// the control lives in.
    pub fn flat_inputs(&self) -> BTreeMap<String, &InputSpec> {
        let mut flat = BTreeMap::new();
        for (name, spec) in &self.inputs {
            flatten_into(name, spec, &mut flat);
        }
        flat
    }

    /// Turn the semantic choices into the slot values actually submitted.
    ///
    /// **Only what the spec sets is written.** An input the spec omits is left
    /// alone, because `fetch_template` already carries the template's own
    /// defaults and the profile's `default` fields exist to seed the *form*,
    /// not to restate the template. Writing every declared slot on every run
    /// would have the app silently asserting values it has no opinion about.
    ///
    /// `slot_overrides` go in first and the spec's inputs after, so a profile
    /// that pins a checkpoint variant (MCP-SURFACE 6) still gets it. The two
    /// sets are not expected to intersect; a profile where they do is an
    /// authoring mistake, and the shipped ones are tested for it.
    ///
    /// Fan-out lives here: one semantic value reaches every address the input
    /// names, which is what hides ACE-Step's two durations and two seeds.
    pub fn resolve_slots(&self, spec: &GenerationSpec) -> Result<ResolvedSlots, ResolveError> {
        let declared = self.flat_inputs();
        let mut resolved: ResolvedSlots = self.comfy.slot_overrides.clone();

        for (name, value) in &spec.inputs {
            let input = declared
                .get(name.as_str())
                .ok_or_else(|| ResolveError::UnknownInput {
                    profile_id: self.id.clone(),
                    input: name.clone(),
                })?;
            check(name, input, value, &self.id)?;
            for address in slots_of(input) {
                resolved.insert(address.clone(), value.clone());
            }
        }

        Ok(resolved)
    }
}

/// Add `spec` under `name`, descending into groups with a dotted prefix.
fn flatten_into<'a>(name: &str, spec: &'a InputSpec, into: &mut BTreeMap<String, &'a InputSpec>) {
    if let InputSpec::Group { members, .. } = spec {
        for (member, inner) in members {
            flatten_into(&format!("{name}.{member}"), inner, into);
        }
    } else {
        into.insert(name.to_string(), spec);
    }
}

/// The addresses one control writes. A group writes none of its own.
fn slots_of(spec: &InputSpec) -> &[SlotAddress] {
    match spec {
        InputSpec::Text { slots, .. }
        | InputSpec::Lyrics { slots, .. }
        | InputSpec::Int { slots, .. }
        | InputSpec::Float { slots, .. }
        | InputSpec::Seed { slots }
        | InputSpec::Enum { slots, .. } => slots,
        InputSpec::Group { .. } | InputSpec::Unsupported { .. } => &[],
    }
}

/// Whether `value` is acceptable for `spec`.
///
/// Types are matched exactly rather than widened. `set_workflow_slot`'s
/// structured form preserves the type it is given (MCP-SURFACE 9.1), so an
/// `Int` accepted for a `Float` control is an integer landing in a FLOAT slot,
/// and a seed demoted to an `Int` is a track that cannot be reproduced.
fn check(
    name: &str,
    spec: &InputSpec,
    value: &InputValue,
    profile_id: &str,
) -> Result<(), ResolveError> {
    let mismatch = |expected| {
        Err(ResolveError::TypeMismatch {
            input: name.to_string(),
            expected,
            actual: value.kind(),
        })
    };

    match (spec, value) {
        (InputSpec::Text { .. }, InputValue::Text(_)) => Ok(()),
        (InputSpec::Text { .. }, _) => mismatch("text"),
        (InputSpec::Lyrics { .. }, InputValue::Text(_)) => Ok(()),
        (InputSpec::Lyrics { .. }, _) => mismatch("text"),
        (InputSpec::Seed { .. }, InputValue::Seed(_)) => Ok(()),
        (InputSpec::Seed { .. }, _) => mismatch("seed"),
        (InputSpec::Int { min, max, .. }, InputValue::Int(v)) => {
            in_range(name, *v as f64, *min as f64, *max as f64)
        }
        (InputSpec::Int { .. }, _) => mismatch("int"),
        (InputSpec::Float { min, max, .. }, InputValue::Float(v)) => in_range(name, *v, *min, *max),
        (InputSpec::Float { .. }, _) => mismatch("float"),
        (
            InputSpec::Enum {
                from_node_choices,
                choices,
                ..
            },
            InputValue::Enum(v),
        ) => {
            // A live-read list cannot be checked here: the choices come from
            // the node schema at render time, and this crate never talks to
            // ComfyUI (ARCHITECTURE 2).
            if *from_node_choices || choices.iter().any(|c| c == v) {
                Ok(())
            } else {
                Err(ResolveError::NotAChoice {
                    input: name.to_string(),
                    value: v.clone(),
                })
            }
        }
        (InputSpec::Enum { .. }, _) => mismatch("enum"),
        (InputSpec::Unsupported { reason }, _) => Err(ResolveError::Unsupported {
            profile_id: profile_id.to_string(),
            input: name.to_string(),
            reason: reason
                .clone()
                .unwrap_or_else(|| "this model has no such input".to_string()),
        }),
        (InputSpec::Group { .. }, _) => Err(ResolveError::UnknownInput {
            profile_id: profile_id.to_string(),
            input: name.to_string(),
        }),
    }
}

fn in_range(name: &str, value: f64, min: f64, max: f64) -> Result<(), ResolveError> {
    if value < min || value > max {
        return Err(ResolveError::OutOfRange {
            input: name.to_string(),
            min,
            max,
            value,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn ace_step() -> ModelProfile {
        serde_json::from_str(include_str!("../../../profiles/ace-step-1.5-turbo.json")).unwrap()
    }

    fn minimax() -> ModelProfile {
        serde_json::from_str(include_str!("../../../profiles/minimax-music-3.json")).unwrap()
    }

    #[test]
    fn test_seed_value_does_not_collapse_to_int() {
        let original = InputValue::Seed(3);
        let json = serde_json::to_string(&original).unwrap();
        let parsed: InputValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, InputValue::Seed(3));
        assert_ne!(parsed, InputValue::Int(3));
    }

    #[test]
    fn test_lora_path_with_backslashes_roundtrips() {
        let file = r"ACE-Step-v1.5-ambient_dream1-LoRA\adapter_model.safetensors".to_string();
        let original = LoraRef {
            file: file.clone(),
            strength: 0.85,
            enabled: true,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: LoraRef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.file, file);
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_with_seed_replaces_only_the_seed() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "tags".to_string(),
            InputValue::Text("synthwave".to_string()),
        );
        inputs.insert("duration_s".to_string(), InputValue::Float(120.0));
        inputs.insert(GenerationSpec::SEED_KEY.to_string(), InputValue::Seed(42));
        let spec = GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let next = spec.with_seed(99);
        assert_eq!(next.seed(), Some(99));
        assert_eq!(
            next.inputs.get("tags"),
            Some(&InputValue::Text("synthwave".to_string()))
        );
        assert_eq!(
            next.inputs.get("duration_s"),
            Some(&InputValue::Float(120.0))
        );
        assert_eq!(next.inputs.len(), 3);
    }

    #[test]
    fn test_seed_returns_none_when_absent() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "tags".to_string(),
            InputValue::Text("synthwave".to_string()),
        );
        let spec = GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs,
            loras: vec![],
            lyrics: None,
        };
        assert_eq!(spec.seed(), None);
    }

    #[test]
    fn test_active_loras_skips_disabled() {
        let spec = GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs: BTreeMap::new(),
            loras: vec![
                LoraRef {
                    file: "a.safetensors".to_string(),
                    strength: 1.0,
                    enabled: true,
                },
                LoraRef {
                    file: "b.safetensors".to_string(),
                    strength: 0.75,
                    enabled: false,
                },
                LoraRef {
                    file: "c.safetensors".to_string(),
                    strength: 1.25,
                    enabled: true,
                },
            ],
            lyrics: None,
        };

        let active: Vec<&LoraRef> = spec.active_loras().collect();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].file, "a.safetensors");
        assert_eq!(active[1].file, "c.safetensors");
    }

    #[test]
    fn test_duration_reaches_both_slots() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("duration_s".to_string(), InputValue::Float(150.0));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let resolved = profile.resolve_slots(&spec).unwrap();
        assert_eq!(
            resolved.get(&SlotAddress("94.duration".to_string())),
            Some(&InputValue::Float(150.0))
        );
        assert_eq!(
            resolved.get(&SlotAddress("98.seconds".to_string())),
            Some(&InputValue::Float(150.0))
        );
    }

    #[test]
    fn test_to_slot_value_per_variant() {
        assert_eq!(
            InputValue::Text("x".to_string()).to_slot_value(),
            serde_json::Value::String("x".to_string())
        );
        assert_eq!(
            InputValue::Enum("x".to_string()).to_slot_value(),
            serde_json::Value::String("x".to_string())
        );
        assert_eq!(
            InputValue::Int(12).to_slot_value(),
            serde_json::Value::from(12)
        );
        assert_eq!(
            InputValue::Float(1.5).to_slot_value(),
            serde_json::Value::from(1.5)
        );
        assert_eq!(
            InputValue::Bool(true).to_slot_value(),
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            InputValue::Seed(u64::MAX).to_slot_value(),
            serde_json::Value::from(u64::MAX)
        );
    }

    #[test]
    fn test_to_slot_value_is_not_serde_to_value() {
        let value = InputValue::Seed(42);
        let serde_value = serde_json::to_value(&value).unwrap();
        assert!(
            serde_value.get("type").is_some(),
            "serde must produce the adjacent tag"
        );
        assert_eq!(value.to_slot_value(), serde_json::Value::from(42));
    }

    #[test]
    fn test_seed_reaches_its_slot_and_u64_max_survives() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("seed".to_string(), InputValue::Seed(u64::MAX));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let resolved = profile.resolve_slots(&spec).unwrap();
        assert_eq!(
            resolved.get(&SlotAddress("109.value".to_string())),
            Some(&InputValue::Seed(u64::MAX))
        );
        // The graph fans the seed out to both sampler and planner; the profile
        // no longer lists both addresses. u64::MAX still survives resolution
        // unchanged, but the live PrimitiveInt slot tops out at i64::MAX and
        // validate_workflow rejects anything above it (MCP-SURFACE 18.3/18.4).
    }

    #[test]
    fn test_group_member_resolves_under_dotted_name() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("planner.temperature".to_string(), InputValue::Float(0.95));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let resolved = profile.resolve_slots(&spec).unwrap();
        assert_eq!(
            resolved.get(&SlotAddress("94.temperature".to_string())),
            Some(&InputValue::Float(0.95))
        );
    }

    #[test]
    fn test_flat_inputs_contains_group_members_not_bare_group() {
        let profile = ace_step();
        let flat = profile.flat_inputs();
        assert!(flat.contains_key("planner.cfg_scale"));
        assert!(flat.contains_key("planner.temperature"));
        assert!(flat.contains_key("planner.top_p"));
        assert!(flat.contains_key("planner.top_k"));
        assert!(flat.contains_key("planner.min_p"));
        assert!(!flat.contains_key("planner"));
    }

    #[test]
    fn test_slot_overrides_seed_the_map() {
        let profile = minimax();
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs: BTreeMap::new(),
            loras: vec![],
            lyrics: None,
        };

        let resolved = profile.resolve_slots(&spec).unwrap();
        assert_eq!(
            resolved.get(&SlotAddress("37/6.unet_name".to_string())),
            Some(&InputValue::Enum(
                "minimax_music3_dit_int8_convrot.safetensors".to_string()
            ))
        );
    }

    #[test]
    fn test_slot_overrides_survive_with_inputs() {
        let profile = minimax();
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "caption".to_string(),
            InputValue::Text("test caption".to_string()),
        );
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let resolved = profile.resolve_slots(&spec).unwrap();
        assert_eq!(
            resolved.get(&SlotAddress("37/6.unet_name".to_string())),
            Some(&InputValue::Enum(
                "minimax_music3_dit_int8_convrot.safetensors".to_string()
            ))
        );
        assert_eq!(
            resolved.get(&SlotAddress("37/13.caption".to_string())),
            Some(&InputValue::Text("test caption".to_string()))
        );
    }

    #[test]
    fn test_negative_is_unsupported() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("negative".to_string(), InputValue::Text("bad".to_string()));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        match err {
            ResolveError::Unsupported {
                profile_id,
                input,
                reason,
            } => {
                assert_eq!(profile_id, "ace-step-1.5-turbo");
                assert_eq!(input, "negative");
                assert!(reason.contains("negative"));
            }
            other => panic!("expected Unsupported, got {:?}", other),
        }
    }

    #[test]
    fn test_unknown_input_errors() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("not_real".to_string(), InputValue::Text("x".to_string()));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownInput { .. }));
    }

    #[test]
    fn test_type_mismatch_errors() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("duration_s".to_string(), InputValue::Int(120));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        assert!(matches!(err, ResolveError::TypeMismatch { .. }));
    }

    /// Protects: **a seed is never accepted as a plain integer.**
    ///
    /// This is the demotion `InputValue`'s adjacent tagging exists to prevent
    /// (T-003): an `Int` in a seed slot is a track whose sidecar cannot
    /// reproduce it, and nothing downstream would notice. Found missing by
    /// mutation -- loosening the seed arm to accept `Int` passed all 22 tests
    /// of the T-304 run, so `test_type_mismatch_errors` above was guarding a
    /// float control and nothing was guarding this one.
    #[test]
    fn test_a_seed_is_not_accepted_as_an_int() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("seed".to_string(), InputValue::Int(42));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        assert!(
            matches!(&err, ResolveError::TypeMismatch { expected, actual, .. }
                if *expected == "seed" && *actual == "int"),
            "expected a seed/int mismatch, got {err:?}"
        );
    }

    /// Protects: the reverse demotion, a seed offered where an integer belongs.
    /// `steps` is an `Int` control; a `Seed` reaching it would be a u64 in a
    /// slot ranged 1..=100.
    #[test]
    fn test_a_seed_is_not_accepted_for_an_int_control() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("steps".to_string(), InputValue::Seed(8));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        assert!(
            matches!(&err, ResolveError::TypeMismatch { expected, actual, .. }
                if *expected == "int" && *actual == "seed"),
            "expected an int/seed mismatch, got {err:?}"
        );
    }

    #[test]
    fn test_out_of_range_int_errors() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("bpm".to_string(), InputValue::Int(400));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        assert!(matches!(err, ResolveError::OutOfRange { .. }));
    }

    #[test]
    fn test_out_of_range_float_errors() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert("duration_s".to_string(), InputValue::Float(500.0));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        assert!(matches!(err, ResolveError::OutOfRange { .. }));
    }

    #[test]
    fn test_static_enum_rejects_unknown_choice() {
        let json = r#"
        {
            "id": "enum-test",
            "display_name": "Enum Test",
            "kind": "music",
            "license": "MIT",
            "comfy": { "output": { "save_node": "SaveAudio" } },
            "inputs": {
                "key": { "type": "enum", "slots": ["1.key"], "choices": ["C", "G"], "label": "Key" }
            }
        }
        "#;
        let profile: ModelProfile = serde_json::from_str(json).unwrap();
        let mut inputs = BTreeMap::new();
        inputs.insert("key".to_string(), InputValue::Enum("D".to_string()));
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let err = profile.resolve_slots(&spec).unwrap_err();
        assert!(matches!(err, ResolveError::NotAChoice { .. }));
    }

    #[test]
    fn test_from_node_choices_enum_accepts_anything() {
        let profile = ace_step();
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "keyscale".to_string(),
            InputValue::Enum("totally-made-up-key".to_string()),
        );
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let resolved = profile.resolve_slots(&spec).unwrap();
        assert_eq!(
            resolved.get(&SlotAddress("94.keyscale".to_string())),
            Some(&InputValue::Enum("totally-made-up-key".to_string()))
        );
    }

    #[test]
    fn test_shipped_profiles_overrides_do_not_intersect_inputs() {
        for profile in [ace_step(), minimax()] {
            let mut input_addresses = BTreeSet::new();
            for spec in profile.flat_inputs().values() {
                input_addresses.extend(slots_of(spec).iter().cloned());
            }
            for override_addr in profile.comfy.slot_overrides.keys() {
                assert!(
                    !input_addresses.contains(override_addr),
                    "{}: slot override {} collides with an input slot",
                    profile.id,
                    override_addr.0
                );
            }
        }
    }

    #[test]
    fn test_empty_spec_resolves_to_empty_map_when_no_overrides() {
        let profile = ace_step();
        let spec = GenerationSpec {
            profile_id: profile.id.clone(),
            inputs: BTreeMap::new(),
            loras: vec![],
            lyrics: None,
        };

        let resolved = profile.resolve_slots(&spec).unwrap();
        assert!(resolved.is_empty());
    }
}
