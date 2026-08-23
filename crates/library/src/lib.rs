//! On-disk store: projects, tracks, provenance sidecars, config.
//!
//! JSON files under the app data dir, no database (ARCHITECTURE.md §8).
//! Secrets live in the OS keychain, never in config. Populated by T-004.

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "library");
    }
}
