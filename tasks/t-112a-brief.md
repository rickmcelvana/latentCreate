# T-112a: lyric-model suggestions, read as data
**Depends:** T-107a | **Crate/dir:** `crates/create-core`, `crates/library`, `data/`
**Files to create/modify:**
- `data/lyric-llms.json` (create)
- `crates/create-core/src/suggestions.rs` (create)
- `crates/library/src/suggestions.rs` (create)
- `crates/create-core/src/lib.rs`, `crates/library/src/lib.rs` (modify: one `mod`, one re-export each)

## Goal
The wizard's model suggestions, and how an installed tag is matched to one. Read
docs/MODELS.md's "Lyric-writing LLMs" section and [LLM-SURFACE](../docs/LLM-SURFACE.md) **11.5**
first.

## Spec
Exactly the reference implementation below.

**Suggestions are data, not code** (docs/MODELS.md). Models move fast and these are the owner's
hints, never a gate, so the list ships as JSON and the wizard reads it. The markdown table in
MODELS.md is the human-readable twin of `data/lyric-llms.json`; they change together.

**Three rules that are correctness, not taste:**

- **Matching cannot be equality.** MODELS.md suggests "Gemma 4 12B". The verification machine
  has *two* of them, `gemma4:12b-32k` and `gemma4:12b-it-qat`, and **neither is named
  `gemma4:12b`**. An equality match recommends nothing while the recommended model sits
  installed. Ids are matched by prefix.
- **The preselect is deterministic.** Because more than one variant can match, "first one seen"
  would make the preselected model vary with whatever order the endpoint returned. Lowest id
  wins.
- **A configured model always wins**, even when it matches no suggestion. That is the whole
  difference between a suggestion and a setting: a wizard that re-picks on every visit silently
  discards a deliberate choice. The exception is a configured model that is no longer installed,
  which falls back rather than pinning the picker to something unusable.

**`library::suggestions::load` never fails**, mirroring `library::profiles`. A missing file
yields no suggestions and one warning; losing the hints must not stop the wizard opening.

**The app never pulls an LLM.** `pull_command` is shown to the user to run themselves -- their
disk, their bandwidth (docs/MODELS.md).

## Reference implementation

### `data/lyric-llms.json` (create)
```json
{
  "source": "docs/MODELS.md",
  "note": "Suggestions for lyric writing, never a gate. The user's own choice always wins. Kept as data because models move fast and the wizard must not hardcode them.",
  "suggestions": [
    {
      "id_prefix": "gemma4:12b",
      "label": "Gemma 4 12B",
      "why": "Outperforms other models of its size for lyrics.",
      "vram_hint": "about 8-12 GB",
      "preselect": true,
      "pull_command": "ollama pull gemma4:12b"
    },
    {
      "id_prefix": "gemma4:26b",
      "label": "Gemma 4 26B",
      "why": "Also strong for lyrics if you have the VRAM.",
      "vram_hint": "about 24 GB and up",
      "preselect": false,
      "pull_command": "ollama pull gemma4:26b"
    },
    {
      "id_prefix": "gemma4:31b",
      "label": "Gemma 4 31B",
      "why": "Also strong for lyrics if you have the VRAM.",
      "vram_hint": "about 24 GB and up",
      "preselect": false,
      "pull_command": "ollama pull gemma4:31b"
    }
  ]
}
```

### `crates/create-core/src/suggestions.rs` (create)
```rust
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
    #[test]
    fn test_preselect_is_deterministic_when_several_variants_match() {
        let chosen = shipped().preselect(INSTALLED, None);
        assert_eq!(chosen, Some("gemma4:12b-32k"), "lowest matching id wins");
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
```

