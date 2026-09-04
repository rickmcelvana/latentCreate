//! Tauri commands over album lists.
//!
//! Thin glue like `tracks` and `projects`: resolve the selected project, call
//! `library::albums`, map errors. No command-level tests -- the chain
//! `selected_project` -> `library::albums` shares one `project` value, and the
//! rules (reorder permutation, duplicate names, foreign track ids) live in
//! `library` where their tests reach them.

use create_core::project::{AlbumList, ArtId, TrackId};
use tauri::State;

use crate::projectctx::selected_project;
use crate::ConfigDir;

/// The result every album command returns: the project's albums after the
/// write, so the frontend never has to guess whether it landed.
type AlbumsResult = Result<Vec<AlbumList>, String>;

/// Every album in the selected project, in creation order.
#[tauri::command]
pub fn albums_list(config_dir: State<'_, ConfigDir>) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::list_albums(&config_dir.0, &project.slug).map_err(|e| e.to_string())
}

/// Create an album in the selected project.
#[tauri::command]
pub fn album_create(config_dir: State<'_, ConfigDir>, name: String) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::create_album(&config_dir.0, &project.slug, &name).map_err(|e| e.to_string())
}

/// Rename an album in the selected project.
#[tauri::command]
pub fn album_rename(config_dir: State<'_, ConfigDir>, from: String, to: String) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::rename_album(&config_dir.0, &project.slug, &from, &to)
        .map_err(|e| e.to_string())
}

/// Delete an album from the selected project. Removes only the list; every
/// track it held stays in the library.
#[tauri::command]
pub fn album_delete(config_dir: State<'_, ConfigDir>, name: String) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::delete_album(&config_dir.0, &project.slug, &name).map_err(|e| e.to_string())
}

/// Add a track to an album in the selected project.
#[tauri::command]
pub fn album_add_track(
    config_dir: State<'_, ConfigDir>,
    album: String,
    track_id: String,
) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let track_id = TrackId(track_id);
    library::albums::add_track(&config_dir.0, &project.slug, &album, &track_id)
        .map_err(|e| e.to_string())
}

/// Remove a track from an album in the selected project.
#[tauri::command]
pub fn album_remove_track(
    config_dir: State<'_, ConfigDir>,
    album: String,
    track_id: String,
) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let track_id = TrackId(track_id);
    library::albums::remove_track(&config_dir.0, &project.slug, &album, &track_id)
        .map_err(|e| e.to_string())
}

/// Reorder an album's tracks in the selected project. `track_ids` is the full
/// new order; the backend refuses anything that is not the current tracks
/// rearranged.
#[tauri::command]
pub fn album_reorder(
    config_dir: State<'_, ConfigDir>,
    album: String,
    track_ids: Vec<TrackId>,
) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::reorder_tracks(&config_dir.0, &project.slug, &album, &track_ids)
        .map_err(|e| e.to_string())
}

/// Set or clear an album's cover. `None` clears it.
///
/// The artwork id is checked against the project so a dangling reference is never
/// written.
#[tauri::command]
pub fn album_set_cover(
    config_dir: State<'_, ConfigDir>,
    album: String,
    cover: Option<String>,
) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let cover_id = cover.map(ArtId);
    library::albums::set_album_cover(&config_dir.0, &project.slug, &album, cover_id.as_ref())
        .map_err(|e| e.to_string())
}
