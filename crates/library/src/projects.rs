//! Projects on disk: `<app config dir>/projects/<slug>/project.json`.
//!
//! Listing mirrors `config`'s contract -- it **never fails**, and a project that
//! cannot be read becomes a warning rather than hiding every other one. Loading
//! a named project does fail, because the caller asked for that one and getting
//! a default back instead would look like data loss.
//!
//! `project.json` holds ids only: track facts live in each track's sidecar and
//! lyric text lives in each document's file (ARCHITECTURE 8).

use std::fs;
use std::path::{Path, PathBuf};

use create_core::project::Project;
use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::LibraryError;

/// Directory under the library root holding every project.
pub const PROJECTS_DIR: &str = "projects";

/// File inside a project directory holding the project record.
pub const PROJECT_FILE: &str = "project.json";

/// Longest slug [`slugify`] mints, before any uniqueness suffix.
const MAX_SLUG_LEN: usize = 48;

/// Used when a name contains no slug-safe character at all -- an all-emoji name,
/// or a name of nothing but punctuation.
const FALLBACK_SLUG: &str = "project";

/// Names Windows refuses as a directory, whatever the extension.
///
/// A user is entitled to call a project "Aux" or "Con"; the store is not
/// entitled to hand them a directory that cannot be created.
const RESERVED_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Something about listing projects the user should be told.
///
/// Every variant is recoverable: the app carries on with the projects it could
/// read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectWarning {
    /// The projects directory exists but could not be listed.
    DirUnreadable { dir: String, detail: String },
    /// A project directory has no readable `project.json`.
    Unreadable { slug: String, detail: String },
    /// A `project.json` is not a valid project record.
    Malformed { slug: String, detail: String },
    /// The record's `slug` disagreed with the directory it was found in --
    /// what a copied or renamed project directory looks like. The directory
    /// name wins, because that is where the files actually are.
    SlugMismatch { directory: String, recorded: String },
}

/// Every project the app could read, plus anything worth reporting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSet {
    /// Sorted by slug, so the order does not depend on `read_dir`.
    pub projects: Vec<Project>,
    #[serde(default)]
    pub warnings: Vec<ProjectWarning>,
}

/// The current time as RFC 3339 with second precision, e.g.
/// `"2026-08-25T20:11:04Z"`.
///
/// The one place this crate reads a clock. Every function that records a
/// timestamp takes it as an argument instead, so their tests are deterministic.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Turns a user-facing name into a directory name.
///
/// ASCII letters and digits are lowercased and kept; every other character
/// becomes a separator, runs collapse, and the result is trimmed and truncated.
/// A name that reduces to nothing, or to a name Windows reserves, still yields a
/// usable directory.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len().min(MAX_SLUG_LEN));
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= MAX_SLUG_LEN {
            break;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        return FALLBACK_SLUG.to_string();
    }
    if RESERVED_NAMES.contains(&trimmed) {
        return format!("{trimmed}-1");
    }
    trimmed.to_string()
}

/// Whether a slug may be joined onto the library root.
///
/// Deliberately a whitelist. A slug reaches this crate from the frontend, and
/// `..`, an absolute path or a separator would otherwise read and write files
/// anywhere the app can reach.
fn is_safe_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= MAX_SLUG_LEN + 8
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The directory for one project, refusing any slug that could escape `root`.
pub fn project_dir(root: &Path, slug: &str) -> Result<PathBuf, LibraryError> {
    if !is_safe_slug(slug) {
        return Err(LibraryError::UnusableName(slug.to_string()));
    }
    Ok(root.join(PROJECTS_DIR).join(slug))
}

/// Creates a project directory and its `project.json`.
///
/// The slug comes from `name`; when it is taken, a numeric suffix is added
/// rather than the existing project being opened -- two projects may share a
/// name, and silently returning someone else's is worse than a second
/// directory. `created_at` is RFC 3339 ([`now_rfc3339`]).
pub fn create_project(root: &Path, name: &str, created_at: &str) -> Result<Project, LibraryError> {
    let base = slugify(name);
    let slug = free_slug(root, &base)?;
    let project = Project::new(slug, name, created_at);
    save_project(root, &project)?;
    Ok(project)
}

