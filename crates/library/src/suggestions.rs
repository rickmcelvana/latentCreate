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
