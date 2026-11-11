//! latentCreate desktop shell.
//!
//! The single place that wires the workspace crates to the frontend. Tauri
//! commands and events live here; domain logic belongs in the crates
//! (ARCHITECTURE.md section 2).

use std::path::PathBuf;
use tauri::Manager;

mod albums;
mod art;
mod comfy;
mod generate;
mod import;
mod ingest;
mod install;
mod jobs;
mod llm;
mod loras;
mod lyricdoc;
mod lyrics;
mod models;
mod optimize;
mod profile;
mod projectctx;
mod projects;
mod sendto;
mod tracks;

use jobs::ComfyState;
use lyrics::LyricsState;

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
            app.manage(ProfilesDir(shipped_dir(app.handle(), "profiles")));
            app.manage(ComfyState::default());
            app.manage(install::InstallState::default());
            app.manage(LyricsState::default());
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
            models::models_status,
            generate::generate_audio,
            generate::generate_image,
            llm::llm_probe,
            llm::llm_test,
            profile::profile_guide,
            profile::profile_inputs,
            profile::enum_choices,
            loras::lora_panel,
            import::import_workflow,
            import::save_imported_profile,
            install::models_install,
            install::models_progress,
            sendto::send_to,
            tracks::delete_track,
            tracks::rename_track,
            tracks::set_track_cover,
            tracks::export_track,
            tracks::reveal_track,
            jobs::connect_comfy,
            jobs::run_workflow,
            jobs::cancel_job,
            tracks::library_tracks,
            tracks::track_audio_path,
            art::library_art,
            art::art_image_path,
            art::delete_art,
            projects::projects_list,
            projects::projects_create,
            projects::projects_delete,
            albums::albums_list,
            albums::album_create,
            albums::album_rename,
            albums::album_delete,
            albums::album_add_track,
            albums::album_remove_track,
            albums::album_reorder,
            albums::album_set_cover,
            lyrics::lyrics_generate,
            lyrics::lyrics_cancel,
            optimize::lyrics_optimize,
            lyricdoc::lyrics_open,
            lyricdoc::lyrics_list,
            lyricdoc::lyrics_create,
            lyricdoc::lyrics_save,
            lyricdoc::lyrics_delete_version,
            lyricdoc::lyrics_delete_doc,
            lyricdoc::lyrics_lint
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

/// Locate one of the app's shipped resource directories.
///
/// The bundled resource wins: `tauri.conf.json` maps `../profiles/` to
/// `profiles/` explicitly, so it lands at `<resource_dir>/profiles`. (Listed as
/// a `../profiles/*.json` glob it landed at `<resource_dir>/_up_/profiles`, this
/// lookup missed it, and every build fell back to the source checkout path
/// compiled in below -- which exists only on the machine that built it. A
/// CI-built installer therefore shipped with no models at all on anyone else's
/// PC; T-519.)
///
/// The checkout fallback is **debug builds only**: in a release build a
/// missing bundle must show as "no profiles", never be masked by the
/// developer's own files.
fn shipped_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>, name: &str) -> PathBuf {
    let bundled = app.path().resource_dir().map(|r| r.join(name));
    if let Ok(dir) = &bundled {
        if dir.is_dir() {
            return dir.clone();
        }
    }
    #[cfg(debug_assertions)]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(name)
    }
    #[cfg(not(debug_assertions))]
    {
        bundled.unwrap_or_else(|_| PathBuf::from(name))
    }
}

#[cfg(test)]
mod tests {
    use super::app_version;

    #[test]
    fn test_app_version_matches_crate_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }

    /// The CSP must name the asset protocol in every directive the app loads
    /// through it.
    ///
    /// A test is the only guard there can be. `app.security.csp` is injected
    /// only into responses the app's own protocol serves, and a dev run loads
    /// the frontend from Vite's dev server instead -- so **no CSP is applied
    /// in development at all**, and a directive the app needs but does not
    /// declare fails for the first time in a packaged build. That is exactly
    /// how the missing `img-src` shipped: `media-src` was written for T-402's
    /// audio, T-506's cover art arrived four months later, and every gate,
    /// click-through and dev run passed with the gap open.
    ///
    /// Both directives are checked together because both are reached the same
    /// way -- `convertFileSrc` in `bridge/player.ts` for audio and
    /// `bridge/art.ts` for images -- and both spellings are required:
    /// `asset:` on macOS and Linux, `http://asset.localhost` on Windows.
    /// Protects: profiles and their workflows are bundled where `shipped_dir`
    /// looks. A `../` glob bundles under `_up_/` instead, which the lookup
    /// misses -- and `npm run gate` cannot see that, because it never runs
    /// `tauri build` (T-519).
    #[test]
    fn test_profiles_are_bundled_at_resources_profiles() {
        let raw = include_str!("../tauri.conf.json");
        let conf: serde_json::Value = serde_json::from_str(raw).expect("tauri.conf.json parses");
        let resources = conf["bundle"]["resources"]
            .as_object()
            .expect("resources is a source -> target map");
        assert_eq!(
            resources.get("../profiles/").and_then(|v| v.as_str()),
            Some("profiles/")
        );
    }

    #[test]
    fn test_csp_covers_every_asset_protocol_directive() {
        let raw = include_str!("../tauri.conf.json");
        let conf: serde_json::Value = serde_json::from_str(raw).expect("tauri.conf.json parses");
        let csp = conf["app"]["security"]["csp"]
            .as_str()
            .expect("a CSP is configured");

        for directive in ["img-src", "media-src"] {
            let sources: Vec<&str> = csp
                .split(';')
                .map(|d| d.split_whitespace().collect::<Vec<_>>())
                .find(|tokens| tokens.first() == Some(&directive))
                .unwrap_or_else(|| panic!("CSP has no {directive}: {csp}"));
            for scheme in ["asset:", "http://asset.localhost"] {
                assert!(
                    sources.contains(&scheme),
                    "CSP {directive} does not name {scheme}: {csp}"
                );
            }
        }
    }
}
