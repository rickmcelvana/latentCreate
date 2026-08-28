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
}
