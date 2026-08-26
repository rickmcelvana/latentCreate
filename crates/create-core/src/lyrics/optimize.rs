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
//! **Measured 2026-08-26 (T-211), and it holds.** Five runs against
//! `gemma4:12b-32k`, the model this app recommends for lyrics: the rewrite came
//! back as a well-formed brief in **5 of 5**, and the five [`FIXED_LABELS`]
//! lines were reproduced word for word in **5 of 5**. No truncation at
//! [`OPTIMIZER_MAX_TOKENS`], no commentary, no fences, 3.4-3.6 s per call. The
//! rules below are therefore kept as written, and changing them means measuring
//! again -- a prompt is a third-party surface in this repo (LLM-SURFACE 12.5).
//! The harness is `test_live_optimizer_returns_a_brief_and_reports_what_it_altered`
//! in `src-tauri/src/optimize.rs`.
//!
//! One behaviour the measurement found and this module accepts: the model
//! **adds** an `Era and references` line when the brief leaves that field
//! empty, in 5 of 5 runs. It is a rewritable label, the line is well-formed,
//! and it reaches the user as an added line in the diff. See [`LabelReport`].

use crate::profile::ModelProfile;

/// Completion budget for one optimizer call.
///
/// A brief is roughly 100 tokens and a sharpened one is not much longer, so
/// this is mostly headroom for an endpoint the app could not enrich, where
/// `reasoning_effort` is deliberately not sent and the model may think first
/// (LLM-SURFACE 12.2). Truncation is reported rather than hidden -- see
/// `src-tauri/src/optimize.rs`.
///
/// Measured adequate: no truncation across 5 live runs (T-211).
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

/// The label of every labelled line, in the order the lines appear.
///
/// A **measurement** helper, used by T-211's live check to answer "did the
/// rewrite come back as the same brief". Lines with no colon contribute
/// nothing: commentary the model added is not a label, and `clean_optimized`
/// deliberately leaves it in place.
pub fn labels_in_order(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(label, _)| label.trim().to_string())
        .collect()
}

/// How a rewrite's labelled lines differ from the brief's.
///
/// **A label the rewrite added is deliberately not a finding**, as long as the
/// brief could have carried it. Measured 2026-08-26: `gemma4:12b-32k` added an
/// `Era and references` line in **5 of 5** runs on the default brief, which
/// leaves that field empty. Era is on [`REWRITABLE_LABELS`], the added line is
/// well-formed, and it appears in the diff as an added line the user accepts or
/// reverts -- that is the optimizer adding specificity to a field the user left
/// blank, which is what it is for. The first version of this check tested the
/// label list for equality and called all five runs a failure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LabelReport {
    /// Labels the brief carried that the rewrite dropped.
    pub missing: Vec<String>,
    /// Labels the rewrite invented that no brief can carry. These are the ones
    /// that make an answer undiffable against the original.
    pub unknown: Vec<String>,
    /// Whether the labels present in both appear in a different relative order.
    pub reordered: bool,
}

impl LabelReport {
    /// True when the rewrite came back as a brief: nothing dropped, nothing
    /// invented, nothing shuffled.
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.unknown.is_empty() && !self.reordered
    }
}

/// Compare a rewrite's labelled lines with the brief's.
///
/// A measurement, like [`altered_fixed_lines`] -- nothing rejects a rewrite on
/// its findings.
pub fn label_report(original: &str, optimized: &str) -> LabelReport {
    let before = labels_in_order(original);
    let after = labels_in_order(optimized);
    let known = |label: &String| {
        REWRITABLE_LABELS.contains(&label.as_str()) || FIXED_LABELS.contains(&label.as_str())
    };

    let missing: Vec<String> = before
        .iter()
        .filter(|label| !after.contains(label))
        .cloned()
        .collect();
    let unknown: Vec<String> = after
        .iter()
        .filter(|label| !known(label))
        .cloned()
        .collect();

    // Order is judged only on the labels both texts carry, so an added line
    // does not read as a shuffle.
    let shared_before: Vec<&String> = before.iter().filter(|l| after.contains(l)).collect();
    let shared_after: Vec<&String> = after.iter().filter(|l| before.contains(l)).collect();

    LabelReport {
        missing,
        unknown,
        reordered: shared_before != shared_after,
    }
}

