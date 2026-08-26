//! The consent-gated prompt optimizer's own prompts.
//!
//! **What is optimized is the assembled brief, not the lyrics.** The user
//! message this app sends is a list of labelled lines (see
//! [`super::assemble_user_message`]), and that shape was chosen partly so this
//! diff would read well -- a paragraph rewritten into another paragraph diffs
//! into noise.
//!
//! **Nothing here is ever applied on its own.** The rewrite comes back, is
//! shown beside the original as a word diff, and only the text the user accepts
//! is sent or stored (ARCHITECTURE 6). That consent step is also the only real
//! guard on the rules below: a model that ignores "reproduce this line exactly"
//! produces a visible, highlighted change the user has to accept before it goes
//! anywhere.
//!
//! **This prompt has not been measured live.** The lyric prompt in the parent
//! module was captured working before it was written down, and the repo's rule
//! is that a prompt change is a change to a third-party surface and gets
//! measured like one (LLM-SURFACE 12.5). T-211 is where this one meets a real
//! model; until then it is a first draft, not a verified surface.

use crate::profile::ModelProfile;

/// Completion budget for one optimizer call.
///
/// A brief is roughly 100 tokens and a sharpened one is not much longer, so
/// this is mostly headroom for an endpoint the app could not enrich, where
/// `reasoning_effort` is deliberately not sent and the model may think first
/// (LLM-SURFACE 12.2). Truncation is reported rather than hidden -- see
/// `src-tauri/src/optimize.rs`.
pub const OPTIMIZER_MAX_TOKENS: u32 = 1024;

/// The brief lines the optimizer may rewrite: the creative half.
pub const REWRITABLE_LABELS: [&str; 4] = [
    "Theme",
    "Genre and style tags",
    "Mood",
    "Era and references",
];

/// The brief lines the optimizer must reproduce word for word.
///
/// These are settings the form owns, not prose. Structure feeds the lint,
/// duration feeds the token budget, and language and the explicit flag are
/// answers the user gave rather than material to improve.
pub const FIXED_LABELS: [&str; 5] = [
    "Structure",
    "Language",
    "Point of view",
    "Explicit content allowed",
    "Target duration",
];

/// Builds the optimizer's system prompt for one profile.
///
/// The profile is named for the same reason the lyric prompt names it: "more
/// specific style tags" means something different for a model that wants
/// comma-separated tags than for one that wants a written caption, and the
/// profile's own `tag_style` is the only place that difference is recorded.
pub fn optimizer_system_prompt(profile: &ModelProfile) -> String {
    let mut out = format!(
        "You are a prompt engineer sharpening a song brief for {}.\n",
        profile.display_name
    );
    out.push_str(
        "The brief is a list of labelled lines. Rewrite it so a songwriter would know exactly what to write: concrete imagery, specific detail, the same song the user asked for.\n",
    );

    if let Some(tag_style) = profile
        .prompt_guide
        .as_ref()
        .and_then(|guide| guide.tag_style.as_ref())
    {
        out.push_str(&format!("Style tags for this model are {tag_style}.\n"));
    }

    out.push_str("\nHard rules:\n");
    out.push_str(
        "- Output ONLY the rewritten brief. No commentary, no explanation, no markdown code fences.\n",
    );
    out.push_str("- Keep every label exactly as written, one line each, in the same order.\n");
    out.push_str(&format!(
        "- Rewrite only these lines: {}.\n",
        REWRITABLE_LABELS.join(", ")
    ));
    out.push_str(&format!(
        "- Reproduce these lines exactly as given, word for word: {}.\n",
        FIXED_LABELS.join(", ")
    ));
    out.push_str("- Add no labels and drop none.\n");
    out.push_str(
        "- Keep the user's own subject. Sharpen the brief; do not write a different song.\n",
    );
    out
}

