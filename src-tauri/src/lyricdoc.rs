//! The lyric document store, wired to the frontend.
//!
//! `library::lyrics` (T-201) persists one JSON file per lyric document, but
//! nothing exposed it to Tauri. These commands are the seam: the frontend holds
//! the `LyricDoc` (versions and approval) and asks the backend to open, save,
//! list, create and delete them. T-408b retired the Phase 2 one-document
//! shortcut -- a project can now hold many documents, addressed by id.

use create_core::generation::LyricDocId;
use create_core::lyrics::lint::{lint_lyrics, LintFinding};
use create_core::lyrics::LyricBrief;
use create_core::project::LyricDoc;
use library::LyricDocSet;
use tauri::State;

use crate::projectctx::selected_project;
use crate::{ConfigDir, ProfilesDir};

/// Open a lyric document by id, or -- with no id -- the project's first
/// document, creating one (and the project) when there is none.
///
/// The `id` argument is what retires the Phase 2 one-document shortcut (T-408b):
/// the picker passes the id it wants, while the no-id call stays as the
/// first-open default. Never fails for a service reason: there is no service
/// here, only the disk.
#[tauri::command]
pub fn lyrics_open(
    config_dir: State<'_, ConfigDir>,
    doc_id: Option<String>,
) -> Result<LyricDoc, String> {
    let root = &config_dir.0;
    let mut project = selected_project(root).map_err(|e| e.to_string())?;
    if let Some(id) = doc_id {
        return library::lyrics::load_doc(root, &project.slug, &LyricDocId(id))
            .map_err(|e| e.to_string());
    }
    let doc = match project.lyrics.first() {
        Some(id) => {
            library::lyrics::load_doc(root, &project.slug, id).map_err(|e| e.to_string())?
        }
        None => library::lyrics::create_doc(root, &mut project, None).map_err(|e| e.to_string())?,
    };
    Ok(doc)
}

/// Every lyric document in the selected project, in creation order, plus any
/// warnings (an id listed with no file, an unreadable one). Drives the picker.
#[tauri::command]
pub fn lyrics_list(config_dir: State<'_, ConfigDir>) -> Result<LyricDocSet, String> {
    let root = &config_dir.0;
    let project = selected_project(root).map_err(|e| e.to_string())?;
    Ok(library::lyrics::list_docs(root, &project))
}

/// Create a new, empty lyric document in the selected project and return it.
///
/// The id is minted from the project's monotonic counter, never reused, so a
/// deleted document's id cannot be handed to a later one.
#[tauri::command]
pub fn lyrics_create(
    config_dir: State<'_, ConfigDir>,
    title: Option<String>,
) -> Result<LyricDoc, String> {
    let root = &config_dir.0;
    let mut project = selected_project(root).map_err(|e| e.to_string())?;
    library::lyrics::create_doc(root, &mut project, title).map_err(|e| e.to_string())
}

/// Delete a whole lyric document -- file to OS trash, id unlisted -- refusing
/// (with a message naming the tracks) when a track's provenance references any
/// of its versions. Returns the project's remaining documents.
#[tauri::command]
pub fn lyrics_delete_doc(
    config_dir: State<'_, ConfigDir>,
    doc_id: String,
) -> Result<LyricDocSet, String> {
    let root = &config_dir.0;
    let project = selected_project(root).map_err(|e| e.to_string())?;
    library::lyrics::delete_doc(
        root,
        &project.slug,
        &LyricDocId(doc_id),
        library::tracks::trash_to_os,
    )
    .map_err(|e| e.to_string())
}

/// Persist the working document, versions and approval included.
///
/// The document id is validated against the whitelist before it touches a
/// path, so a bogus id from the frontend cannot write outside the project.
#[tauri::command]
pub fn lyrics_save(config_dir: State<'_, ConfigDir>, doc: LyricDoc) -> Result<(), String> {
    let root = &config_dir.0;
    let project = selected_project(root).map_err(|e| e.to_string())?;
    library::lyrics::save_doc(root, &project.slug, &doc).map_err(|e| e.to_string())
}

/// Delete one version from the working document, returning the updated document.
///
/// Refuses when a track's provenance points at the version, with a message
/// naming the tracks -- the refusal is the feature (PROJECT.md decisions log,
/// 2026-09-01). The document id is whitelisted in `library` before it touches a
/// path, so a bogus id from the frontend cannot escape the project.
#[tauri::command]
pub fn lyrics_delete_version(
    config_dir: State<'_, ConfigDir>,
    doc_id: String,
    version: u32,
) -> Result<LyricDoc, String> {
    let root = &config_dir.0;
    let project = selected_project(root).map_err(|e| e.to_string())?;
    library::lyrics::delete_version(root, &project, &LyricDocId(doc_id), version)
        .map_err(|e| e.to_string())
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
