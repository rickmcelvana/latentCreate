//! On-disk store: projects, tracks, provenance sidecars, config.
//!
//! JSON files under the app data dir, no database (ARCHITECTURE.md section 8).
//! Secrets live in the OS keychain, never in config. Populated by T-004.

pub mod albums;
mod atomic;
pub mod config;
pub mod lyrics;
pub mod profiles;
pub mod projects;
pub mod secrets;
pub mod tracks;

/// Re-export of [`config::Config`].
pub use config::Config;
/// Re-export of [`config::ConfigWarning`].
pub use config::ConfigWarning;
/// Re-export of [`config::LoadedConfig`].
pub use config::LoadedConfig;
/// Re-export of [`lyrics::LyricDocSet`].
pub use lyrics::LyricDocSet;
/// Re-export of [`lyrics::LyricWarning`].
pub use lyrics::LyricWarning;
/// Re-export of [`profiles::LoadedProfile`].
pub use profiles::LoadedProfile;
/// Re-export of [`profiles::ProfileSet`].
pub use profiles::ProfileSet;
/// Re-export of [`profiles::ProfileSource`].
pub use profiles::ProfileSource;
/// Re-export of [`profiles::ProfileWarning`].
pub use profiles::ProfileWarning;
/// Re-export of [`projects::ProjectSet`].
pub use projects::ProjectSet;
/// Re-export of [`projects::ProjectWarning`].
pub use projects::ProjectWarning;
/// Re-export of [`secrets::SecretKey`].
pub use secrets::SecretKey;
/// Re-export of [`tracks::TrackSet`].
pub use tracks::TrackSet;
/// Re-export of [`tracks::TrackWarning`].
pub use tracks::TrackWarning;

use thiserror::Error;

/// Anything that can go wrong reading or writing the library.
#[derive(Debug, Error)]
pub enum LibraryError {
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Malformed JSON on write, or a value that cannot be serialised.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// OS keychain failure.
    #[error("keychain error: {0}")]
    Keyring(#[from] keyring::Error),
    /// The frontend named a secret outside the whitelist.
    #[error("unknown secret name: {0}")]
    UnknownSecret(String),
    /// A project or lyric document that is not on disk. Distinct from an I/O
    /// failure: the caller named something that does not exist, and returning a
    /// default instead would look like the user's work had vanished.
    #[error("{kind} not found: {id}")]
    NotFound {
        /// What was looked for, e.g. `"project"`.
        kind: &'static str,
        /// The slug or id that was asked for.
        id: String,
    },
    /// A name that cannot become a directory here -- an unsafe slug from the
    /// frontend, or a base name with a thousand collisions.
    #[error("unusable name: {0}")]
    UnusableName(String),
    /// A name another album in the same project already holds.
    #[error("an album named {0} already exists; choose another name")]
    DuplicateName(String),
    /// A reorder that is not the album's current tracks rearranged.
    #[error("the new order must be the same tracks, in a different order")]
    ReorderMismatch,
    /// Moving a file to the OS trash failed. Carries the crate's own message
    /// rather than the `trash::Error` type, so the boundary stays serde-simple.
    #[error("could not move to trash: {0}")]
    Trash(String),
}

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "library");
    }
}