/// First unused `<base>`, `<base>-2`, `<base>-3`, ... in the projects directory.
fn free_slug(root: &Path, base: &str) -> Result<String, LibraryError> {
    for n in 1..1000u32 {
        let candidate = if n == 1 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        if !project_dir(root, &candidate)?.exists() {
            return Ok(candidate);
        }
    }
    Err(LibraryError::UnusableName(base.to_string()))
}

/// Writes `project.json` atomically.
pub fn save_project(root: &Path, project: &Project) -> Result<(), LibraryError> {
    let path = project_dir(root, &project.slug)?.join(PROJECT_FILE);
    atomic::write_json(&path, project)
}

/// Loads one project by slug.
///
/// Fails when it is absent or malformed: the caller named this project, so a
/// default would be a silent substitution.
pub fn load_project(root: &Path, slug: &str) -> Result<Project, LibraryError> {
    let path = project_dir(root, slug)?.join(PROJECT_FILE);
    let text = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LibraryError::NotFound {
                kind: "project",
                id: slug.to_string(),
            }
        } else {
            LibraryError::Io(e)
        }
    })?;
    let mut project: Project = serde_json::from_str(&text)?;
    project.slug = slug.to_string();
    Ok(project)
}

/// Delete a whole project -- its entire `projects/<slug>/` directory to the OS
/// trash. Returns the projects that remain.
///
/// The most destructive delete in the app: the tree trashed holds every track,
/// sidecar, lyric document and the `project.json` itself. It is also the
/// simplest, because a project *is* its directory. There is no outer record to
/// keep in step ([`list_projects`] reads the filesystem) and no id counter to
/// preserve, so -- unlike [`delete_track`](crate::tracks::delete_track) and
/// [`delete_doc`](crate::lyrics::delete_doc) -- there is no
/// file-first/record-last order to get wrong: one trash call moves the whole
/// tree, and the project lands in the Recycle Bin as a single restorable folder.
///
/// **Existence is checked on the directory, not the record.** This does not
/// [`load_project`]: a project whose `project.json` is malformed must still be
/// deletable -- reading it first would make the one project the user most wants
/// gone the one they cannot remove. [`project_dir`] still refuses a slug that
/// could escape the root.
///
/// **The selection is not touched here.** Deleting the *selected* project leaves
/// `config.default_project_slug` naming a slug that no longer exists;
/// `projectctx::selected_project` and the frontend's `effectiveProjectSlug`
/// already resolve that to the first remaining project -- the same fallback a
/// hand-edited config hits. Reconciling config here would duplicate a resolution
/// the app already trusts and couple a filesystem op to the config store.
///
/// `trash` is the injected trash operation -- production passes
/// [`trash_to_os`](crate::tracks::trash_to_os), tests a fake -- the shape T-405
/// established for the one destructive action a test must not really perform.
pub fn delete_project<F>(root: &Path, slug: &str, trash: F) -> Result<ProjectSet, LibraryError>
where
    F: Fn(&Path) -> Result<(), LibraryError>,
{
    let dir = project_dir(root, slug)?;
    if !dir.exists() {
        return Err(LibraryError::NotFound {
            kind: "project",
            id: slug.to_string(),
        });
    }
    trash(&dir)?;
    Ok(list_projects(root))
}

