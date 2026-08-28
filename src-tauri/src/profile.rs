//! The authoring guide one profile carries: what LyricsStudio prefills from.
//!
//! A profile's `prompt_guide` is the model author's own worked examples and tag
//! style, and the brief form prefills its style-tags field from the first
//! example. The two shipped profiles disagree about what a "style tag" even is
//! -- ACE-Step wants comma-separated short tags, MiniMax wants a structured
//! caption -- so the prefill must come from the profile, never a constant.

use std::collections::BTreeMap;

use create_core::profile::{InputSpec, ModelProfile, PromptExample};
use serde::Serialize;
use tauri::State as TauriState;
use tauri::State;

use crate::{ConfigDir, ProfilesDir};

/// One worked example, for the prefill and for the LLM system prompt.
#[derive(Debug, Clone, Serialize)]
pub struct PromptExampleView {
    pub tags: String,
    pub lyrics: Option<String>,
}

/// What the brief form needs from a profile's `prompt_guide`.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileGuideView {
    /// Shown so the user knows which model the lyric is being written for.
    pub display_name: String,
    /// Hint for the style-tags field, e.g. "comma-separated short tags".
    pub tag_style: Option<String>,
    /// Worked examples; the first one's `tags` prefills the form.
    pub examples: Vec<PromptExampleView>,
}

/// The selected profile's authoring guide, or `None` when the profile does not
/// exist.
///
/// A profile with no `prompt_guide` is a model the form can still write for, so
/// that case is an empty guide, not `None` -- only an unknown id is `None`.
#[tauri::command]
pub fn profile_guide(
    profiles_dir: State<'_, ProfilesDir>,
    config_dir: State<'_, ConfigDir>,
    profile_id: String,
) -> Option<ProfileGuideView> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    set.profiles
        .get(&profile_id)
        .map(|loaded| guide_view(&loaded.profile))
}

/// The selected profile's declared inputs, or `None` for an unknown id.
///
/// Returned **as the profile writes them**, with no view type in between.
/// `InputSpec` is `#[serde(tag = "type")]`, so it serialises to exactly the
/// shape already sitting in `profiles/*.json`: a second projection here would
/// be a copy of the schema, free to drift from it, and the panel is built to
/// render declarations rather than a flattened summary of them.
///
/// In particular an `Unsupported` input survives the trip **with its reason**.
/// That reason is evidence -- somebody read a live node schema and recorded
/// that ACE-Step has no negative prompt -- and a view type that dropped it
/// would leave a missing control looking exactly like a forgotten one.
#[tauri::command]
pub fn profile_inputs(
    profiles_dir: State<'_, ProfilesDir>,
    config_dir: State<'_, ConfigDir>,
    profile_id: String,
) -> Option<BTreeMap<String, InputSpec>> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    set.profiles
        .get(&profile_id)
        .map(|loaded| loaded.profile.inputs.clone())
}

/// Project one profile into its guide view, empty when it has no guide.
fn guide_view(profile: &ModelProfile) -> ProfileGuideView {
    let guide = profile.prompt_guide.as_ref();
    ProfileGuideView {
        display_name: profile.display_name.clone(),
        tag_style: guide.and_then(|g| g.tag_style.clone()),
        examples: guide
            .map(|g| g.examples.iter().map(example_view).collect())
            .unwrap_or_default(),
    }
}

fn example_view(example: &PromptExample) -> PromptExampleView {
    PromptExampleView {
        tags: example.tags.clone(),
        lyrics: example.lyrics.clone(),
    }
}

/// Live options for one `from_node_choices` enum.
///
/// A tagged union rather than an optional list, because the three situations
/// read completely differently to a user and the backend is the half that can
/// tell them apart (the same rule [`crate::comfy::ComfyStatus`] follows).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EnumOptions {
    /// Read from the node registry.
    Loaded {
        choices: Vec<String>,
        /// Whether comfy-cli answered from its cache. **Tri-state**: `None`
        /// means the response did not say, which is not the same as fresh.
        stale: Option<bool>,
        /// The `object_info_stale` warning text, when there was one.
        note: Option<String>,
    },
    /// The profile asks for live choices but names no node class, so there is
    /// nothing to ask. A profile-authoring mistake, surfaced rather than
    /// guessed at.
    Undeclared,
    /// ComfyUI could not be asked at all.
    Unavailable { detail: String },
}

/// Which node class and input each `from_node_choices` enum needs.
///
/// Pure, and separate from the call so the mapping is testable without a
/// backend. The input name is the **field part of the first slot address** --
/// `"94.keyscale"` is instance 94's `keyscale` -- which is why only the class
/// had to be added to the schema.
fn enum_requests(profile: &ModelProfile) -> BTreeMap<String, Option<(String, String)>> {
    let mut wanted = BTreeMap::new();
    collect_enums(&profile.inputs, &mut wanted);
    wanted
}

