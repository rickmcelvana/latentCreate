//! Installing a profile's missing models into the user's ComfyUI.
//!
//! Separate from `models`, which only reports. This module is the only thing in
//! the app that starts a multi-gigabyte transfer, and it runs solely from an
//! explicit user action -- ACE-Step 1.5 is 18.5 GiB across four files.
//!
//! **Where the files go is ComfyUI's answer, not the app's guess** (T-519).
//! Each file lands in the directory `/internal/folder_paths` reports for its
//! folder, written as `.partial` and renamed into place, so ComfyUI never lists
//! a half-written model and lists the finished one at once -- the readiness
//! check that follows reads Installed with no restart. comfy-mcp's
//! `download_model` wrote into comfy-cli's own workspace instead, which on a
//! Comfy Desktop install was a different drive, and the row never turned green.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use comfy_bridge::download::{destination_dir, download_client, run_transfer, Transfer};
use create_core::readiness::ModelInventory;
use serde::Serialize;
use tauri::{async_runtime, State};

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::models::inventory_of;
use crate::{ConfigDir, ProfilesDir};

/// One file's download, once submitted.
#[derive(Debug, Clone, Serialize)]
pub struct StartedFile {
    pub file: String,
    /// The handle progress is read with, or `None` when it could not start.
    pub download_id: Option<String>,
    /// Why this file could not be started. Per-file, so a partly-started
    /// install reports exactly what is missing.
    pub error: Option<String>,
}

/// Progress for one file.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileProgress {
    pub download_id: String,
    /// `starting` | `downloading` | `completed` | `failed` | `unknown`.
    pub status: String,
    pub completed_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    /// `0..100`, null until the size is known.
    pub percent: Option<f64>,
    pub error: Option<String>,
}

impl FileProgress {
    fn starting(id: &str, total: Option<u64>) -> Self {
        FileProgress {
            download_id: id.to_string(),
            status: "starting".into(),
            completed_bytes: Some(0),
            total_bytes: total,
            percent: total.map(|_| 0.0),
            error: None,
        }
    }
}

/// Every download this session started, by id. Managed state.
#[derive(Default)]
pub struct InstallState {
    progress: Arc<Mutex<HashMap<String, FileProgress>>>,
    /// Checked between chunks. Nothing sets it yet -- there is no Cancel
    /// button -- but the transfer honours it, and a closed app needs none: the
    /// task dies with the process and the `.partial` resumes next time.
    cancel: Arc<AtomicBool>,
    next: AtomicU64,
}

/// Start downloading everything one profile is missing.
///
/// **Only ever called from an explicit user action.** Files download one after
/// another in the background; the call returns as soon as each has an id (or a
/// reason it cannot start), and `models_progress` reports them.
#[tauri::command]
pub async fn models_install(
    comfy_state: State<'_, ComfyState>,
    install: State<'_, InstallState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    id: String,
) -> Result<Vec<StartedFile>, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    let profile = set
        .profiles
        .get(&id)
        .map(|loaded| loaded.profile.clone())
        .ok_or_else(|| format!("no profile with id {id}"))?;

    let comfy = ensure_connected(&comfy_state, &config_dir)
        .await
        .map_err(EnsureError::into_message)?;
    if !comfy.is_loopback() {
        return Err(format!(
            "ComfyUI is at {}, not on this computer, so latentCreate cannot put files in its \
             models folder. Install the model on that computer, then Retry here.",
            comfy.base_url()
        ));
    }

    let folders = profile
        .comfy
        .models
        .iter()
        .map(|m| m.folder.clone())
        .collect();
    let inventory: ModelInventory = inventory_of(&comfy, &folders).await?;
    let paths = comfy.folder_paths().await.map_err(|e| {
        format!(
            "ComfyUI did not say where it keeps models ({e}). Download the files into its models \
             folder by hand, then Retry."
        )
    })?;

    let mut started = Vec::new();
    let mut transfers: Vec<(String, Transfer)> = Vec::new();
    for spec in profile.readiness(Some(&inventory)).missing() {
        let fail = |error: String| StartedFile {
            file: spec.file.clone(),
            download_id: None,
            error: Some(error),
        };
        let Some(url) = spec.source_url.clone() else {
            started.push(fail("no download URL; place this file by hand".into()));
            continue;
        };
        let dir = match destination_dir(&spec.folder, &paths, |p| p.is_dir()) {
            Ok(dir) => dir,
            Err(e) => {
                started.push(fail(e.to_string()));
                continue;
            }
        };
        let download_id = format!("dl-{}", install.next.fetch_add(1, Ordering::Relaxed));
        install.progress.lock().expect("progress lock").insert(
            download_id.clone(),
            FileProgress::starting(&download_id, spec.size_bytes),
        );
        transfers.push((
            download_id.clone(),
            Transfer {
                url,
                dest: dir.join(&spec.file),
                expected_size: spec.size_bytes,
            },
        ));
        started.push(StartedFile {
            file: spec.file.clone(),
            download_id: Some(download_id),
            error: None,
        });
    }

    if !transfers.is_empty() {
        install.cancel.store(false, Ordering::Relaxed);
        let progress = Arc::clone(&install.progress);
        let cancel = Arc::clone(&install.cancel);
        let log_dir = config_dir.0.clone();
        async_runtime::spawn(run_all(transfers, progress, cancel, log_dir));
    }
    Ok(started)
}

