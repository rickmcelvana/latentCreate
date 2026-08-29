//! Which project the app is working in.
//!
//! Shared rather than duplicated, and that is the whole point of the module:
//! lyrics and tracks must agree on where they are filed. Two copies of this
//! rule would drift on the first change, and the symptom -- a track saved into
//! one project while the lyrics it was generated from sit in another -- would
//! look like data loss rather than a policy disagreement.
//!
//! Policy, not storage, so it lives here and not in `library`.

use std::path::Path;

use create_core::project::Project;

/// Name of the project things are written under, before the user has named one.
pub const DEFAULT_PROJECT_NAME: &str = "My First Song";

/// The project the app writes to, creating it on first use.
///
/// The default is the first project in slug order; a fresh root has none, so
/// one is created. Deterministic so a restart lands on the same project.
pub fn default_project(root: &Path) -> Result<Project, library::LibraryError> {
    let set = library::projects::list_projects(root);
    if let Some(first) = set.projects.into_iter().next() {
        return Ok(first);
    }
    library::projects::create_project(
        root,
        DEFAULT_PROJECT_NAME,
        &library::projects::now_rfc3339(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protects: the default project exists after the first open, and a second
    /// open lands on the same one rather than minting a second.
    #[test]
    fn test_default_project_is_created_once_and_reused() {
        let root = tempfile::tempdir().unwrap();
        let first = default_project(root.path()).unwrap();
        assert_eq!(first.name, DEFAULT_PROJECT_NAME);
        assert_eq!(first.slug, "my-first-song");

        let again = default_project(root.path()).unwrap();
        assert_eq!(again.slug, "my-first-song");
        assert_eq!(again.created_at, first.created_at);
    }

    /// Protects: lyrics and tracks resolving to different projects.
    ///
    /// The reason this module exists. Both callers go through one function, so
    /// this asserts the property that matters rather than the call graph: two
    /// resolutions against the same root are the same project.
    #[test]
    fn test_every_caller_resolves_to_the_same_project() {
        let root = tempfile::tempdir().unwrap();
        let for_lyrics = default_project(root.path()).unwrap();
        let for_tracks = default_project(root.path()).unwrap();
        assert_eq!(for_lyrics.slug, for_tracks.slug);
    }
}
