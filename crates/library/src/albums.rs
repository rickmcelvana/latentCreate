//! Album lists within a project: `Project.albums`, edited in place.
//!
//! Albums are **name-addressed** and names are unique within a project. The
//! schema has no album id and gets none: albums have no file of their own, so
//! the name is a safe handle -- unlike a slug, it never maps to a path (T-403
//! decision, PROJECT.md). A duplicate name is refused at create and rename
//! rather than silently creating an ambiguous second list.
//!
//! The track ids an album holds are *not* re-validated when listing: a track
//! deleted after being added stays in the album, and the frontend renders it
//! as missing rather than dropping it (the T-403 trap). Adding is the one
//! moment the invariant can be protected, so `add_track` refuses an id the
//! project does not own.

use std::path::Path;

use create_core::project::{AlbumList, Project, TrackId};

use crate::projects::{load_project, save_project};
use crate::LibraryError;

/// Every album a project holds, in the order they were created.
pub fn list_albums(root: &Path, slug: &str) -> Result<Vec<AlbumList>, LibraryError> {
    Ok(load_project(root, slug)?.albums)
}

/// Creates an album and saves the project. Refuses a name already in use.
pub fn create_album(root: &Path, slug: &str, name: &str) -> Result<Vec<AlbumList>, LibraryError> {
    let name = trimmed(name)?;
    let mut project = load_project(root, slug)?;
    ensure_name_free(&project, &name)?;
    project.albums.push(AlbumList {
        name,
        tracks: Vec::new(),
    });
    save_project(root, &project)?;
    Ok(project.albums)
}

/// Renames an album. Refuses a target name another album already holds;
/// renaming to the album's own name is a harmless no-op.
pub fn rename_album(
    root: &Path,
    slug: &str,
    from: &str,
    to: &str,
) -> Result<Vec<AlbumList>, LibraryError> {
    let to = trimmed(to)?;
    let mut project = load_project(root, slug)?;
    let index = find_album(&project, from)?;
    if project
        .albums
        .iter()
        .enumerate()
        .any(|(i, album)| i != index && album.name == to)
    {
        return Err(LibraryError::DuplicateName(to.to_string()));
    }
    project.albums[index].name = to;
    save_project(root, &project)?;
    Ok(project.albums)
}

/// Adds a track to an album. The id must belong to the project -- adding is
/// the one moment a dangling id can be prevented; adding an id already present
/// is a no-op, so a double-click cannot create a duplicate row.
pub fn add_track(
    root: &Path,
    slug: &str,
    album: &str,
    track_id: &TrackId,
) -> Result<Vec<AlbumList>, LibraryError> {
    let mut project = load_project(root, slug)?;
    if !project.tracks.contains(track_id) {
        return Err(LibraryError::NotFound {
            kind: "track",
            id: track_id.0.clone(),
        });
    }
    let index = find_album(&project, album)?;
    if !project.albums[index].tracks.contains(track_id) {
        project.albums[index].tracks.push(track_id.clone());
        save_project(root, &project)?;
    }
    Ok(project.albums)
}

/// Removes a track from an album. Removing an id that is not there is a no-op.
pub fn remove_track(
    root: &Path,
    slug: &str,
    album: &str,
    track_id: &TrackId,
) -> Result<Vec<AlbumList>, LibraryError> {
    let mut project = load_project(root, slug)?;
    let index = find_album(&project, album)?;
    let before = project.albums[index].tracks.len();
    project.albums[index].tracks.retain(|id| id != track_id);
    if project.albums[index].tracks.len() != before {
        save_project(root, &project)?;
    }
    Ok(project.albums)
}

/// Reorders an album's tracks. `track_ids` must be exactly the album's current
/// tracks rearranged -- a list that drops or invents an id is refused, so a
/// stale frontend can never silently wipe part of an album.
pub fn reorder_tracks(
    root: &Path,
    slug: &str,
    album: &str,
    track_ids: &[TrackId],
) -> Result<Vec<AlbumList>, LibraryError> {
    let mut project = load_project(root, slug)?;
    let index = find_album(&project, album)?;
    if !is_permutation(&project.albums[index].tracks, track_ids) {
        return Err(LibraryError::ReorderMismatch);
    }
    project.albums[index].tracks = track_ids.to_vec();
    save_project(root, &project)?;
    Ok(project.albums)
}

/// The index of the album named `album`, or a NotFound error.
fn find_album(project: &Project, album: &str) -> Result<usize, LibraryError> {
    project
        .albums
        .iter()
        .position(|entry| entry.name == album)
        .ok_or_else(|| LibraryError::NotFound {
            kind: "album",
            id: album.to_string(),
        })
}

/// A name that is not blank, trimmed. Blank is an unusable name.
fn trimmed(name: &str) -> Result<String, LibraryError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(LibraryError::UnusableName(name.to_string()));
    }
    Ok(trimmed.to_string())
}

