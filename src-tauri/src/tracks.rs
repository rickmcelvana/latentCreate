//! Tauri commands over the on-disk track library.
//!
//! **Named `tracks`, not `library`, and it cannot be.** This crate depends
//! on a crate called `library`, so a `mod library;` here shadows it for every
//! `library::...` path in the whole crate -- ten unresolved-name errors, none
//! of them pointing here. The house pattern is a module named for what it
//! wraps anyway: `lyricdoc` over `library::lyrics`, `profile` over
//! `library::profiles`, this over `library::tracks`.

use std::path::PathBuf;

use create_core::project::{ArtId, TrackId};
use library::TrackSet;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::ConfigDir;

/// Every track in the default project, with warnings for any sidecars that
/// could not be read.
///
/// `Err` only when the project itself cannot be resolved; a bad sidecar is
/// surfaced as a warning inside `TrackSet` rather than hiding the library.
#[tauri::command]
pub fn library_tracks(config_dir: State<'_, ConfigDir>) -> Result<TrackSet, String> {
    let project = crate::projectctx::selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(library::tracks::list_tracks(&config_dir.0, &project))
}

/// The absolute path to one track's audio file, for the webview to play.
///
/// `id` is validated by `load_track`'s whitelist before anything is joined, and
/// the stored `file` is resolved and checked to stay inside the project's
/// `tracks/` directory by [`library::tracks::resolve_track_file`]. Returns
/// `Err` for an unknown id, an unreadable sidecar, or a stored path that
/// escapes -- the frontend maps that to a play error rather than a crash.
#[tauri::command]
pub fn track_audio_path(config_dir: State<'_, ConfigDir>, id: String) -> Result<String, String> {
    let project = crate::projectctx::selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let track = library::tracks::load_track(&config_dir.0, &project.slug, &TrackId(id))
        .map_err(|e| e.to_string())?;
    let abs = library::tracks::resolve_track_file(&config_dir.0, &project.slug, &track.file)
        .map_err(|e| e.to_string())?;
    Ok(abs.to_string_lossy().into_owned())
}

/// Move a track's audio and sidecar to the OS trash and unlist its id.
///
/// Never a hard delete (CONVENTIONS): the real trasher is
/// `library::tracks::trash_to_os`. The frontend re-loads the library on
/// success -- the action is synchronous and user-initiated, so no event is
/// pushed. Removing the id from `Project::tracks` and every album is the
/// library's job; the id is never reused.
#[tauri::command]
pub fn delete_track(config_dir: State<'_, ConfigDir>, id: String) -> Result<(), String> {
    let root = &config_dir.0;
    let project = crate::projectctx::selected_project(root).map_err(|e| e.to_string())?;
    library::tracks::delete_track(
        root,
        &project.slug,
        &TrackId(id),
        library::tracks::trash_to_os,
    )
    .map_err(|e| e.to_string())
}

/// Set or clear a track's title on its sidecar.
///
/// An empty title clears it, and the Library falls back to the id. The sidecar
/// is the single source of truth for a title (ARCHITECTURE 8), so nothing else
/// is written.
#[tauri::command]
pub fn rename_track(
    config_dir: State<'_, ConfigDir>,
    id: String,
    title: String,
) -> Result<(), String> {
    let root = &config_dir.0;
    let project = crate::projectctx::selected_project(root).map_err(|e| e.to_string())?;
    library::tracks::rename_track(root, &project.slug, &TrackId(id), &title)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Set or clear a track's cover. `None` clears it.
///
/// The artwork id is checked against the project so a dangling reference is
/// never written; the sidecar is the single source of truth for a cover.
#[tauri::command]
pub fn set_track_cover(
    config_dir: State<'_, ConfigDir>,
    id: String,
    cover: Option<String>,
) -> Result<(), String> {
    let root = &config_dir.0;
    let project = crate::projectctx::selected_project(root).map_err(|e| e.to_string())?;
    let cover_id = cover.map(ArtId);
    library::tracks::set_track_cover(root, &project.slug, &TrackId(id), cover_id.as_ref())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Copy a track's audio file to a destination the user chose in the save dialog.
///
/// `dest` comes from the OS save dialog, so it is trusted; the source id is
/// whitelisted before it touches a path. A copy, so the track stays in the
/// library.
#[tauri::command]
pub fn export_track(
    config_dir: State<'_, ConfigDir>,
    id: String,
    dest: String,
) -> Result<(), String> {
    let root = &config_dir.0;
    let project = crate::projectctx::selected_project(root).map_err(|e| e.to_string())?;
    library::tracks::export_track(root, &project.slug, &TrackId(id), &PathBuf::from(dest))
        .map_err(|e| e.to_string())
}

/// Reveal a track's audio file in the OS file manager.
///
/// The same reveal `send_to` uses, on its own: resolve the id to an absolute
/// path, then hand it to the opener plugin.
#[tauri::command]
pub fn reveal_track(
    app: AppHandle,
    config_dir: State<'_, ConfigDir>,
    id: String,
) -> Result<(), String> {
    let root = &config_dir.0;
    let project = crate::projectctx::selected_project(root).map_err(|e| e.to_string())?;
    let track = library::tracks::load_track(root, &project.slug, &TrackId(id))
        .map_err(|e| e.to_string())?;
    let path = library::tracks::resolve_track_file(root, &project.slug, &track.file)
        .map_err(|e| e.to_string())?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| format!("Could not show the file: {e}. It is at {}.", path.display()))
}
