//! Domain types shared across latentCreate.
//!
//! Pure data: no I/O, no async, no file loading -- that is `library`'s job.
//!
//! [`profile`] holds the model capability schema (T-003), the abstraction the whole
//! app is built on: supporting a new music model is a JSON file, not code.
//! [`generation`] holds what the user asked for before it is fanned out to slots.
//! [`graph`] holds pure workflow graph edits that slots cannot express (T-305a).
//! [`audit`] checks whether a resolved slot write can actually reach the engine (T-306a).
//! [`loras`] turns the installed-LoRA list into something a person can pick from (T-307).
//! [`project`] holds the library's project, lyric and track-id types.
//! [`provenance`] holds the reproducible recipe for one generated asset.
//! [`readiness`] decides whether a profile's model files are present.
//! [`lyrics`] holds the lyric brief and the prompt assembled from it.

pub mod audit;
pub mod generation;
pub mod graph;
pub mod loras;
pub mod lyrics;
pub mod profile;
pub mod project;
pub mod provenance;
pub mod readiness;

pub use audit::*;
pub use generation::*;
pub use graph::*;
pub use loras::*;
pub use lyrics::*;
pub use profile::*;
pub use project::*;
pub use provenance::*;
pub use readiness::*;

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "create-core");
    }
}