/// Trims the model's answer down to the brief it was asked for.
///
/// Only two things are removed: surrounding whitespace, and a markdown fence
/// wrapped around the whole answer. **Stray commentary is deliberately left
/// in.** Guessing which lines are commentary would mean editing text the user
/// is about to review, and a rewrite that arrives with an apology attached is
/// something they should see before accepting it.
pub fn clean_optimized(raw: &str) -> String {
    let trimmed = raw.trim();
    let lines: Vec<&str> = trimmed.lines().collect();
    let fenced = lines.len() >= 2
        && lines
            .first()
            .is_some_and(|line| line.trim_start().starts_with("```"))
        && lines.last().is_some_and(|line| line.trim() == "```");
    if fenced {
        return lines[1..lines.len() - 1].join("\n").trim().to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::{assemble_user_message, LyricBrief};

    fn ace() -> ModelProfile {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("profiles")
            .join("ace-step-1.5-turbo.json");
        let text = std::fs::read_to_string(&path).expect("shipped profile is readable");
        serde_json::from_str(&text).expect("shipped profile parses")
    }

    /// Invariant: the optimizer's two label lists cover every line the brief can
    /// emit. A field added to `LyricBrief` without a decision about whether the
    /// optimizer may touch it would otherwise reach the model with no rule at
    /// all, and the first sign of it would be a silently rewritten setting.
    #[test]
    fn test_every_brief_label_is_classified_as_rewritable_or_fixed() {
        let brief = LyricBrief {
            era_refs: Some("early Chromatics".to_string()),
            ..LyricBrief::default()
        };
        let message = assemble_user_message(&brief);
        assert!(message.lines().count() >= 9, "{message}");

        for line in message.lines() {
            let (label, _) = line
                .split_once(':')
                .unwrap_or_else(|| panic!("every brief line is labelled: {line}"));
            assert!(
                REWRITABLE_LABELS.contains(&label) || FIXED_LABELS.contains(&label),
                "the brief line {label:?} is neither rewritable nor fixed"
            );
        }
    }

    /// Invariant: no label is on both lists. A line the prompt calls rewritable
    /// and fixed in the same breath is an instruction the model cannot follow.
    #[test]
    fn test_no_label_is_both_rewritable_and_fixed() {
        for label in REWRITABLE_LABELS {
            assert!(!FIXED_LABELS.contains(&label), "{label} is on both lists");
        }
    }

    /// Invariant: the settings the form owns are named as untouchable, and the
    /// model is told to emit the brief and nothing else. Both are what make the
    /// answer diffable against the original.
    #[test]
    fn test_prompt_names_the_profile_and_protects_the_settings() {
        let prompt = optimizer_system_prompt(&ace());
        assert!(
            prompt.starts_with(
                "You are a prompt engineer sharpening a song brief for ACE-Step 1.5 XL Turbo.\n"
            ),
            "{prompt}"
        );
        assert!(
            prompt.contains("Output ONLY the rewritten brief"),
            "{prompt}"
        );
        for label in FIXED_LABELS {
            assert!(
                prompt.contains(label),
                "the prompt must name {label} as fixed: {prompt}"
            );
        }
    }

    /// Invariant: a profile's own tag style reaches the optimizer, and a profile
    /// without one still yields a usable prompt. "Better style tags" means a
    /// different thing per model, and only the profile records which.
    #[test]
    fn test_tag_style_comes_from_the_profile_when_it_has_one() {
        let profile = ace();
        let tag_style = profile
            .prompt_guide
            .as_ref()
            .and_then(|guide| guide.tag_style.clone())
            .expect("the shipped ACE-Step profile carries a tag style");
        assert!(optimizer_system_prompt(&profile).contains(&tag_style));

        let mut bare = profile;
        bare.prompt_guide = None;
        let prompt = optimizer_system_prompt(&bare);
        assert!(!prompt.contains("Style tags for this model"), "{prompt}");
        assert!(prompt.contains("Hard rules:"), "{prompt}");
    }

    /// Invariant: a fenced answer is unwrapped, and an unfenced one is returned
    /// as written. A leading fence would otherwise become the first line of the
    /// prompt the user is asked to accept.
    #[test]
    fn test_clean_optimized_unwraps_a_fence_and_leaves_plain_text_alone() {
        assert_eq!(
            clean_optimized("```text\nTheme: a night drive\nMood: bittersweet\n```"),
            "Theme: a night drive\nMood: bittersweet"
        );
        assert_eq!(
            clean_optimized("  \nTheme: a night drive\n\n"),
            "Theme: a night drive"
        );
        assert_eq!(clean_optimized("   "), "");
    }

    /// Invariant: commentary survives cleaning. Stripping the lines that look
    /// like an apology means editing text before the user reads it, and the
    /// diff is what that text exists to be read in.
    #[test]
    fn test_clean_optimized_keeps_commentary_for_the_user_to_see() {
        let raw = "Here is the improved brief:\nTheme: a night drive";
        assert_eq!(clean_optimized(raw), raw);
    }
}
