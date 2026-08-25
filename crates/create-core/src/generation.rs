use crate::profile::SlotAddress;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