/// Which [`FIXED_LABELS`] lines the rewrite failed to reproduce.
///
/// **This is a measurement, not a gate.** Nothing in the app calls it to reject
/// a rewrite: a changed settings line is already a highlighted change the user
/// has to accept, and a second enforcement point would be a second answer to a
/// question the consent step answers (PROJECT.md, 2026-08-26). It exists so
/// T-211 can put a number on how often the prompt's own rules hold.
///
/// A line is "reproduced" when its whole text matches after trimming --
/// trailing whitespace is not a change anyone means. A label present in one
/// text and absent from the other counts as altered, because dropping
/// `Target duration` changes the request as surely as rewriting it. Line
/// **order** is deliberately not considered here; [`labels_in_order`] is the
/// question about order.
pub fn altered_fixed_lines(original: &str, optimized: &str) -> Vec<String> {
    FIXED_LABELS
        .iter()
        .filter(|label| labelled_line(original, label) != labelled_line(optimized, label))
        .map(|label| (*label).to_string())
        .collect()
}

/// The whole line carrying `label`, trimmed, if the text has one.
fn labelled_line<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    text.lines().map(str::trim).find(|line| {
        line.split_once(':')
            .is_some_and(|(found, _)| found == label)
    })
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

    /// Invariant: the measurement reports a settings line the rewrite changed
    /// or dropped, and stays quiet about one it merely reformatted. A helper
    /// that cried wolf over a trailing space would make T-211's number
    /// meaningless; one blind to a dropped line would hide the failure that
    /// matters most.
    #[test]
    fn test_altered_fixed_lines_reports_real_changes_only() {
        let original = "Theme: a night drive\nStructure: V-C-V-C-B-C (Verse, Chorus)\nLanguage: English\nPoint of view: first person\nExplicit content allowed: no\nTarget duration: 120 seconds";

        assert!(altered_fixed_lines(original, original).is_empty());

        let creative_only = original.replace("a night drive", "a rain-slick night drive");
        assert!(
            altered_fixed_lines(original, &creative_only).is_empty(),
            "rewriting the theme is what the optimizer is for"
        );

        let padded = original.replace("Language: English", "Language: English   ");
        assert!(
            altered_fixed_lines(original, &padded).is_empty(),
            "trailing whitespace is not a change anyone means"
        );

        let retimed = original.replace("120 seconds", "180 seconds");
        assert_eq!(altered_fixed_lines(original, &retimed), ["Target duration"]);

        let dropped = original.replace("Point of view: first person\n", "");
        assert_eq!(altered_fixed_lines(original, &dropped), ["Point of view"]);
    }

    /// Invariant: the report is quiet about the one thing the live run actually
    /// does. `gemma4:12b-32k` added an `Era and references` line in 5 of 5 runs
    /// on the default brief; that is a rewritable label on a field the user left
    /// blank, and calling it a failure -- which the first version of this check
    /// did -- turns the measurement into noise.
    #[test]
    fn test_label_report_allows_an_added_rewritable_line() {
        let brief = assemble_user_message(&LyricBrief::default());
        assert!(!brief.contains("Era and references"), "{brief}");

        let with_era = brief.replace(
            "Structure:",
            "Era and references: 1980s midnight aesthetic\nStructure:",
        );
        assert_eq!(label_report(&brief, &with_era), LabelReport::default());
        assert!(label_report(&brief, &with_era).is_clean());
    }

    /// Invariant: the three failures that do make a rewrite undiffable are each
    /// reported, and reported separately -- a dropped line, an invented label,
    /// and a shuffle are different problems with different answers.
    #[test]
    fn test_label_report_names_dropped_invented_and_shuffled_labels() {
        let brief = assemble_user_message(&LyricBrief::default());

        let dropped = brief.replace("Language: English\n", "");
        assert_eq!(label_report(&brief, &dropped).missing, ["Language"]);

        let invented = format!("{brief}Bpm: 105\n");
        let report = label_report(&brief, &invented);
        assert_eq!(report.unknown, ["Bpm"]);
        assert!(report.missing.is_empty(), "nothing was dropped");

        let shuffled = format!(
            "Mood: bittersweet, hopeful\nTheme: a night drive\n{}",
            brief
                .lines()
                .filter(|l| !l.starts_with("Theme:") && !l.starts_with("Mood:"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(label_report(&brief, &shuffled).reordered);
    }

    /// Invariant: the labels come back in the order they appear, and prose the
    /// model added is not mistaken for one. Order is the other half of "did it
    /// come back as the same brief", and a colon in a sentence is not a label.
    #[test]
    fn test_labels_in_order_reads_the_brief_and_not_the_prose() {
        let brief = assemble_user_message(&LyricBrief::default());
        assert_eq!(
            labels_in_order(&brief),
            [
                "Theme",
                "Genre and style tags",
                "Mood",
                "Structure",
                "Language",
                "Point of view",
                "Explicit content allowed",
                "Target duration",
            ]
        );

        let with_commentary = format!("Sure, here you go\n{brief}");
        assert_eq!(
            labels_in_order(&with_commentary),
            labels_in_order(&brief),
            "a line without a colon is not a label"
        );
    }
}
