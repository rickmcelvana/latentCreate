//! Tauri commands over the on-disk track library.
//!
//! **Named `tracks`, not `library`, and it cannot be.** This crate depends
//! on a crate called `library`, so a `mod library;` here shadows it for every
//! `library::...` path in the whole crate -- ten unresolved-name errors, none
//! of them pointing here. The house pattern is a module named for what it
//! wraps anyway: `lyricdoc` over `library::lyrics`, `profile` over
//! `library::profiles`, this over `library::tracks`.

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
    let project = crate::projectctx::default_project(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(library::tracks::list_tracks(&config_dir.0, &project))
}