/// Reads every project under `root`. **Never fails.**
///
/// A missing projects directory yields nothing and no warning -- that is the
/// normal first-run state. Directories are visited in sorted order so the list
/// is stable across platforms.
pub fn list_projects(root: &Path) -> ProjectSet {
    let dir = root.join(PROJECTS_DIR);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ProjectSet {
                projects: Vec::new(),
                warnings: Vec::new(),
            }
        }
        Err(e) => {
            return ProjectSet {
                projects: Vec::new(),
                warnings: vec![ProjectWarning::DirUnreadable {
                    dir: dir.to_string_lossy().into_owned(),
                    detail: e.to_string(),
                }],
            }
        }
    };

    let mut slugs: Vec<String> = Vec::new();
    let mut warnings: Vec<ProjectWarning> = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        slugs.push(entry.file_name().to_string_lossy().into_owned());
    }
    slugs.sort();

    let mut projects = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let path = match project_dir(root, &slug) {
            Ok(dir) => dir.join(PROJECT_FILE),
            Err(_) => {
                warnings.push(ProjectWarning::Unreadable {
                    slug: slug.clone(),
                    detail: "directory name is not a usable project slug".to_string(),
                });
                continue;
            }
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                warnings.push(ProjectWarning::Unreadable {
                    slug,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        match serde_json::from_str::<Project>(&text) {
            Ok(mut project) => {
                if project.slug != slug {
                    warnings.push(ProjectWarning::SlugMismatch {
                        directory: slug.clone(),
                        recorded: project.slug.clone(),
                    });
                    project.slug = slug;
                }
                projects.push(project);
            }
            Err(e) => warnings.push(ProjectWarning::Malformed {
                slug,
                detail: e.to_string(),
            }),
        }
    }

    ProjectSet { projects, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-08-25T20:11:04Z";

    /// Invariant: what `create_project` returns is what a later `load_project`
    /// reads back -- the directory is real and the record is in it.
    #[test]
    fn test_create_project_is_loadable_afterwards() {
        let root = tempfile::tempdir().unwrap();
        let created = create_project(root.path(), "Night Drive", NOW).unwrap();
        assert_eq!(created.slug, "night-drive");

        let loaded = load_project(root.path(), "night-drive").unwrap();
        assert_eq!(loaded, created);
        assert_eq!(loaded.name, "Night Drive");
        assert_eq!(loaded.next_lyric_seq, 1);
    }

    /// Invariant: a second project with the same name gets its own directory.
    /// Returning the first one would hand a user someone else's work.
    #[test]
    fn test_create_project_suffixes_a_taken_slug() {
        let root = tempfile::tempdir().unwrap();
        let first = create_project(root.path(), "Night Drive", NOW).unwrap();
        let second = create_project(root.path(), "Night Drive", NOW).unwrap();
        assert_eq!(first.slug, "night-drive");
        assert_eq!(second.slug, "night-drive-2");
        assert!(project_dir(root.path(), "night-drive-2").unwrap().is_dir());
    }

    /// Invariant: every character that is not ASCII alphanumeric becomes a
    /// single separator, and the result has no leading or trailing one.
    #[test]
    fn test_slugify_reduces_to_safe_characters() {
        assert_eq!(slugify("My Song / Vol. 2!"), "my-song-vol-2");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
        assert_eq!(slugify("Cafe\u{301} Latte"), "cafe-latte");
        assert!(is_safe_slug(&slugify("../../etc/passwd")));
    }

    /// Invariant: a name with nothing slug-safe in it still yields a directory
    /// name, rather than an empty path that would resolve to the parent.
    #[test]
    fn test_slugify_falls_back_when_nothing_survives() {
        assert_eq!(slugify("!!!"), FALLBACK_SLUG);
        assert_eq!(slugify(""), FALLBACK_SLUG);
    }

    /// Invariant: a project named after a DOS device still gets a creatable
    /// directory. On Windows, `con` cannot be one.
    #[test]
    fn test_slugify_avoids_windows_reserved_names() {
        assert_eq!(slugify("CON"), "con-1");
        assert_eq!(slugify("aux"), "aux-1");
        assert_eq!(slugify("COM3"), "com3-1");
        assert_eq!(slugify("console"), "console");
    }

    /// Invariant: nothing outside the library root is reachable through a slug,
    /// which arrives from the frontend.
    #[test]
    fn test_unsafe_slugs_are_refused_rather_than_joined() {
        let root = tempfile::tempdir().unwrap();
        for slug in ["../secrets", "a/b", "C:\\Windows", "Night Drive", ""] {
            let err = project_dir(root.path(), slug).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "slug {slug:?} should be refused"
            );
        }
    }

    /// Invariant: a missing projects directory is the first-run state, not a
    /// fault -- no warning is raised for it.
    #[test]
    fn test_list_projects_on_a_fresh_root_is_empty_and_silent() {
        let root = tempfile::tempdir().unwrap();
        let set = list_projects(root.path());
        assert!(set.projects.is_empty());
        assert!(set.warnings.is_empty());
    }

    /// Invariant: one unreadable project never hides the others. The bad one is
    /// reported by name so the user can act on it.
    #[test]
    fn test_list_projects_reports_a_malformed_project_and_keeps_the_rest() {
        let root = tempfile::tempdir().unwrap();
        create_project(root.path(), "Good One", NOW).unwrap();
        let bad = project_dir(root.path(), "bad-one").unwrap();
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join(PROJECT_FILE), "{ not json").unwrap();

        let set = list_projects(root.path());
        assert_eq!(set.projects.len(), 1);
        assert_eq!(set.projects[0].slug, "good-one");
        assert!(matches!(
            set.warnings.as_slice(),
            [ProjectWarning::Malformed { slug, .. }] if slug == "bad-one"
        ));
    }

    /// Invariant: a project directory copied under a new name loads under the
    /// name it actually has, and the disagreement is reported rather than
    /// silently corrected.
    #[test]
    fn test_list_projects_prefers_the_directory_name_over_a_stale_slug() {
        let root = tempfile::tempdir().unwrap();
        create_project(root.path(), "Original", NOW).unwrap();
        let copy = project_dir(root.path(), "copy-of-original").unwrap();
        fs::create_dir_all(&copy).unwrap();
        fs::copy(
            project_dir(root.path(), "original")
                .unwrap()
                .join(PROJECT_FILE),
            copy.join(PROJECT_FILE),
        )
        .unwrap();

        let set = list_projects(root.path());
        let slugs: Vec<&str> = set.projects.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, vec!["copy-of-original", "original"]);
        assert!(matches!(
            set.warnings.as_slice(),
            [ProjectWarning::SlugMismatch { directory, recorded }]
                if directory == "copy-of-original" && recorded == "original"
        ));
    }

    /// Invariant: `load_project` says a project is missing rather than handing
    /// back an empty one, which would look like the user's work vanished.
    #[test]
    fn test_load_missing_project_is_not_found() {
        let root = tempfile::tempdir().unwrap();
        let err = load_project(root.path(), "nothing-here").unwrap_err();
        assert!(matches!(
            err,
            LibraryError::NotFound {
                kind: "project",
                ..
            }
        ));
    }

    /// Invariant: saving twice leaves one readable file, not a temp file beside
    /// it -- `list_projects` walks this directory.
    #[test]
    fn test_save_project_overwrites_in_place() {
        let root = tempfile::tempdir().unwrap();
        let mut project = create_project(root.path(), "Rewrite", NOW).unwrap();
        project.name = "Rewritten".to_string();
        project.next_lyric_seq = 7;
        save_project(root.path(), &project).unwrap();

        let loaded = load_project(root.path(), "rewrite").unwrap();
        assert_eq!(loaded.name, "Rewritten");
        assert_eq!(loaded.next_lyric_seq, 7);

        let names: Vec<String> = fs::read_dir(project_dir(root.path(), "rewrite").unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![PROJECT_FILE.to_string()]);
    }

    /// Invariant: the timestamp helper produces the RFC 3339 shape the records
    /// declare, not a debug rendering of a clock type.
    #[test]
    fn test_now_rfc3339_has_the_recorded_shape() {
        let now = now_rfc3339();
        assert_eq!(now.len(), 20, "expected YYYY-MM-DDTHH:MM:SSZ, got {now}");
        assert!(now.ends_with('Z'));
        assert_eq!(&now[4..5], "-");
        assert_eq!(&now[10..11], "T");
    }

    /// A fake trasher that records what it was asked to trash and moves it into
    /// a graveyard, so a later `exists()` sees it gone -- without touching the
    /// real Recycle Bin. Handles a directory (a whole project) as one move.
    /// `RefCell` because the closure is `Fn`, not `FnMut`.
    fn recording_trasher<'a>(
        seen: &'a std::cell::RefCell<Vec<PathBuf>>,
        graveyard: &Path,
    ) -> impl Fn(&Path) -> Result<(), LibraryError> + 'a {
        let graveyard = graveyard.to_path_buf();
        move |path: &Path| {
            seen.borrow_mut().push(path.to_path_buf());
            let name = path.file_name().unwrap();
            fs::rename(path, graveyard.join(name))?;
            Ok(())
        }
    }

    /// Invariant: delete moves the whole project directory via the injected
    /// trasher and never hard-deletes it. The test that matters for the one
    /// destructive action: it asserts the trash call was made with the project
    /// directory, not merely that the directory is gone (CONVENTIONS).
    #[test]
    fn test_delete_project_trashes_the_whole_directory_and_hard_deletes_nothing() {
        let root = tempfile::tempdir().unwrap();
        create_project(root.path(), "Alpha", NOW).unwrap();
        let dir = project_dir(root.path(), "alpha").unwrap();
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        delete_project(root.path(), "alpha", recording_trasher(&seen, &graveyard)).unwrap();

        let trashed = seen.into_inner();
        assert_eq!(
            trashed.len(),
            1,
            "the whole project dir is trashed in one move"
        );
        assert_eq!(trashed[0], dir);
        // It really left the projects dir -- via the trasher, not a hard delete.
        assert!(!dir.exists());
        assert!(graveyard.join("alpha").join(PROJECT_FILE).exists());
    }

    /// Invariant: the returned list is the projects that remain, read back from
    /// the filesystem. Deleting "alpha" not "beta", and reloading, is what kills
    /// a "return before removal" or "trash the wrong dir" mutation.
    #[test]
    fn test_delete_project_returns_the_remaining_projects() {
        let root = tempfile::tempdir().unwrap();
        create_project(root.path(), "Alpha", NOW).unwrap();
        create_project(root.path(), "Beta", NOW).unwrap();
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        let remaining =
            delete_project(root.path(), "alpha", recording_trasher(&seen, &graveyard)).unwrap();

        let slugs: Vec<&str> = remaining.projects.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, vec!["beta"]);
        let reloaded: Vec<String> = list_projects(root.path())
            .projects
            .into_iter()
            .map(|p| p.slug)
            .collect();
        assert_eq!(reloaded, vec!["beta".to_string()]);
    }

    /// Invariant: deleting one project leaves the others untouched -- kills a
    /// mutation that trashes the parent `projects/` directory instead of the
    /// slug's own.
    #[test]
    fn test_delete_project_leaves_the_other_projects_intact() {
        let root = tempfile::tempdir().unwrap();
        create_project(root.path(), "Alpha", NOW).unwrap();
        let beta = create_project(root.path(), "Beta", NOW).unwrap();
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        delete_project(root.path(), "alpha", recording_trasher(&seen, &graveyard)).unwrap();

        let still_there = load_project(root.path(), "beta").unwrap();
        assert_eq!(still_there, beta);
    }

    /// Invariant: a project whose `project.json` is malformed is still
    /// deletable. Proves existence is checked on the *directory*, not via
    /// `load_project` -- kills a mutation that swaps the existence check for a
    /// `load_project` guard, which would refuse exactly this project.
    #[test]
    fn test_delete_project_of_a_malformed_project_still_deletes() {
        let root = tempfile::tempdir().unwrap();
        let dir = project_dir(root.path(), "broken").unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(PROJECT_FILE), "{ not json").unwrap();
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        delete_project(root.path(), "broken", recording_trasher(&seen, &graveyard)).unwrap();

        assert!(!dir.exists());
        assert!(list_projects(root.path()).projects.is_empty());
    }

    /// Invariant: deleting a slug with no directory is a NotFound, and the
    /// trasher is never called -- nothing is trashed for a project that is not
    /// there.
    #[test]
    fn test_delete_project_of_an_unknown_slug_is_not_found_and_trashes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        let err =
            delete_project(root.path(), "nope", recording_trasher(&seen, root.path())).unwrap_err();

        assert!(matches!(
            err,
            LibraryError::NotFound {
                kind: "project",
                ..
            }
        ));
        assert!(seen.into_inner().is_empty());
    }

    /// Invariant: a slug from the frontend that could escape the root is refused
    /// before existence is even checked, and the trasher is never called.
    #[test]
    fn test_delete_project_refuses_a_slug_that_escapes_the_root() {
        let root = tempfile::tempdir().unwrap();
        for slug in ["../secrets", "a/b", "C:\\Windows", ""] {
            let seen = std::cell::RefCell::new(Vec::new());
            let err = delete_project(root.path(), slug, recording_trasher(&seen, root.path()))
                .unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "slug {slug:?} should be refused"
            );
            assert!(seen.into_inner().is_empty());
        }
    }
}
