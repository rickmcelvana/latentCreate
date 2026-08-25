//! Installing a profile's missing models.
//!
//! Separate from `models`, which only reports. This module is the only thing in
//! the app that starts a multi-gigabyte transfer, and it runs solely from an
//! explicit user action -- ACE-Step 1.5 is 18.5 GiB across four files
//! (MCP-SURFACE 14.5).
//!
//! Files are submitted and reported one at a time. comfy-cli takes a single URL
//! per call, and a failure on the third file must not discard the two already
//! running or leave the user unsure what did download.

use std::collections::BTreeSet;

use serde::Serialize;
use tauri::State;

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::models::{take_inventory, TakeError};
use crate::{ConfigDir, ProfilesDir};

/// One file's download, once submitted.
#[derive(Debug, Clone, Serialize)]
pub struct StartedFile {
    pub file: String,
    /// The handle progress is polled with, or `None` when the submit failed.
    pub download_id: Option<String>,
    /// Why this file could not be started, when it could not. Per-file so a
    /// partly-started install reports exactly what is missing rather than
    /// failing whole and leaving the user unsure what already downloaded.
    pub error: Option<String>,
}

/// Progress for one file being downloaded.
#[derive(Debug, Clone, Serialize)]
pub struct FileProgress {
    pub download_id: String,
    pub status: String,
    pub completed_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    /// `0..100`, null until the server sends a content length.
    pub percent: Option<f64>,
    pub error: Option<String>,
}

/// Start downloading everything one profile is missing.
///
/// **Only ever called from an explicit user action.** ACE-Step 1.5 is 18.5 GiB
/// across four files; nothing here may run on its own, and the size is shown
/// before the button is offered.
///
/// Files are submitted individually and reported individually. comfy-cli takes
/// one URL per call, and a failure on the third file must not discard the two
/// already running.
#[tauri::command]
pub async fn models_install(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    id: String,
    bin: Option<String>,
) -> Result<Vec<StartedFile>, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    let loaded = set
        .profiles
        .get(&id)
        .ok_or_else(|| format!("no profile with id {id}"))?;

    let folders: BTreeSet<String> = loaded
        .profile
        .comfy
        .models
        .iter()
        .map(|spec| spec.folder.clone())
        .collect();
    let inventory = match take_inventory(&state, &config_dir, bin.clone(), &folders).await {
        Ok(inventory) => inventory,
        Err(TakeError::Comfy(e)) => return Err(e.to_string()),
        Err(TakeError::Log(detail)) => return Err(detail),
    };

    let comfy = match ensure_connected(&state, &config_dir, bin).await {
        Ok(comfy) => comfy,
        Err(EnsureError::Comfy(e)) => return Err(e.to_string()),
        Err(EnsureError::Log(detail)) => return Err(detail),
    };

    let mut started = Vec::new();
    for spec in loaded.profile.readiness(Some(&inventory)).missing() {
        let Some(url) = spec.source_url.as_deref() else {
            started.push(StartedFile {
                file: spec.file.clone(),
                download_id: None,
                error: Some("no download URL; place this file by hand".to_string()),
            });
            continue;
        };
        // `filename` is sent even though these URLs end in the name --
        // comfy-cli fails the call outright when it cannot work one out.
        let relative = models_relative_path(&spec.folder);
        let submitted = comfy.download_model(url, &relative, Some(&spec.file)).await;
        started.push(match submitted {
            Ok(submit) => StartedFile {
                file: spec.file.clone(),
                download_id: Some(submit.download_id),
                error: None,
            },
            Err(e) => StartedFile {
                file: spec.file.clone(),
                download_id: None,
                error: Some(e.to_string()),
            },
        });
    }
    Ok(started)
}

/// Poll every in-flight download.
///
/// One call for the whole set, so the view makes a single round trip per tick
/// rather than one per file.
#[tauri::command]
pub async fn models_progress(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    ids: Vec<String>,
    bin: Option<String>,
) -> Result<Vec<FileProgress>, String> {
    let comfy = match ensure_connected(&state, &config_dir, bin).await {
        Ok(comfy) => comfy,
        Err(EnsureError::Comfy(e)) => return Err(e.to_string()),
        Err(EnsureError::Log(detail)) => return Err(detail),
    };
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        out.push(match comfy.download_status(&id).await {
            Ok(state) => FileProgress {
                download_id: id,
                status: state.status,
                completed_bytes: state.completed_bytes,
                total_bytes: state.total_bytes,
                percent: state.percent,
                error: state.error,
            },
            // A poll that fails is not a failed download -- report it as
            // unknown and let the next tick answer.
            Err(e) => FileProgress {
                download_id: id,
                status: "unknown".to_string(),
                completed_bytes: None,
                total_bytes: None,
                percent: None,
                error: Some(e.to_string()),
            },
        });
    }
    Ok(out)
}

/// Where comfy-cli writes a model for one ComfyUI folder.
///
/// **Must start with `models`**: comfy-cli rejects anything else, and the
/// folder name alone (`"vae"`) is exactly the mistake that reads correctly
/// (MCP-SURFACE 11.2).
fn models_relative_path(folder: &str) -> String {
    format!("models/{folder}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protects: the destination comfy-cli is given. It must start with
    /// `models` or the call is rejected outright, and the bare folder name is
    /// the plausible-looking mistake. Verified against the live folder list.
    #[test]
    fn test_download_destination_is_under_models() {
        assert_eq!(
            models_relative_path("diffusion_models"),
            "models/diffusion_models"
        );
        assert_eq!(models_relative_path("vae"), "models/vae");
        for folder in ["diffusion_models", "text_encoders", "vae"] {
            assert!(models_relative_path(folder).starts_with("models/"));
        }
    }
}
