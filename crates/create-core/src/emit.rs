//! Turning accepted role mappings into a real [`ModelProfile`].
//!
//! ARCHITECTURE 5b's bar is a user profile **indistinguishable from a shipped
//! one**, so this builds the same struct the shipped JSON deserializes into and
//! writes it to the same directory. Nothing about an imported profile is a
//! second class of thing.
//!
//! **Two honest limits, stated rather than hidden.** The shipped ACE-Step
//! profile declares `steps: 1..100`; the node really accepts `1..10000`, so
//! that narrowing is a human curating a model they know. Emission cannot
//! reproduce it and uses the node's real bounds instead, because the
//! alternative is inventing a range for a graph nobody here has seen. And a
//! numeric input whose bounds the registry does not report is **refused**,
//! not filled in -- a slider with invented limits is worse than an absent
//! control.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::profile::{ComfySpec, InputSpec, ModelKind, ModelProfile, OutputSpec, SlotAddress};
use crate::roles::Role;

/// The save node an emitted profile writes through, matching both shipped
/// profiles. `prefer_lossless` is what stops a lossy file reaching the
/// mastering stage.
const SAVE_NODE: &str = "SaveAudioAdvanced";

/// Node classes [`crate::graph::ensure_lossless_output`] recognises. Kept in
/// step with that module's own list -- if a graph has none of these, generation
/// fails with `GraphError::NoSaveNode`, and catching it here is the difference
/// between an explanation and a surprise.
const SAVE_NODE_TYPES: [&str; 4] = [
    "SaveAudio",
    "SaveAudioMP3",
    "SaveAudioOpus",
    "SaveAudioAdvanced",
];

/// Numeric bounds for one input, from the live node registry.
#[derive(Debug, Clone, PartialEq)]
pub struct Bounds {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
}

/// One slot a role was mapped to, with everything needed to declare it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappedSlot {
    pub address: String,
    /// `STRING` / `INT` / `FLOAT`, as `list_workflow_slots` reports it.
    pub widget_type: String,
    /// What the user's graph currently holds -- the emitted default, because
    /// it is the value they already chose.
    pub current_value: Value,
    /// `None` when the registry reported no bounds for this input.
    pub bounds: Option<Bounds>,
}

/// Why a mapping could not become a profile.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EmitError {
    /// A numeric role whose bounds the registry did not report.
    #[error("{role:?} is mapped to {address}, whose limits ComfyUI does not report. Map it to a different input, or leave it out.")]
    NoBounds { role: Role, address: String },
    /// A role present in the mapping but pointing at nothing.
    #[error("{role:?} was accepted but mapped to no slot")]
    NoSlots { role: Role },
    /// The graph has no node this app can save audio through.
    #[error("This workflow has no audio save node, so latentCreate cannot collect what it produces. Add a Save Audio node in ComfyUI, then import it again.")]
    NoSaveNode,
}

/// The profile key for a role. The shipped names, because a person reads this
/// file -- the *binding* contract is the `InputSpec` variant, since the app
/// finds the lyrics control by kind rather than by name.
fn key_for(role: Role) -> &'static str {
    match role {
        Role::Tags => "tags",
        Role::Lyrics => "lyrics",
        Role::Negative => "negative",
        Role::DurationSeconds => "duration_s",
        Role::Seed => "seed",
        Role::Steps => "steps",
        Role::Cfg => "cfg",
    }
}

/// The label the panel shows.
fn label_for(role: Role) -> &'static str {
    match role {
        Role::Tags => "Style tags",
        Role::Lyrics => "Lyrics",
        Role::Negative => "Negative prompt",
        Role::DurationSeconds => "Duration (s)",
        Role::Seed => "Seed",
        Role::Steps => "Steps",
        Role::Cfg => "CFG",
    }
}

