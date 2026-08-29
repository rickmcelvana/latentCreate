//! The lyric document store, wired to the frontend.
//!
//! `library::lyrics` (T-201) persists one JSON file per lyric document, but
//! nothing exposed it to Tauri. These commands are the seam: the frontend holds
//! the `LyricDoc` (versions and approval) and asks the backend to open, save and
//! lint it. Until Phase 4's Library view, there is exactly one project and one
//! working document, both created on demand.

use create_core::lyrics::lint::{lint_lyrics, LintFinding};
use create_core::lyrics::LyricBrief;
use create_core::project::LyricDoc;
use tauri::State;

use crate::projectctx::default_project;
use crate::{ConfigDir, ProfilesDir};

/// Open the working lyric document, creating it (and its project) on first use.
///
/// Returns the newest document the project already has, or a fresh empty one.
/// Never fails for a service reason: there is no service here, only the disk.
#[tauri::command]
pub fn lyrics_open(config_dir: State<'_, ConfigDir>) -> Result<LyricDoc, String> {
    let root = &config_dir.0;
    let mut project = default_project(root).map_err(|e| e.to_string())?;
    let doc = match project.lyrics.first() {
        Some(id) => {
            library::lyrics::load_doc(root, &project.slug, id).map_err(|e| e.to_string())?
        }
        None => library::lyrics::create_doc(root, &mut project, None).map_err(|e| e.to_string())?,
    };
    Ok(doc)
}

/// Persist the working document, versions and approval included.
///
/// The document id is validated against the whitelist before it touches a
/// path, so a bogus id from the frontend cannot write outside the project.
#[tauri::command]
pub fn lyrics_save(config_dir: State<'_, ConfigDir>, doc: LyricDoc) -> Result<(), String> {
    let root = &config_dir.0;
    let project = default_project(root).map_err(|e| e.to_string())?;
    library::lyrics::save_doc(root, &project.slug, &doc).map_err(|e| e.to_string())
}

/// Lint lyric text against a profile and brief, returning advisory findings.
///
/// A missing profile yields no findings rather than an error: the lint is
/// advisory, and nothing to check against is not a fault.
#[tauri::command]
pub fn lyrics_lint(
    profiles_dir: State<'_, ProfilesDir>,
    config_dir: State<'_, ConfigDir>,
    profile_id: String,
    brief: LyricBrief,
    text: String,
) -> Vec<LintFinding> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    lint_text(&set, &profile_id, &brief, &text)
}

/// The lint, separated from the command so it can be tested without Tauri state.
fn lint_text(
    set: &library::profiles::ProfileSet,
    profile_id: &str,
    brief: &LyricBrief,
    text: &str,
) -> Vec<LintFinding> {
    let Some(loaded) = set.profiles.get(profile_id) else {
        return Vec::new();
    };
    lint_lyrics(&loaded.profile, brief, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Protects: a lyric with a stray production direction is linted against
    /// the shipped profile, and a missing profile lints to nothing rather than
    /// failing the generation.
    #[test]
    fn test_lint_text_finds_stray_directions_and_missing_profile_is_empty() {
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../profiles");
        let user = Path::new(env!("CARGO_MANIFEST_DIR")).join("../nonexistent");
        let set = library::profiles::load(&shipped, &user);

        let brief = LyricBrief::default();
        let text = "[Verse]\nfirst\n[Chorus]\nhook\n[whispered] secret\n[Verse]\nsecond\n";
        let findings = lint_text(&set, "ace-step-1.5-turbo", &brief, text);
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, LintFinding::UnknownTag { tag, .. } if tag == "[whispered]")),
            "{findings:#?}"
        );

        assert!(lint_text(&set, "no-such-profile", &brief, text).is_empty());
    }
}