### `crates/library/src/suggestions.rs` (create)
```rust
//! Loading the shipped lyric-model suggestions.
//!
//! Mirrors [`crate::profiles`]: reads shipped data, **never fails**, and
//! reports what went wrong instead of refusing to start. A wizard that cannot
//! offer suggestions is a smaller problem than a wizard that will not open, and
//! these are hints, not requirements (docs/MODELS.md).

use create_core::suggestions::LyricLlmSuggestions;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The file the wizard reads its suggestions from.
pub const SUGGESTIONS_FILE: &str = "lyric-llms.json";

/// Why the suggestion list could not be read.
///
/// Surfaced rather than swallowed, so a packaging mistake is visible in the
/// diagnostics pane instead of silently costing every user their suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SuggestionWarning {
    /// The file is not where it should be. Normal in a build that ships none.
    Absent { path: String },
    /// It exists but could not be read.
    Unreadable { path: String, detail: String },
    /// It was read but is not a suggestion list.
    Malformed { path: String, detail: String },
}

/// The suggestions, plus anything that went wrong reading them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionSet {
    pub suggestions: LyricLlmSuggestions,
    #[serde(default)]
    pub warnings: Vec<SuggestionWarning>,
}

/// Read `lyric-llms.json` from `dir`. **Never fails.**
///
/// An absent file yields no suggestions and one warning: the wizard still
/// works, it just stops recommending anything.
pub fn load(dir: &Path) -> SuggestionSet {
    let path = dir.join(SUGGESTIONS_FILE);
    let shown = path.display().to_string();

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return SuggestionSet {
                suggestions: LyricLlmSuggestions::default(),
                warnings: vec![SuggestionWarning::Absent { path: shown }],
            };
        }
        Err(e) => {
            return SuggestionSet {
                suggestions: LyricLlmSuggestions::default(),
                warnings: vec![SuggestionWarning::Unreadable {
                    path: shown,
                    detail: e.to_string(),
                }],
            };
        }
    };

    match serde_json::from_str(&text) {
        Ok(suggestions) => SuggestionSet {
            suggestions,
            warnings: Vec::new(),
        },
        Err(e) => SuggestionSet {
            suggestions: LyricLlmSuggestions::default(),
            warnings: vec![SuggestionWarning::Malformed {
                path: shown,
                detail: e.to_string(),
            }],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protects: the shipped file loads from a real directory. An
    /// `include_str!` test would pass while the packaged app shipped nothing,
    /// because that checks the repo, not the resource directory.
    #[test]
    fn test_the_shipped_file_loads_from_disk() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let set = load(&dir);
        assert!(set.warnings.is_empty(), "warnings: {:?}", set.warnings);
        assert_eq!(set.suggestions.suggestions.len(), 3);
    }

    /// Protects: a missing file is a warning, not a failure. Suggestions are
    /// hints; losing them must never stop the wizard opening.
    #[test]
    fn test_an_absent_file_yields_no_suggestions_and_one_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        let set = load(dir.path());
        assert!(set.suggestions.suggestions.is_empty());
        assert!(matches!(
            set.warnings.as_slice(),
            [SuggestionWarning::Absent { .. }]
        ));
    }

    /// Protects: a corrupt file is reported, not swallowed. Silently offering
    /// no suggestions would look like the models simply are not installed.
    #[test]
    fn test_a_malformed_file_is_reported() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join(SUGGESTIONS_FILE), "{ not json").expect("write");
        let set = load(dir.path());
        assert!(set.suggestions.suggestions.is_empty());
        assert!(matches!(
            set.warnings.as_slice(),
            [SuggestionWarning::Malformed { .. }]
        ));
    }
}
```

### `crates/create-core/src/lib.rs` and `crates/library/src/lib.rs` (modify)
```diff
diff --git a/crates/create-core/src/lib.rs b/crates/create-core/src/lib.rs
index 11c451c..6592c9f 100644
--- a/crates/create-core/src/lib.rs
+++ b/crates/create-core/src/lib.rs
@@ -14,12 +14,14 @@ pub mod profile;
 pub mod project;
 pub mod provenance;
 pub mod readiness;
+pub mod suggestions;
 
 pub use generation::*;
 pub use profile::*;
 pub use project::*;
 pub use provenance::*;
 pub use readiness::*;
+pub use suggestions::*;
 
 #[cfg(test)]
 mod tests {
diff --git a/crates/library/src/lib.rs b/crates/library/src/lib.rs
index ef307a3..9961f20 100644
--- a/crates/library/src/lib.rs
+++ b/crates/library/src/lib.rs
@@ -6,6 +6,7 @@
 pub mod config;
 pub mod profiles;
 pub mod secrets;
+pub mod suggestions;
 
 /// Re-export of [`config::Config`].
 pub use config::Config;
@@ -24,6 +25,10 @@ pub use profiles::ProfileWarning;
 /// Re-export of [`secrets::SecretKey`].
 pub use secrets::SecretKey;
 
+pub use suggestions::SuggestionSet;
+
+pub use suggestions::SuggestionWarning;
+
 use thiserror::Error;
 
 /// Anything that can go wrong reading or writing the library.
```

## Acceptance criteria
- `npm run gate` green.
- `cargo test -p create-core` goes 34 -> **41**; `cargo test -p library` goes 19 -> **22**.
- **No non-ASCII characters anywhere in the diff.**

## Out of scope
Anything that talks to an endpoint (T-112b), and all frontend (T-112c/d).

## If unclear
Follow the reference implementation exactly.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read docs/MODELS.md --read docs/LLM-SURFACE.md --read crates/library/src/profiles.rs --file data/lyric-llms.json --file crates/create-core/src/suggestions.rs --file crates/library/src/suggestions.rs --file crates/create-core/src/lib.rs --file crates/library/src/lib.rs
```