/// Whether `workflow` has a node the pipeline can save audio through.
pub fn has_audio_save_node(workflow: &Value) -> bool {
    fn scan(nodes: Option<&Vec<Value>>) -> bool {
        nodes.is_some_and(|nodes| {
            nodes.iter().any(|n| {
                n.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| SAVE_NODE_TYPES.contains(&t))
            })
        })
    }

    if scan(workflow.get("nodes").and_then(Value::as_array)) {
        return true;
    }
    // Subgraph interiors count: MiniMax's save node lives inside one, and
    // `ensure_lossless_output` rewrites both levels.
    workflow
        .pointer("/definitions/subgraphs")
        .and_then(Value::as_array)
        .is_some_and(|subs| {
            subs.iter()
                .any(|s| scan(s.get("nodes").and_then(Value::as_array)))
        })
}

/// Build a runnable profile from accepted mappings.
pub fn build_profile(
    id: &str,
    display_name: &str,
    workflow: &Value,
    workflow_path: &str,
    mappings: &[(Role, Vec<MappedSlot>)],
) -> Result<ModelProfile, EmitError> {
    if !has_audio_save_node(workflow) {
        return Err(EmitError::NoSaveNode);
    }

    let mut inputs: BTreeMap<String, InputSpec> = BTreeMap::new();
    for (role, slots) in mappings {
        if slots.is_empty() {
            return Err(EmitError::NoSlots { role: *role });
        }
        inputs.insert(key_for(*role).to_string(), spec_for(*role, slots)?);
    }

    Ok(ModelProfile {
        id: id.to_string(),
        display_name: display_name.to_string(),
        kind: ModelKind::Music,
        // The app has no idea what someone's own graph is licensed under and
        // must not imply one. The field is required and shown wherever a model
        // is chosen, so it says exactly what is known.
        license: "Not declared (imported workflow)".to_string(),
        license_notes: None,
        comfy: ComfySpec {
            // Never both: `place_working_copy` refuses a profile declaring a
            // template and a workflow, and its test names this builder as the
            // thing that must not produce one.
            template: None,
            workflow: Some(workflow_path.to_string()),
            vram_gb_min: None,
            slot_overrides: BTreeMap::new(),
            models: Vec::new(),
            output: OutputSpec {
                save_node: SAVE_NODE.to_string(),
                prefer_lossless: true,
            },
        },
        loras: None,
        inputs,
        lyrics_contract: None,
        prompt_guide: None,
    })
}

/// The control one role's slots become.
fn spec_for(role: Role, slots: &[MappedSlot]) -> Result<InputSpec, EmitError> {
    let addresses: Vec<SlotAddress> = slots
        .iter()
        .map(|s| SlotAddress(s.address.clone()))
        .collect();
    let first = &slots[0];

    Ok(match role {
        Role::Tags | Role::Negative => InputSpec::Text {
            slots: addresses,
            // Their graph's own text is their prompt, so it is a default worth
            // keeping. This is the opposite of MCP-SURFACE 20.2, which is about
            // a *template's* demo text running invisibly under an empty box.
            default: first.current_value.as_str().map(str::to_string),
            label: Some(label_for(role).to_string()),
            advanced: false,
        },
        Role::Lyrics => InputSpec::Lyrics {
            slots: addresses,
            // **Never a default**, copying the shipped profile's own stated
            // reason: prefilled lyrics are words the app put in the user's
            // mouth.
            default: None,
            // Nothing in an imported graph publishes a structure-tag
            // vocabulary, and inventing one would make the lint lie.
            structure_tags: Vec::new(),
            label: Some(label_for(role).to_string()),
            advanced: false,
        },
        // No bounds needed, which is exactly why T-313c's hop to a
        // `PrimitiveInt` costs nothing here.
        Role::Seed => InputSpec::Seed { slots: addresses },
        Role::Steps => numeric(role, addresses, first, true)?,
        // By the **widget type**, not the role: ACE-Step's duration is a
        // FLOAT, another graph's may be an INT, and the graph decides.
        Role::DurationSeconds | Role::Cfg => {
            let integral = first.widget_type == "INT";
            numeric(role, addresses, first, integral)?
        }
    })
}

