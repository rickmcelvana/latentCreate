//! "Send to" -- hand a finished track to the mixing or mastering app.
//!
//! v1 is a link-out, not a file handoff: the sibling app opens in the browser
//! and the OS file manager reveals the track so the user can drag it in. The
//! real handoff protocol is owned by `../latent-mixing` and
//! `../latent-mastering` and does not exist yet (ARCHITECTURE 8; phase-4
//! decision 3, 2026-09-01).

use std::path::{Path, PathBuf};

use create_core::project::TrackId;
use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::ConfigDir;

/// Which sibling app a track is being sent to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendTarget {
    /// Stem separation and mixing.
    Mixing,
    /// Mastering chains.
    Mastering,
}

/// Shown when the sidecar names an audio file that is not on disk.
const MISSING_FILE: &str = "That track's audio file is not where the app left it. \
Restore or regenerate the track, then try Send to again.";

/// The address of each sibling app.
///
/// The **only** place this app hardcodes the siblings' addresses, and a product
/// decision rather than a constant to bury. Verified 2026-09-01: the live
/// `latentbeats.com` links both, and `../latent-mixing`'s decisions log records
/// `app.latentmixer.com` as the deployed address, with every
/// `latentmixing.com` reference stale and awaiting a doc sweep there.
pub fn target_url(target: SendTarget) -> &'static str {
    match target {
        SendTarget::Mixing => "https://app.latentmixer.com",
        SendTarget::Mastering => "https://app.latentmastering.com",
    }
}

/// Open the sibling app and reveal the track's audio file for drag-in.
///
/// Reveals **before** opening the browser, and refuses outright when the file
/// is missing: a browser tab with nothing to drag into it is a worse outcome
/// than a sentence saying why nothing happened.
#[tauri::command]
pub async fn send_to(
    app: AppHandle,
    config_dir: State<'_, ConfigDir>,
    id: String,
    target: SendTarget,
) -> Result<(), String> {
    let path = track_path(&config_dir.0, &id)?;
    app.opener().reveal_item_in_dir(&path).map_err(|e| {
        format!(
            "Could not show the file: {e}. It is at {}, if you want to open that folder yourself.",
            path.display()
        )
    })?;
    let url = target_url(target);
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Could not open your browser: {e}. The site is {url}."))?;
    Ok(())
}

/// Resolve a track id to its audio file, refusing one that is not on disk.
///
/// Separated from the command so the refusal can be tested without a Tauri
/// app handle: everything above the two `opener` calls is ordinary disk work.
fn track_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    let project = crate::projectctx::selected_project(root).map_err(|e| e.to_string())?;
    let track = library::tracks::load_track(root, &project.slug, &TrackId(id.to_string()))
        .map_err(|e| e.to_string())?;
    let path = library::tracks::resolve_track_file(root, &project.slug, &track.file)
        .map_err(|e| e.to_string())?;
    if !path.is_file() {
        return Err(MISSING_FILE.to_string());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protects: the two addresses are the ones the siblings are actually
    /// deployed at. A typo here is a dead page, and nothing else in the app
    /// mentions these hosts, so no other test can catch it.
    #[test]
    fn test_target_url_names_the_deployed_sibling_apps() {
        assert_eq!(
            target_url(SendTarget::Mixing),
            "https://app.latentmixer.com"
        );
        assert_eq!(
            target_url(SendTarget::Mastering),
            "https://app.latentmastering.com"
        );
    }

    /// Protects: the wire word the frontend sends is the word this enum
    /// accepts. The frontend hand-mirrors this type (CONVENTIONS), so a
    /// rename on either side has to fail here rather than at a click.
    #[test]
    fn test_send_target_deserializes_from_snake_case() {
        let mixing: SendTarget = serde_json::from_str("\"mixing\"").unwrap();
        let mastering: SendTarget = serde_json::from_str("\"mastering\"").unwrap();
        assert_eq!(mixing, SendTarget::Mixing);
        assert_eq!(mastering, SendTarget::Mastering);
    }

    /// Protects: a sidecar whose audio file has been deleted outside the app
    /// stops the send instead of revealing a path that is not there. The
    /// producer has already hit this on the player (T-402 click-through).
    #[test]
    fn test_track_path_refuses_a_sidecar_whose_audio_is_gone() {
        let root = tempfile::tempdir().unwrap();
        let mut project =
            library::projects::create_project(root.path(), "Night Drive", "2026-09-01T10:00:00Z")
                .unwrap();
        let id = library::tracks::mint_track_id(&mut project);
        let track = create_core::provenance::Track {
            id: id.clone(),
            title: None,
            cover: None,
            file: format!("tracks/{}.flac", id.0),
            duration_s: None,
            provenance: provenance_stub(),
        };
        library::tracks::save_track(root.path(), &project.slug, &track).unwrap();
        library::projects::save_project(root.path(), &project).unwrap();

        let err = track_path(root.path(), &id.0).unwrap_err();
        assert_eq!(err, MISSING_FILE);
    }

    /// Protects: an id no project owns is refused before any path is joined.
    #[test]
    fn test_track_path_refuses_an_unknown_id() {
        let root = tempfile::tempdir().unwrap();
        library::projects::create_project(root.path(), "Night Drive", "2026-09-01T10:00:00Z")
            .unwrap();
        assert!(track_path(root.path(), "tr-9999").is_err());
    }

    fn provenance_stub() -> create_core::provenance::Provenance {
        create_core::provenance::Provenance {
            profile_id: "ace-step-1.5-turbo".to_string(),
            profile_display_name: "ACE-Step 1.5 XL Turbo".to_string(),
            model_license: "Apache-2.0".to_string(),
            template: None,
            spec: create_core::generation::GenerationSpec {
                title: None,
                profile_id: "ace-step-1.5-turbo".to_string(),
                inputs: std::collections::BTreeMap::new(),
                loras: Vec::new(),
                lyrics: None,
            },
            resolved_slots: std::collections::BTreeMap::new(),
            comfy: None,
            created_at: "2026-09-01T10:00:00Z".to_string(),
            prompt_id: None,
        }
    }
}