/// Refuses a name another album in the project already holds.
fn ensure_name_free(project: &Project, name: &str) -> Result<(), LibraryError> {
    if project.albums.iter().any(|album| album.name == name) {
        return Err(LibraryError::DuplicateName(name.to_string()));
    }
    Ok(())
}

/// Whether `candidate` is `current` rearranged: the same ids, the same count.
fn is_permutation(current: &[TrackId], candidate: &[TrackId]) -> bool {
    let mut current_sorted = current.to_vec();
    let mut candidate_sorted = candidate.to_vec();
    current_sorted.sort();
    candidate_sorted.sort();
    current_sorted == candidate_sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::create_project;

    const NOW: &str = "2026-08-25T20:11:04Z";

    fn project_with_tracks(root: &Path, ids: &[&str]) -> Project {
        let mut project = create_project(root, "Night Drive", NOW).unwrap();
        project.tracks = ids.iter().map(|id| TrackId(id.to_string())).collect();
        save_project(root, &project).unwrap();
        project
    }

    /// Invariant: a project with no albums lists none -- not an error.
    #[test]
    fn test_list_albums_is_empty_for_a_project_with_none() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &[]);
        assert!(list_albums(root.path(), &project.slug).unwrap().is_empty());
    }

    /// Invariant: what create returns is what a reload reads back -- the
    /// project record was saved, not just mutated in memory.
    #[test]
    fn test_create_album_registers_and_persists() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &[]);

        let albums = create_album(root.path(), &project.slug, "Night Drive").unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].name, "Night Drive");
        assert!(albums[0].tracks.is_empty());

        let reloaded = list_albums(root.path(), &project.slug).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].name, "Night Drive");
    }

    /// Invariant: a duplicate name is refused, so "open this album" is never
    /// ambiguous. Both copies would otherwise render identically.
    #[test]
    fn test_create_album_refuses_a_duplicate_name() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &[]);
        create_album(root.path(), &project.slug, "Demo").unwrap();

        let err = create_album(root.path(), &project.slug, "Demo").unwrap_err();
        assert!(matches!(err, LibraryError::DuplicateName(_)));
        assert_eq!(list_albums(root.path(), &project.slug).unwrap().len(), 1);
    }

    /// Invariant: a blank name cannot create a nameless album row.
    #[test]
    fn test_create_album_refuses_a_blank_name() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &[]);

        for name in ["", "   "] {
            let err = create_album(root.path(), &project.slug, name).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "name {name:?} should be refused"
            );
        }
        assert!(list_albums(root.path(), &project.slug).unwrap().is_empty());
    }

    /// Invariant: rename changes the stored name and persists it.
    #[test]
    fn test_rename_album_persists_the_new_name() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &[]);
        create_album(root.path(), &project.slug, "Old").unwrap();

        let albums = rename_album(root.path(), &project.slug, "Old", "New").unwrap();
        assert_eq!(albums[0].name, "New");

        let reloaded = list_albums(root.path(), &project.slug).unwrap();
        assert_eq!(reloaded[0].name, "New");
    }

    /// Invariant: renaming onto a name another album holds is refused, and
    /// nothing changes.
    #[test]
    fn test_rename_album_refuses_a_name_another_album_holds() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &[]);
        create_album(root.path(), &project.slug, "A").unwrap();
        create_album(root.path(), &project.slug, "B").unwrap();

        let err = rename_album(root.path(), &project.slug, "A", "B").unwrap_err();
        assert!(matches!(err, LibraryError::DuplicateName(_)));
        let reloaded = list_albums(root.path(), &project.slug).unwrap();
        assert_eq!(reloaded[0].name, "A");
        assert_eq!(reloaded[1].name, "B");
    }

    /// Invariant: renaming an album to its own name is a harmless no-op, not a
    /// spurious duplicate error -- the frontend sends the current name through
    /// an edit box and the user may save without changing anything.
    #[test]
    fn test_rename_album_to_itself_is_allowed() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &[]);
        create_album(root.path(), &project.slug, "Keep").unwrap();

        let albums = rename_album(root.path(), &project.slug, "Keep", "Keep").unwrap();
        assert_eq!(albums[0].name, "Keep");
    }

    /// Invariant: an added track is in the album, in order, and persisted.
    #[test]
    fn test_add_track_appends_and_persists() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001", "tr-0002"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        let id = TrackId("tr-0001".to_string());

        let albums = add_track(root.path(), &project.slug, "A", &id).unwrap();
        assert_eq!(albums[0].tracks, vec![id.clone()]);

        let reloaded = list_albums(root.path(), &project.slug).unwrap();
        assert_eq!(reloaded[0].tracks, vec![id]);
    }

    /// Invariant: a track the project does not own cannot be added -- the
    /// album would otherwise start life pointing at audio that is not there.
    #[test]
    fn test_add_track_refuses_an_id_the_project_does_not_own() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001"]);
        create_album(root.path(), &project.slug, "A").unwrap();

        let err = add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-9999".to_string()),
        )
        .unwrap_err();
        assert!(matches!(err, LibraryError::NotFound { kind: "track", .. }));
        assert!(list_albums(root.path(), &project.slug).unwrap()[0]
            .tracks
            .is_empty());
    }

    /// Invariant: adding the same track twice leaves one entry -- the no-op
    /// keeps a double-click from creating a duplicate row.
    #[test]
    fn test_add_track_twice_leaves_one_entry() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        let id = TrackId("tr-0001".to_string());

        add_track(root.path(), &project.slug, "A", &id).unwrap();
        let albums = add_track(root.path(), &project.slug, "A", &id).unwrap();

        assert_eq!(albums[0].tracks, vec![id]);
    }

    /// Invariant: remove drops exactly the named id, keeps the rest, persists.
    #[test]
    fn test_remove_track_drops_only_that_id() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001", "tr-0002"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0001".to_string()),
        )
        .unwrap();
        add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0002".to_string()),
        )
        .unwrap();

        let albums = remove_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0001".to_string()),
        )
        .unwrap();
        assert_eq!(albums[0].tracks, vec![TrackId("tr-0002".to_string())]);

        let reloaded = list_albums(root.path(), &project.slug).unwrap();
        assert_eq!(reloaded[0].tracks, vec![TrackId("tr-0002".to_string())]);
    }

    /// Invariant: removing an id that is not in the album changes nothing.
    #[test]
    fn test_remove_track_absent_is_a_no_op() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001"]);
        create_album(root.path(), &project.slug, "A").unwrap();

        let albums = remove_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0002".to_string()),
        )
        .unwrap();
        assert!(albums[0].tracks.is_empty());
    }

    /// Invariant: reorder writes the given order and persists it.
    #[test]
    fn test_reorder_tracks_persists_the_new_order() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001", "tr-0002"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0001".to_string()),
        )
        .unwrap();
        add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0002".to_string()),
        )
        .unwrap();

        let albums = reorder_tracks(
            root.path(),
            &project.slug,
            "A",
            &[
                TrackId("tr-0002".to_string()),
                TrackId("tr-0001".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            albums[0].tracks,
            vec![
                TrackId("tr-0002".to_string()),
                TrackId("tr-0001".to_string())
            ]
        );

        let reloaded = list_albums(root.path(), &project.slug).unwrap();
        assert_eq!(
            reloaded[0].tracks,
            vec![
                TrackId("tr-0002".to_string()),
                TrackId("tr-0001".to_string())
            ]
        );
    }

    /// Invariant: a reorder that drops an id is refused, so a stale frontend
    /// can never silently wipe part of an album.
    #[test]
    fn test_reorder_tracks_refuses_a_dropped_id() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001", "tr-0002"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0001".to_string()),
        )
        .unwrap();
        add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0002".to_string()),
        )
        .unwrap();

        let err = reorder_tracks(
            root.path(),
            &project.slug,
            "A",
            &[TrackId("tr-0001".to_string())],
        )
        .unwrap_err();
        assert!(matches!(err, LibraryError::ReorderMismatch));
        assert_eq!(
            list_albums(root.path(), &project.slug).unwrap()[0]
                .tracks
                .len(),
            2
        );
    }

    /// Invariant: a reorder that invents an id is refused for the same reason.
    #[test]
    fn test_reorder_tracks_refuses_an_invented_id() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        add_track(
            root.path(),
            &project.slug,
            "A",
            &TrackId("tr-0001".to_string()),
        )
        .unwrap();

        let err = reorder_tracks(
            root.path(),
            &project.slug,
            "A",
            &[
                TrackId("tr-0001".to_string()),
                TrackId("tr-9999".to_string()),
            ],
        )
        .unwrap_err();
        assert!(matches!(err, LibraryError::ReorderMismatch));
    }

    /// Invariant: every mutation names the album it edits, and a name that is
    /// not there is an error -- never a silent no-op that looks like success.
    #[test]
    fn test_an_unknown_album_name_errors_for_every_mutation() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001"]);
        let id = TrackId("tr-0001".to_string());

        let rename = rename_album(root.path(), &project.slug, "nope", "New");
        let add = add_track(root.path(), &project.slug, "nope", &id);
        let remove = remove_track(root.path(), &project.slug, "nope", &id);
        let reorder = reorder_tracks(root.path(), &project.slug, "nope", &[id]);

        for result in [rename, add, remove, reorder] {
            let err = result.unwrap_err();
            assert!(
                matches!(err, LibraryError::NotFound { kind: "album", .. }),
                "expected album NotFound, got {err:?}"
            );
        }
    }
}