fn collect_enums(
    inputs: &BTreeMap<String, InputSpec>,
    into: &mut BTreeMap<String, Option<(String, String)>>,
) {
    for (name, spec) in inputs {
        match spec {
            InputSpec::Group { members, .. } => collect_enums(members, into),
            InputSpec::Enum {
                slots,
                from_node_choices: true,
                node,
                ..
            } => {
                let field = slots
                    .first()
                    .and_then(|a| a.0.rsplit_once('.'))
                    .map(|(_, field)| field.to_string());
                into.insert(name.clone(), node.clone().zip(field));
            }
            _ => {}
        }
    }
}

/// Live choices for every `from_node_choices` enum the profile declares.
///
/// One schema read per distinct node class, not per input: ACE-Step's key
/// scale, time signature and language all live on
/// `TextEncodeAceStepAudio1.5`.
#[tauri::command]
pub async fn enum_choices(
    state: TauriState<'_, crate::jobs::ComfyState>,
    profiles_dir: TauriState<'_, ProfilesDir>,
    config_dir: TauriState<'_, ConfigDir>,
    profile_id: String,
) -> Result<BTreeMap<String, EnumOptions>, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    let Some(loaded) = set.profiles.get(&profile_id) else {
        return Ok(BTreeMap::new());
    };
    let wanted = enum_requests(&loaded.profile);

    let Some(comfy) = state.connected().await else {
        return Ok(wanted
            .into_keys()
            .map(|name| {
                (
                    name,
                    EnumOptions::Unavailable {
                        detail: "ComfyUI is not connected.".to_string(),
                    },
                )
            })
            .collect());
    };

    let mut schemas: BTreeMap<String, Result<mcp_bridge::NodeSchema, String>> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for (name, request) in wanted {
        let Some((class, field)) = request else {
            out.insert(name, EnumOptions::Undeclared);
            continue;
        };
        if !schemas.contains_key(&class) {
            let read = comfy.node_schema(&class).await.map_err(|e| e.to_string());
            schemas.insert(class.clone(), read);
        }
        out.insert(name, options_for(&schemas[&class], &field));
    }
    Ok(out)
}