/// Download each file in turn, recording progress as it goes.
async fn run_all(
    transfers: Vec<(String, Transfer)>,
    progress: Arc<Mutex<HashMap<String, FileProgress>>>,
    cancel: Arc<AtomicBool>,
    log_dir: PathBuf,
) {
    let client = download_client();
    for (id, transfer) in transfers {
        let update = |done: u64, total: Option<u64>| {
            if let Ok(mut map) = progress.lock() {
                if let Some(p) = map.get_mut(&id) {
                    p.status = "downloading".into();
                    p.completed_bytes = Some(done);
                    p.total_bytes = total;
                    p.percent = total
                        .filter(|t| *t > 0)
                        .map(|t| (done as f64 / t as f64 * 100.0).min(100.0));
                }
            }
        };
        let result = run_transfer(&client, &transfer, update, &cancel).await;
        if let Ok(log) = comfy_bridge::SessionLog::open(log_dir.join("session.log")) {
            log.log_result(
                "model_download",
                result.is_ok(),
                &format!(
                    "{} -> {}: {result:?}",
                    transfer.url,
                    transfer.dest.display()
                ),
            );
        }
        if let Ok(mut map) = progress.lock() {
            if let Some(p) = map.get_mut(&id) {
                match &result {
                    Ok(()) => {
                        p.status = "completed".into();
                        p.percent = Some(100.0);
                    }
                    Err(e) => {
                        p.status = "failed".into();
                        p.error = Some(e.to_string());
                    }
                }
            }
        }
    }
}

/// Report every download asked about, in one round trip.
#[tauri::command]
pub async fn models_progress(
    install: State<'_, InstallState>,
    ids: Vec<String>,
) -> Result<Vec<FileProgress>, String> {
    let map = install.progress.lock().map_err(|e| e.to_string())?;
    Ok(progress_of(&map, ids))
}

/// Look each id up. An id this session never started -- the app restarted
/// mid-download -- is `failed` with a sentence, because the view polls until
/// every row is terminal and an `unknown` would hold it there forever. The
/// `.partial` file is still on disk, so Install again resumes it.
fn progress_of(map: &HashMap<String, FileProgress>, ids: Vec<String>) -> Vec<FileProgress> {
    ids.into_iter()
        .map(|id| {
            map.get(&id).cloned().unwrap_or(FileProgress {
                download_id: id,
                status: "failed".into(),
                completed_bytes: None,
                total_bytes: None,
                percent: None,
                error: Some(
                    "This download stopped when latentCreate closed. Install again to resume it."
                        .into(),
                ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_new_download_starts_at_zero_of_its_declared_size() {
        let p = FileProgress::starting("dl-0", Some(100));
        assert_eq!(p.status, "starting");
        assert_eq!(p.percent, Some(0.0));
        let unknown = FileProgress::starting("dl-1", None);
        assert_eq!(unknown.percent, None);
    }

    /// Protects: an id this session never started is terminal for the view,
    /// with a next step -- not a row the poll loop waits on forever.
    #[test]
    fn test_progress_for_an_unknown_id_is_failed_with_a_next_step() {
        let mut map = HashMap::new();
        map.insert("dl-0".to_string(), FileProgress::starting("dl-0", Some(10)));
        let out = progress_of(&map, vec!["dl-0".into(), "dl-9".into()]);
        assert_eq!(out[0].status, "starting");
        assert_eq!(out[1].status, "failed");
        assert!(out[1]
            .error
            .as_deref()
            .expect("why")
            .contains("Install again"));
    }
}
