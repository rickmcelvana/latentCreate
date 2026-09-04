//! Tauri commands over the on-disk artwork library.
//!
//! Named for what it wraps (`library::art`), the house pattern `tracks.rs`
//! describes. The shadowing hazard that file's header warns about is specific
//! to a module named `library`; `art` collides with nothing.

use create_core::project::ArtId;
use library::ArtSet;
use tauri::State;

use crate::ConfigDir;

/// Every artwork in the selected project, with warnings for any sidecars that
/// could not be read.
///
/// `Err` only when the project itself cannot be resolved; an unreadable sidecar
/// is a warning inside `ArtSet` rather than an empty gallery. The track twin,
/// `library_tracks`, splits it the same way for the same reason.
#[tauri::command]
pub fn library_art(config_dir: State<'_, ConfigDir>) -> Result<ArtSet, String> {
    let project = crate::projectctx::selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(library::art::list_art(&config_dir.0, &project))
}

/// The absolute path to one artwork's image file, for the webview to display.
///
/// `id` passes `load_art`'s whitelist before anything is joined, and the stored
/// `file` is resolved through `resolve_art_file`, which refuses an absolute path
/// or a `..` escape from a hand-edited sidecar. The asset protocol's own scope
/// (`$APPCONFIG/projects/**`) is the second gate, and it already covers
/// `projects/<slug>/art/`.
#[tauri::command]
pub fn art_image_path(config_dir: State<'_, ConfigDir>, id: String) -> Result<String, String> {
    let project = crate::projectctx::selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let art = library::art::load_art(&config_dir.0, &project.slug, &ArtId(id))
        .map_err(|e| e.to_string())?;
    let abs = library::art::resolve_art_file(&config_dir.0, &project.slug, &art.file)
        .map_err(|e| e.to_string())?;
    Ok(abs.to_string_lossy().into_owned())
}
