//! latentCreate desktop shell.
//!
//! The single place that wires the workspace crates to the frontend. Tauri
//! commands and events live here; domain logic belongs in the crates
//! (ARCHITECTURE.md section 2).

use std::path::PathBuf;
use tauri::Manager;

mod comfy;
mod install;
mod jobs;
mod models;

use jobs::ComfyState;

/// Resolved once at startup so every command shares one location.
struct ConfigDir(PathBuf);

/// Where the profiles that ship with the app live, resolved once at startup.
///
/// Bundled as a resource; in a dev build the bundle has not been assembled, so
/// this falls back to the repo's own `profiles/`. A missing directory is not an
/// error here -- `library::profiles::load` treats it as "no shipped profiles",
/// and the models step then has nothing to check rather than failing to open.
struct ProfilesDir(PathBuf);

/// Launches the Tauri application.
///
/// # Panics
/// Panics if the Tauri runtime cannot start, which is unrecoverable at
/// this point in startup.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(ConfigDir(dir));
            app.manage(ProfilesDir(shipped_profiles_dir(app.handle())));
            app.manage(ComfyState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            load_config,
            save_config,
            set_secret,
            has_secret,
            delete_secret,
            comfy::comfy_status,
            comfy::comfy_launch,
            models::models_status,
            install::models_install,
            install::models_progress,
            jobs::connect_comfy,
            jobs::run_workflow,
            jobs::cancel_job
        ])
        .run(tauri::generate_context!())
        .expect("error while running latentCreate");
}

/// Returns the shell's crate version, so the frontend can prove the bridge
/// round-trips before any real command exists.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Loads the persisted config (and any warnings) from the app config directory.
#[tauri::command]
fn load_config(state: tauri::State<'_, ConfigDir>) -> library::LoadedConfig {
    library::config::load(&state.0)
}

/// Saves the config atomically to the app config directory.
#[tauri::command]
fn save_config(state: tauri::State<'_, ConfigDir>, config: library::Config) -> Result<(), String> {
    library::config::save(&state.0, &config).map_err(|e| e.to_string())
}

/// Stores a secret in the OS keychain. `name` must be a whitelisted secret key.
#[tauri::command]
fn set_secret(name: String, value: String) -> Result<(), String> {
    let key = library::SecretKey::parse(&name).map_err(|e| e.to_string())?;
    library::secrets::set_secret(key, &value).map_err(|e| e.to_string())
}

/// Returns whether a whitelisted secret is stored in the keychain.
#[tauri::command]
fn has_secret(name: String) -> Result<bool, String> {
    let key = library::SecretKey::parse(&name).map_err(|e| e.to_string())?;
    Ok(library::secrets::has_secret(key))
}

/// Deletes a whitelisted secret from the keychain.
#[tauri::command]
fn delete_secret(name: String) -> Result<(), String> {
    let key = library::SecretKey::parse(&name).map_err(|e| e.to_string())?;
    library::secrets::delete_secret(key).map_err(|e| e.to_string())
}

/// Locate the shipped profiles directory.
///
/// The bundled resource wins. A dev build has no bundle, so it falls back to
/// the repo checkout next to this crate -- which is also why this returns a
/// path rather than failing: neither location existing is a normal state for a
/// build with no profiles, not a startup error.
fn shipped_profiles_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> PathBuf {
    if let Ok(resources) = app.path().resource_dir() {
        let bundled = resources.join("profiles");
        if bundled.is_dir() {
            return bundled;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../profiles")
}

#[cfg(test)]
mod tests {
    use super::app_version;

    #[test]
    fn test_app_version_matches_crate_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }
}