/// An `Int` or `Float` control, refusing rather than inventing limits.
fn numeric(
    role: Role,
    slots: Vec<SlotAddress>,
    first: &MappedSlot,
    integral: bool,
) -> Result<InputSpec, EmitError> {
    let bounds = first.bounds.clone().ok_or_else(|| EmitError::NoBounds {
        role,
        address: first.address.clone(),
    })?;
    let current = first.current_value.as_f64();
    Ok(if integral {
        let default = current.unwrap_or(bounds.min);
        InputSpec::Int {
            slots,
            min: bounds.min as i64,
            max: bounds.max as i64,
            default: default as i64,
            label: Some(label_for(role).to_string()),
            advanced: false,
        }
    } else {
        InputSpec::Float {
            slots,
            min: bounds.min,
            max: bounds.max,
            default: current.unwrap_or(bounds.min),
            step: bounds.step,
            label: Some(label_for(role).to_string()),
            advanced: false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graph_with_save() -> Value {
        json!({ "nodes": [{ "id": 107, "type": "SaveAudioMP3" }] })
    }

    fn slot(address: &str, widget_type: &str, current: Value) -> MappedSlot {
        MappedSlot {
            address: address.to_string(),
            widget_type: widget_type.to_string(),
            current_value: current,
            bounds: Some(Bounds {
                min: 1.0,
                max: 10000.0,
                step: None,
            }),
        }
    }

    fn build(mappings: &[(Role, Vec<MappedSlot>)]) -> Result<ModelProfile, EmitError> {
        build_profile(
            "my-import",
            "My Import",
            &graph_with_save(),
            "C:/workflows/my-import.json",
            mappings,
        )
    }

    /// Protects: an emitted profile loads through exactly the path a shipped
    /// one does.
    ///
    /// Serialized and read back with the real deserializer rather than
    /// inspected field by field -- a profile the app cannot read back is the
    /// whole failure mode of this task, and comparing structs in memory would
    /// never notice.
    #[test]
    fn test_an_emitted_profile_round_trips_as_a_model_profile() {
        let profile = build(&[
            (
                Role::Tags,
                vec![slot("94.tags", "STRING", json!("synthwave"))],
            ),
            (Role::Seed, vec![slot("109.value", "INT", json!(0))]),
        ])
        .expect("builds");

        let text = serde_json::to_string(&profile).expect("serializes");
        let back: ModelProfile =
            serde_json::from_str(&text).expect("a shipped-style profile loads");
        assert_eq!(back, profile);
    }

    /// Protects: pairs with `place_working_copy`'s
    /// `test_a_profile_declaring_both_is_refused`. That test refuses the shape;
    /// this one is the reason nothing produces it.
    #[test]
    fn test_an_emitted_profile_never_declares_both_sources() {
        let profile =
            build(&[(Role::Seed, vec![slot("109.value", "INT", json!(0))])]).expect("builds");
        assert_eq!(profile.comfy.template, None);
        assert_eq!(
            profile.comfy.workflow.as_deref(),
            Some("C:/workflows/my-import.json")
        );
    }

    /// Protects: the one deliberate asymmetry between the two text roles.
    ///
    /// Tags carry the user's own prompt forward; lyrics never do, because
    /// prefilled lyrics are words the app put in their mouth.
    #[test]
    fn test_lyrics_get_no_default_but_tags_do() {
        let profile = build(&[
            (
                Role::Tags,
                vec![slot("94.tags", "STRING", json!("late night trap"))],
            ),
            (
                Role::Lyrics,
                vec![slot("94.lyrics", "STRING", json!("[Verse] words"))],
            ),
        ])
        .expect("builds");

        match &profile.inputs["tags"] {
            InputSpec::Text { default, .. } => {
                assert_eq!(default.as_deref(), Some("late night trap"))
            }
            other => panic!("tags is {other:?}"),
        }
        match &profile.inputs["lyrics"] {
            InputSpec::Lyrics { default, .. } => assert_eq!(*default, None),
            other => panic!("lyrics is {other:?}"),
        }
    }

    /// Protects: the graph decides the control type, not the role.
    #[test]
    fn test_duration_follows_the_widget_type_not_the_role() {
        let as_float = build(&[(
            Role::DurationSeconds,
            vec![slot("94.duration", "FLOAT", json!(120.0))],
        )])
        .expect("builds");
        assert!(matches!(
            as_float.inputs["duration_s"],
            InputSpec::Float { .. }
        ));

        let as_int = build(&[(
            Role::DurationSeconds,
            vec![slot("37/13.max_duration", "INT", json!(90))],
        )])
        .expect("builds");
        assert!(matches!(as_int.inputs["duration_s"], InputSpec::Int { .. }));
    }

    /// Protects: no invented slider limits.
    ///
    /// A control whose range is a guess is worse than an absent one -- it
    /// looks authoritative and is not.
    #[test]
    fn test_a_numeric_role_without_bounds_is_refused_rather_than_guessed() {
        let mut unbounded = slot("3.steps", "INT", json!(8));
        unbounded.bounds = None;

        let err = build(&[(Role::Steps, vec![unbounded])]).expect_err("no bounds, no control");
        assert_eq!(
            err,
            EmitError::NoBounds {
                role: Role::Steps,
                address: "3.steps".to_string()
            }
        );
        assert!(err.to_string().contains("3.steps"), "{err}");
    }

    /// Protects: what makes T-313c's `PrimitiveInt` hop free.
    ///
    /// The seed is the one numeric role with no bounds at all, which is why
    /// mapping it to a primitive's bare `value` widget costs nothing.
    #[test]
    fn test_the_emitted_seed_needs_no_bounds() {
        let mut bare = slot("109.value", "INT", json!(0));
        bare.bounds = None;

        let profile = build(&[(Role::Seed, vec![bare])]).expect("a seed needs no limits");
        assert!(matches!(profile.inputs["seed"], InputSpec::Seed { .. }));
    }

    /// Protects: caught here, where it can still be explained, rather than at
    /// generate time as `GraphError::NoSaveNode`.
    #[test]
    fn test_a_graph_with_no_audio_save_node_is_refused() {
        let err = build_profile(
            "x",
            "X",
            &json!({ "nodes": [{ "id": 1, "type": "PreviewAudio" }] }),
            "C:/x.json",
            &[(Role::Seed, vec![slot("109.value", "INT", json!(0))])],
        )
        .expect_err("nothing to collect output from");

        assert_eq!(err, EmitError::NoSaveNode);
        assert!(err.to_string().contains("Save Audio"), "{err}");
    }

    /// Protects: a save node inside a subgraph counts.
    ///
    /// MiniMax's lives in one, and `ensure_lossless_output` rewrites both
    /// levels -- a top-level-only scan would refuse a graph that works.
    #[test]
    fn test_a_save_node_inside_a_subgraph_counts() {
        let nested = json!({
            "nodes": [{ "id": 1, "type": "UNETLoader" }],
            "definitions": { "subgraphs": [
                { "nodes": [{ "id": 35, "type": "SaveAudioAdvanced" }] }
            ]}
        });
        assert!(has_audio_save_node(&nested));
    }

    /// Protects: a role accepted but pointing nowhere is a bug in the caller,
    /// not an empty control.
    #[test]
    fn test_a_role_mapped_to_nothing_is_refused() {
        let err = build(&[(Role::Tags, Vec::new())]).expect_err("nothing to write to");
        assert_eq!(err, EmitError::NoSlots { role: Role::Tags });
    }
}
