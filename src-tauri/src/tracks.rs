//! Tauri commands over the on-disk track library.
//!
//! **Named `tracks`, not `library`, and it cannot be.** This crate depends
//! on a crate called `library`, so a `mod library;` here shadows it for every
//! `library::...` path in the whole crate -- ten unresolved-name errors, none
//! of them pointing here. The house pattern is a module named for what it
//! wraps anyway: `lyricdoc` over `library::lyrics`, `profile` over
//! `library::profiles`, this over `library::tracks`.

use create_core::project::TrackId;
use library::TrackSet;
use tauri::State;

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
