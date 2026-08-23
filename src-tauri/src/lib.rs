//! latentCreate desktop shell.
//!
//! The single place that wires the workspace crates to the frontend. Tauri
//! commands and events live here; domain logic belongs in the crates
//! (ARCHITECTURE.md §2).

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
        .invoke_handler(tauri::generate_handler![app_version])
        .run(tauri::generate_context!())
        .expect("error while running latentCreate");
}

/// Returns the shell's crate version, so the frontend can prove the bridge
/// round-trips before any real command exists.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::app_version;

    #[test]
    fn test_app_version_matches_crate_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }
}
