//! The authoring guide one profile carries: what LyricsStudio prefills from.
//!
//! A profile's `prompt_guide` is the model author's own worked examples and tag
//! style, and the brief form prefills its style-tags field from the first
//! example. The two shipped profiles disagree about what a "style tag" even is
//! -- ACE-Step wants comma-separated short tags, MiniMax wants a structured
//! caption -- so the prefill must come from the profile, never a constant.

use create_core::profile::{ModelProfile, PromptExample};
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
}
