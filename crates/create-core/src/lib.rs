//! Domain types shared across latentCreate.
//!
//! Pure data: no I/O, no async, no file loading -- that is `library`'s job.
//!
//! [`profile`] holds the model capability schema (T-003), the abstraction the whole
//! app is built on: supporting a new music model is a JSON file, not code.
//! [`generation`] holds what the user asked for before it is fanned out to slots.
//! [`project`] holds the library's project, lyric and track-id types.
//! [`provenance`] holds the reproducible recipe for one generated asset.
//! [`readiness`] decides whether a profile's model files are present.

pub mod generation;
pub mod profile;
pub mod project;
pub mod provenance;
pub mod readiness;
pub mod suggestions;

pub use generation::*;
pub use profile::*;
pub use project::*;
pub use provenance::*;
pub use readiness::*;
pub use suggestions::*;

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "create-core");
    }
}
