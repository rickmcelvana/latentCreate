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

/// The project every command writes to, resolved from the persisted selection.
///
/// The selection is `config.default_project_slug` when that project still
/// exists, else the first project in slug order; a fresh root has none, so
/// `My First Song` is created. Deterministic, so a restart lands on the same
/// project. The fallback chain is the whole point: a configured slug whose
/// project has been deleted (or a garbage slug in a hand-edited config)
/// degrades to the first project, never to an error, and never to a different
/// project per caller.
pub fn selected_project(root: &Path) -> Result<Project, library::LibraryError> {
    let configured = library::config::load(root).config.default_project_slug;
    resolve_selected(root, configured.as_deref())
}

/// The resolution chain, pure and testable: the configured slug when it
/// exists, else the first project, else a freshly created
/// [`DEFAULT_PROJECT_NAME`].
fn resolve_selected(
    root: &Path,
    configured: Option<&str>,
) -> Result<Project, library::LibraryError> {
    if let Some(slug) = configured {
        if let Ok(project) = library::projects::load_project(root, slug) {
            return Ok(project);
        }
    }
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
    use std::path::Path;

    const NOW: &str = "2026-08-30T10:00:00Z";

    fn write_config(root: &Path, slug: Option<&str>) {
        let config = library::Config {
            default_project_slug: slug.map(str::to_string),
            ..library::Config::default()
        };
        library::config::save(root, &config).unwrap();
    }

    /// Protects: the configured slug beats the first project.
    ///
    /// The trap this task exists to avoid. The old `default_project` returned
    /// the first project in slug order, and a half-done refactor keeps doing
    /// that while claiming to honour the selection -- every caller "resolves
    /// to a project", and none of them to the *selected* one.
    #[test]
    fn test_selected_project_uses_the_configured_slug() {
        let root = tempfile::tempdir().unwrap();
        library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        let beta =
            library::projects::create_project(root.path(), "Beta", "2026-08-30T10:00:01Z").unwrap();
        write_config(root.path(), Some("beta"));

        let project = selected_project(root.path()).unwrap();
        assert_eq!(project.slug, beta.slug);
        assert_ne!(project.slug, "alpha");
    }

    /// Protects: a configured slug whose project has gone falls to the first
    /// project rather than erroring -- the phase file's specified fallback.
    #[test]
    fn test_selected_project_falls_back_when_the_configured_one_is_gone() {
        let root = tempfile::tempdir().unwrap();
        let alpha = library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        write_config(root.path(), Some("deleted-project"));

        let project = selected_project(root.path()).unwrap();
        assert_eq!(project.slug, alpha.slug);
    }

    /// Protects: a garbage slug in a hand-edited config cannot break the app.
    /// It degrades to the same fallback as a missing project, silently and
    /// consistently -- `load_project` refuses the slug, the chain falls
    /// through, and every caller still gets the same project.
    #[test]
    fn test_a_garbage_configured_slug_degrades_to_the_fallback() {
        let root = tempfile::tempdir().unwrap();
        let alpha = library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        write_config(root.path(), Some("../../etc/passwd"));

        let project = selected_project(root.path()).unwrap();
        assert_eq!(project.slug, alpha.slug);
    }

    /// Protects: the default project exists after the first open, and a second
    /// open lands on the same one rather than minting a second. Carried over
    /// from the pre-selection `default_project`, now through the config path.
    #[test]
    fn test_selected_project_is_created_once_and_reused() {
        let root = tempfile::tempdir().unwrap();
        let first = selected_project(root.path()).unwrap();
        assert_eq!(first.name, DEFAULT_PROJECT_NAME);
        assert_eq!(first.slug, "my-first-song");

        let again = selected_project(root.path()).unwrap();
        assert_eq!(again.slug, "my-first-song");
        assert_eq!(again.created_at, first.created_at);
    }

    /// Protects: lyrics and tracks resolving to different projects.
    ///
    /// The reason this module exists. Both callers go through one function, so
    /// this asserts the property that matters rather than the call graph: two
    /// resolutions against the same root with the same selection are the same
    /// project -- and that project is the *selected* one, not whichever came
    /// first.
    #[test]
    fn test_every_caller_resolves_to_the_same_project() {
        let root = tempfile::tempdir().unwrap();
        library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        library::projects::create_project(root.path(), "Beta", NOW).unwrap();
        write_config(root.path(), Some("beta"));

        let for_lyrics = selected_project(root.path()).unwrap();
        let for_tracks = selected_project(root.path()).unwrap();
        assert_eq!(for_lyrics.slug, for_tracks.slug);
        assert_eq!(for_lyrics.slug, "beta");
    }
}
