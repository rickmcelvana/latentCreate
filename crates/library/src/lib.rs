//! On-disk store: projects, tracks, provenance sidecars, config.
//!
//! JSON files under the app data dir, no database (ARCHITECTURE.md §8).
//! Secrets live in the OS keychain, never in config. Populated by T-004.

pub mod config;
pub mod secrets;

/// Re-export of [`config::Config`].
pub use config::Config;
/// Re-export of [`config::ConfigWarning`].
pub use config::ConfigWarning;
/// Re-export of [`config::LoadedConfig`].
pub use config::LoadedConfig;
/// Re-export of [`secrets::SecretKey`].
pub use secrets::SecretKey;

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