/// Project one schema read into what the panel shows for one input.
fn options_for(read: &Result<mcp_bridge::NodeSchema, String>, field: &str) -> EnumOptions {
    match read {
        Err(detail) => EnumOptions::Unavailable {
            detail: detail.clone(),
        },
        Ok(schema) => match schema.choices_for(field) {
            None => EnumOptions::Unavailable {
                detail: format!("{} has no input named {field}.", schema.name),
            },
            Some(choices) => EnumOptions::Loaded {
                choices: choices.to_vec(),
                stale: schema.stale,
                note: schema
                    .warnings
                    .iter()
                    .find(|w| w.code == "object_info_stale")
                    .map(|w| w.message.clone()),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_core::profile::ModelProfile;

    const ACE: &str = include_str!("../../profiles/ace-step-1.5-turbo.json");

    /// Protects: the prefill comes from the profile's own example, not a
    /// constant. The two shipped profiles disagree about what a style tag is,
    /// so a hardcoded prefill would tell a MiniMax user to write ACE-Step tags.
    #[test]
    fn test_guide_view_reads_the_shipped_example() {
        let profile: ModelProfile = serde_json::from_str(ACE).expect("profile decodes");
        let view = guide_view(&profile);

        assert_eq!(view.display_name, "ACE-Step 1.5 XL Turbo");
        assert!(!view.examples.is_empty());
        assert!(view.examples[0].tags.contains("synthwave"));
        assert!(view.tag_style.is_some());
    }

    /// Protects: a profile with no guide yields an empty guide, not `None`. The
    /// form falls back to the default prefills either way, but the display name
    /// still reaches the user.
    #[test]
    fn test_guide_view_is_empty_when_the_profile_has_no_guide() {
        let mut profile: ModelProfile = serde_json::from_str(ACE).expect("profile decodes");
        profile.prompt_guide = None;

        let view = guide_view(&profile);
        assert_eq!(view.display_name, "ACE-Step 1.5 XL Turbo");
        assert!(view.examples.is_empty());
        assert_eq!(view.tag_style, None);
    }
    /// Protects: the wire shape the webview's `InputSpec` union is written
    /// against.
    ///
    /// `app/src/bridge/profiles.ts` mirrors this enum by hand -- there is no
    /// generator -- so the two can drift silently and the panel would render a
    /// control it does not understand. Every variant the shipped profile uses
    /// is pinned here by the exact key the TypeScript expects.
    #[test]
    fn test_declared_inputs_serialise_in_the_shape_the_webview_expects() {
        let profile: ModelProfile = serde_json::from_str(ACE).expect("profile decodes");
        let wire = serde_json::to_value(&profile.inputs).expect("inputs serialise");

        assert_eq!(wire["tags"]["type"], "text");
        assert_eq!(wire["seed"]["type"], "seed");
        assert_eq!(wire["bpm"]["type"], "int");
        assert_eq!(wire["bpm"]["default"], 120);
        assert_eq!(wire["duration_s"]["type"], "float");
        assert_eq!(wire["keyscale"]["type"], "enum");
        assert_eq!(wire["keyscale"]["from_node_choices"], true);
        assert_eq!(wire["planner"]["type"], "group");
        assert_eq!(wire["planner"]["advanced"], true);
        assert_eq!(wire["planner"]["members"]["cfg_scale"]["type"], "float");
    }

    /// Protects: an unsupported input crosses the bridge carrying its reason.
    ///
    /// "TextEncodeAceStepAudio1.5 exposes no negative input" is a fact somebody
    /// checked against a live node schema. Strip it in a view type and the
    /// panel can no longer tell a verified absence from an oversight -- the
    /// two look identical on screen.
    #[test]
    fn test_an_unsupported_input_keeps_its_reason_across_the_bridge() {
        let profile: ModelProfile = serde_json::from_str(ACE).expect("profile decodes");
        let wire = serde_json::to_value(&profile.inputs).expect("inputs serialise");

        assert_eq!(wire["negative"]["type"], "unsupported");
        let reason = wire["negative"]["reason"]
            .as_str()
            .expect("a recorded reason");
        assert!(reason.contains("no negative"), "reason was: {reason}");
    }

    /// Protects: every live enum resolves to a node class and an input name.
    ///
    /// The class had to be added to the schema because a slot address names a
    /// node *instance* (`"94.keyscale"`), and nothing outside the workflow file
    /// turns 94 into `TextEncodeAceStepAudio1.5`. The field part was already
    /// there, which is why that is the half this reads from the address.
    #[test]
    fn test_every_live_enum_names_a_class_and_an_input() {
        let profile: ModelProfile = serde_json::from_str(ACE).expect("profile decodes");
        let wanted = enum_requests(&profile);

        assert_eq!(
            wanted.keys().collect::<Vec<_>>(),
            vec!["keyscale", "language", "timesignature"]
        );
        assert_eq!(
            wanted["keyscale"],
            Some((
                "TextEncodeAceStepAudio1.5".to_string(),
                "keyscale".to_string()
            ))
        );
        assert!(
            wanted.values().all(|r| r.is_some()),
            "a live enum with no class would render an empty picker"
        );
    }

    /// Protects: a profile that asks for live choices without naming a class
    /// says so instead of being guessed at.
    #[test]
    fn test_a_live_enum_with_no_class_is_reported_not_guessed() {
        let mut profile: ModelProfile = serde_json::from_str(ACE).expect("profile decodes");
        profile.inputs.insert(
            "mystery".to_string(),
            InputSpec::Enum {
                slots: vec![create_core::profile::SlotAddress("12.mystery".to_string())],
                from_node_choices: true,
                node: None,
                choices: vec![],
                label: None,
                advanced: false,
            },
        );

        assert_eq!(enum_requests(&profile)["mystery"], None);
    }

    /// Protects: a cached schema is reported as cached, all the way to the UI.
    ///
    /// `nodes(action="get")` succeeds with ComfyUI down -- comfy-cli answers
    /// from its own cache and flags it. If that flag stops here, the panel
    /// shows a cached list as the installed one. Harmless for key signatures;
    /// the same path feeds the LoRA picker in T-309, where a cached list offers
    /// LoRAs the user deleted and picking one writes a track with no LoRA on it
    /// rather than failing (MCP-SURFACE 17.6).
    #[test]
    fn test_a_cached_schema_reaches_the_panel_as_cached() {
        let captured: mcp_bridge::NodeSchema = serde_json::from_str(include_str!(
            "../../testdata/mcp/nodes.LoraLoaderModelOnly.json"
        ))
        .expect("the captured schema decodes");

        let options = options_for(&Ok(captured), "lora_name");
        match options {
            EnumOptions::Loaded {
                choices,
                stale,
                note,
            } => {
                assert_eq!(choices.len(), 53);
                assert_eq!(stale, Some(true));
                assert!(note.expect("the reason").contains("cannot reach"));
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    /// Protects: an input the class does not have is a reported failure, not an
    /// empty list. An empty list is indistinguishable from "this model offers
    /// no choices", which is a different thing entirely.
    #[test]
    fn test_an_unknown_input_name_is_unavailable_not_empty() {
        let captured: mcp_bridge::NodeSchema = serde_json::from_str(include_str!(
            "../../testdata/mcp/nodes.LoraLoaderModelOnly.json"
        ))
        .expect("the captured schema decodes");

        match options_for(&Ok(captured), "keyscale") {
            EnumOptions::Unavailable { detail } => assert!(detail.contains("keyscale")),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
}
