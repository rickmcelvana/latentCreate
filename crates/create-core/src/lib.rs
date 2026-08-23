//! Domain types shared across latentCreate.
//!
//! Pure data: `Project`, `Track`, `LyricDoc`, `GenerationSpec`, `ModelProfile`,
//! `Provenance`. No I/O, no async. Populated by T-003.

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "create-core");
    }
}
