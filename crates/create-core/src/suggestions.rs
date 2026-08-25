//! Which lyric-writing models the wizard suggests, and how they are matched.
//!
//! Suggestions are **data, not code** (docs/MODELS.md): models move fast, and
//! the owner's picks are hints in the UI, never a gate. `library` loads the
//! shipped `data/lyric-llms.json`; this module decides what it means.
//!
//! **Matching cannot be equality.** MODELS.md suggests "Gemma 4 12B", and the
//! verification machine has two of them -- `gemma4:12b-32k` and
//! `gemma4:12b-it-qat` -- with neither named `gemma4:12b` (LLM-SURFACE 11.5).
//! Ids are matched by prefix, and because more than one can match, the
//! preselect is deterministic rather than "whichever came back first".

use serde::{Deserialize, Serialize};

/// One suggested lyric model, as `data/lyric-llms.json` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLlmSuggestion {
    /// Matched against a model id with `starts_with`, so one entry covers every
    /// quantisation and context variant the user might have pulled.
    pub id_prefix: String,
    /// Human name, e.g. `"Gemma 4 12B"`.
    pub label: String,
    /// Why it is suggested, shown next to the chip.
    #[serde(default)]
    pub why: Option<String>,
    /// Rough VRAM class, shown so a user can judge before pulling.
    #[serde(default)]
    pub vram_hint: Option<String>,
    /// Whether this is the one to preselect when the user has chosen nothing.
    #[serde(default)]
    pub preselect: bool,
    /// The command the **user** runs. This app never pulls an LLM: that is
    /// their disk and their bandwidth (docs/MODELS.md).
    #[serde(default)]
    pub pull_command: Option<String>,
}

impl LyricLlmSuggestion {
    /// Whether `model_id` is a variant of this suggestion.
    pub fn matches(&self, model_id: &str) -> bool {
        !self.id_prefix.is_empty() && model_id.starts_with(&self.id_prefix)
    }
}

/// The shipped suggestion list.
///
/// Unknown fields are ignored deliberately, so a list written for a newer build
/// still loads on an older one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLlmSuggestions {
    #[serde(default)]
    pub suggestions: Vec<LyricLlmSuggestion>,
}

impl LyricLlmSuggestions {
    /// The suggestion `model_id` matches, if any.
    ///
    /// First match wins, so the file's order is the precedence order.
    pub fn for_model(&self, model_id: &str) -> Option<&LyricLlmSuggestion> {
        self.suggestions.iter().find(|s| s.matches(model_id))
    }

