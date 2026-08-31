//! Tauri commands over the on-disk project store.
//!
//! The selection is deliberately **not** a command here: it persists through
//! the existing `save_config` path exactly as `default_profile_id` does
//! (T-303), so the config store stays the single writer of config. These two
//! commands only list and create.

use create_core::project::Project;
use tauri::State;

use crate::ConfigDir;

/// Every project the app could read, with warnings for the ones it could not.
///
/// Never fails: a project that cannot be read is a warning, not an error that
/// hides every other one (`library::projects::list_projects`).
#[tauri::command]
pub fn projects_list(config_dir: State<'_, ConfigDir>) -> library::ProjectSet {
    library::projects::list_projects(&config_dir.0)
}

/// Create a project and return its record.
///
/// Selecting it is the frontend's next step, kept separate so creating and
/// selecting stay independently testable. `name` is user text; the slug comes
/// from `slugify`, and a taken slug gets a numeric suffix rather than the
/// existing project being returned.
#[tauri::command]
pub fn projects_create(config_dir: State<'_, ConfigDir>, name: String) -> Result<Project, String> {
    library::projects::create_project(&config_dir.0, &name, &library::projects::now_rfc3339())
        .map_err(|e| e.to_string())
}
