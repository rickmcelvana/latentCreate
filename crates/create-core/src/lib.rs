//! Domain types shared across latentCreate.
//!
//! Pure data: no I/O, no async, no file loading -- that is `library`'s job.
//!
//! [`profile`] holds the model capability schema (T-003), the abstraction the whole
//! app is built on: supporting a new music model is a JSON file, not code.
//! `Project`, `LyricDoc`, `Track`, `GenerationSpec` and `Provenance` arrive in T-003b.

pub mod profile;
pub use profile::*;

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "create-core");
    }
}