    /// Which model to preselect out of `available`, given what is configured.
    ///
    /// **A configured model always wins**, even when it matches nothing here --
    /// the user's own choice is not a suggestion to be overridden. Otherwise
    /// the lowest id matching a `preselect` suggestion is taken, so two
    /// installed variants of the same model resolve the same way every time
    /// rather than following list order.
    ///
    /// Returns `None` when nothing is configured and nothing matches; the UI
    /// then leaves the picker unset rather than choosing arbitrarily.
    pub fn preselect<'a>(
        &self,
        available: &[&'a str],
        configured: Option<&'a str>,
    ) -> Option<&'a str> {
        if let Some(current) = configured {
            if available.contains(&current) {
                return Some(current);
            }
        }
        available
            .iter()
            .filter(|id| {
                self.for_model(id)
                    .is_some_and(|suggestion| suggestion.preselect)
            })
            .min()
            .copied()
    }

    /// Suggestions with nothing installed to satisfy them.
    ///
    /// These become help text with the user's own pull command. **Never a
    /// download**: this app does not pull an LLM.
    pub fn missing(&self, available: &[&str]) -> Vec<&LyricLlmSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| !available.iter().any(|id| s.matches(id)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = include_str!("../../../data/lyric-llms.json");

    fn shipped() -> LyricLlmSuggestions {
        serde_json::from_str(SHIPPED).expect("shipped suggestions decode")
    }

    /// The chat-capable ids on the verification machine, 2026-08-25.
    const INSTALLED: &[&str] = &[
        "deepseek-v4-flash:cloud",
        "gemma4:12b-32k",
        "gemma4:12b-it-qat",
        "qwen3.5:9b",
    ];

    /// Protects: matching is by prefix, not equality. The machine this was
    /// verified on has two Gemma 4 12B variants and neither is named
    /// `gemma4:12b`, so an equality match would recommend nothing at all while
    /// the recommended model sat installed (LLM-SURFACE 11.5).
    #[test]
    fn test_a_variant_tag_still_matches_its_suggestion() {
        let suggestions = shipped();
        let matched = suggestions
            .for_model("gemma4:12b-it-qat")
            .expect("a 12B variant matches the 12B suggestion");
        assert_eq!(matched.label, "Gemma 4 12B");
        assert!(suggestions.for_model("gemma4:12b-32k").is_some());
        assert!(suggestions.for_model("qwen3.5:9b").is_none());
    }

    /// Protects: two matching variants resolve the same way every time. List
    /// order from the endpoint is not stable, so "first one seen" would make
    /// the preselected model vary between runs on one machine.
    ///
    /// The reversed list is the point of this test. `INSTALLED` is already in
    /// sorted order, so against it "lowest id" and "whichever came back first"
    /// agree -- it cannot tell the two apart, and asserting only on it would
    /// leave the rule unguarded.
    #[test]
    fn test_preselect_is_deterministic_when_several_variants_match() {
        let suggestions = shipped();
        assert_eq!(
            suggestions.preselect(INSTALLED, None),
            Some("gemma4:12b-32k"),
            "lowest matching id wins"
        );

        let worst_first = ["gemma4:12b-it-qat", "gemma4:12b-32k"];
        assert_eq!(
            suggestions.preselect(&worst_first, None),
            Some("gemma4:12b-32k"),
            "list order must not change the answer"
        );
    }

    /// Protects: the user's own choice is never overridden. This is the whole
    /// difference between a suggestion and a setting -- a wizard that re-picks
    /// on every visit silently discards a deliberate decision.
    #[test]
    fn test_a_configured_model_always_wins() {
        let suggestions = shipped();
        assert_eq!(
            suggestions.preselect(INSTALLED, Some("qwen3.5:9b")),
            Some("qwen3.5:9b"),
            "configured model is kept even though it matches no suggestion"
        );
        assert_eq!(
            suggestions.preselect(INSTALLED, Some("gemma4:12b-it-qat")),
            Some("gemma4:12b-it-qat"),
            "and kept even when another variant would have been preselected"
        );
    }

    /// Protects: a configured model that is no longer installed does not pin
    /// the picker to something unusable. Falling back to a suggestion is right;
    /// keeping a dangling id is not.
    #[test]
    fn test_an_uninstalled_configured_model_falls_back() {
        let chosen = shipped().preselect(INSTALLED, Some("gemma4:26b"));
        assert_eq!(chosen, Some("gemma4:12b-32k"));
    }

    /// Protects: nothing matching means nothing chosen. Picking an arbitrary
    /// model for the user is worse than leaving the picker empty.
    #[test]
    fn test_no_match_preselects_nothing() {
        assert_eq!(shipped().preselect(&["qwen3.5:9b"], None), None);
        assert_eq!(shipped().preselect(&[], None), None);
    }

    /// Protects: suggestions with nothing installed become help text, and the
    /// pull command reaches the user verbatim so it can be copied. The app
    /// never runs it -- that is the user's disk and bandwidth (MODELS.md).
    #[test]
    fn test_uninstalled_suggestions_carry_a_pull_command_for_the_user() {
        let suggestions = shipped();
        let missing = suggestions.missing(INSTALLED);
        assert_eq!(missing.len(), 2, "26B and 31B are not installed here");
        assert!(missing.iter().all(|s| s.pull_command.is_some()));
        assert!(missing.iter().all(|s| !s.label.contains("12B")));

        assert!(
            suggestions.missing(&["gemma4:12b-32k"]).len() == 2,
            "an installed 12B variant satisfies the 12B suggestion"
        );
    }

    /// Protects: the shipped file stays loadable and keeps exactly one
    /// preselect. Two would make the choice order-dependent again.
    #[test]
    fn test_the_shipped_list_has_exactly_one_preselect() {
        let suggestions = shipped();
        assert_eq!(suggestions.suggestions.len(), 3);
        assert_eq!(
            suggestions
                .suggestions
                .iter()
                .filter(|s| s.preselect)
                .count(),
            1
        );
    }
}
