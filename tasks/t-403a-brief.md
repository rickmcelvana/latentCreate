# T-403a: album lists -- the backend (`library::albums` + commands)

**Depends:** T-402 (player; the Library view it lands in) | **Crate/dir:** library + src-tauri
**Files to create/modify:**
- `crates/library/src/albums.rs` (new)
- `crates/library/src/lib.rs` (modify: `pub mod albums;`)
- `src-tauri/src/albums.rs` (new)
- `src-tauri/src/lib.rs` (modify: `mod albums;` + six command registrations)

## Goal

`Project.albums` (already in the schema as `Vec<AlbumList>` with `name` + `tracks:
Vec<TrackId>`) becomes editable: create, rename, add track, remove track, reorder -- each a
load-project -> mutate -> save-project function in `library`, with six thin Tauri commands over
them. This is the backend half of the "album list" Phase 4 milestone line.

## Design decision (recorded, not guessed)

**Albums are name-addressed, and names are unique within a project.** The schema has no album id
and this task does not add one. An album id would exist only to address an in-record list, and the
one thing ids exist for here -- a filesystem-safe handle -- is exactly what a name is *not* needed
to be, because albums never map to a path. Uniqueness is enforced at create and rename instead: a
duplicate name is refused with a "say what to do next" error, so "open this album" is never
ambiguous. (Contrast: track and lyric ids are minted because they become *filenames*; albums stay
inside `project.json`.)

**A reorder is a full-order replace, validated as a permutation.** The frontend computes the new
order after an up/down move and sends the whole list; the backend refuses any list that is not
exactly the album's current tracks rearranged. A stale frontend can never silently wipe part of an
album, and there is no move-one-off-by-one index arithmetic to get wrong on the wire.

**`add_track` refuses an id the project does not own.** Adding is the one moment a dangling id can
be prevented. *Deleting* is the only legitimate source of a dangling id, and the frontend renders
those as missing (the T-403 trap) -- so the backend validates on add and stays passive on list.

## Spec

### `crates/library/src/albums.rs` (new)

```rust
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

        let err = add_track(root.path(), &project.slug, "A", &TrackId("tr-9999".to_string()))
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
        add_track(root.path(), &project.slug, "A", &TrackId("tr-0001".to_string())).unwrap();
        add_track(root.path(), &project.slug, "A", &TrackId("tr-0002".to_string())).unwrap();

        let albums = remove_track(root.path(), &project.slug, "A", &TrackId("tr-0001".to_string()))
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

        let albums = remove_track(root.path(), &project.slug, "A", &TrackId("tr-0002".to_string()))
            .unwrap();
        assert!(albums[0].tracks.is_empty());
    }

    /// Invariant: reorder writes the given order and persists it.
    #[test]
    fn test_reorder_tracks_persists_the_new_order() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001", "tr-0002"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        add_track(root.path(), &project.slug, "A", &TrackId("tr-0001".to_string())).unwrap();
        add_track(root.path(), &project.slug, "A", &TrackId("tr-0002".to_string())).unwrap();

        let albums = reorder_tracks(
            root.path(),
            &project.slug,
            "A",
            &[TrackId("tr-0002".to_string()), TrackId("tr-0001".to_string())],
        )
        .unwrap();
        assert_eq!(
            albums[0].tracks,
            vec![TrackId("tr-0002".to_string()), TrackId("tr-0001".to_string())]
        );

        let reloaded = list_albums(root.path(), &project.slug).unwrap();
        assert_eq!(
            reloaded[0].tracks,
            vec![TrackId("tr-0002".to_string()), TrackId("tr-0001".to_string())]
        );
    }

    /// Invariant: a reorder that drops an id is refused, so a stale frontend
    /// can never silently wipe part of an album.
    #[test]
    fn test_reorder_tracks_refuses_a_dropped_id() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001", "tr-0002"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        add_track(root.path(), &project.slug, "A", &TrackId("tr-0001".to_string())).unwrap();
        add_track(root.path(), &project.slug, "A", &TrackId("tr-0002".to_string())).unwrap();

        let err = reorder_tracks(
            root.path(),
            &project.slug,
            "A",
            &[TrackId("tr-0001".to_string())],
        )
        .unwrap_err();
        assert!(matches!(err, LibraryError::ReorderMismatch));
        assert_eq!(
            list_albums(root.path(), &project.slug).unwrap()[0].tracks.len(),
            2
        );
    }

    /// Invariant: a reorder that invents an id is refused for the same reason.
    #[test]
    fn test_reorder_tracks_refuses_an_invented_id() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_tracks(root.path(), &["tr-0001"]);
        create_album(root.path(), &project.slug, "A").unwrap();
        add_track(root.path(), &project.slug, "A", &TrackId("tr-0001".to_string())).unwrap();

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
```

### `crates/library/src/lib.rs` (modify)

Add the module declaration beside the others (alphabetical, after `atomic`):

```rust
mod atomic;
pub mod albums;
pub mod config;
```

### `crates/library/src/lib.rs` -- `LibraryError` (modify)

Add two variants to the existing enum, keeping the `thiserror` style:

```rust
    /// A name another album in the same project already holds.
    #[error("an album named {0} already exists; choose another name")]
    DuplicateName(String),
    /// A reorder that is not the album's current tracks rearranged.
    #[error("the new order must be the same tracks, in a different order")]
    ReorderMismatch,
```

### `src-tauri/src/albums.rs` (new)

Thin glue, exactly like `tracks.rs` and `projects.rs`: resolve the selected project, call
`library::albums`, map errors. **No command-level tests** -- the chain
`selected_project` -> `library::albums` shares one `project` value, and every rule worth testing
(reorder permutation, duplicate names, foreign track ids) lives in `library` where the tests are.

```rust
//! Tauri commands over album lists.
//!
//! Thin glue like `tracks` and `projects`: resolve the selected project, call
//! `library::albums`, map errors. No command-level tests -- the chain
//! `selected_project` -> `library::albums` shares one `project` value, and the
//! rules (reorder permutation, duplicate names, foreign track ids) live in
//! `library` where their tests reach them.

use create_core::project::{AlbumList, TrackId};
use tauri::State;

use crate::projectctx::selected_project;
use crate::ConfigDir;

/// The result every album command returns: the project's albums after the
/// write, so the frontend never has to guess whether it landed.
type AlbumsResult = Result<Vec<AlbumList>, String>;

/// Every album in the selected project, in creation order.
#[tauri::command]
pub fn albums_list(config_dir: State<'_, ConfigDir>) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::list_albums(&config_dir.0, &project.slug).map_err(|e| e.to_string())
}

/// Create an album in the selected project.
#[tauri::command]
pub fn album_create(config_dir: State<'_, ConfigDir>, name: String) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::create_album(&config_dir.0, &project.slug, &name).map_err(|e| e.to_string())
}

/// Rename an album in the selected project.
#[tauri::command]
pub fn album_rename(config_dir: State<'_, ConfigDir>, from: String, to: String) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::rename_album(&config_dir.0, &project.slug, &from, &to)
        .map_err(|e| e.to_string())
}

/// Add a track to an album in the selected project.
#[tauri::command]
pub fn album_add_track(
    config_dir: State<'_, ConfigDir>,
    album: String,
    track_id: String,
) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let track_id = TrackId(track_id);
    library::albums::add_track(&config_dir.0, &project.slug, &album, &track_id)
        .map_err(|e| e.to_string())
}

/// Remove a track from an album in the selected project.
#[tauri::command]
pub fn album_remove_track(
    config_dir: State<'_, ConfigDir>,
    album: String,
    track_id: String,
) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let track_id = TrackId(track_id);
    library::albums::remove_track(&config_dir.0, &project.slug, &album, &track_id)
        .map_err(|e| e.to_string())
}

/// Reorder an album's tracks in the selected project. `track_ids` is the full
/// new order; the backend refuses anything that is not the current tracks
/// rearranged.
#[tauri::command]
pub fn album_reorder(
    config_dir: State<'_, ConfigDir>,
    album: String,
    track_ids: Vec<TrackId>,
) -> AlbumsResult {
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    library::albums::reorder_tracks(&config_dir.0, &project.slug, &album, &track_ids)
        .map_err(|e| e.to_string())
}
```

### `src-tauri/src/lib.rs` (modify)

Add `mod albums;` to the module list (alphabetical, before `comfy`), and register the six
commands in `invoke_handler` (after the `projects::` entries):

```rust
mod albums;
mod comfy;
```

```rust
            projects::projects_list,
            projects::projects_create,
            albums::albums_list,
            albums::album_create,
            albums::album_rename,
            albums::album_add_track,
            albums::album_remove_track,
            albums::album_reorder,
```

## Acceptance criteria

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` green.
- [ ] library goes **58 -> 74** tests (16 new, each with the invariant stated above).
- [ ] No changes outside the four listed files; `create_core` is untouched (no schema change).
- [ ] No `unwrap()`/`expect()` outside tests; errors typed via `LibraryError` variants.
- [ ] Mutation checks for the two flagship guards: dropping `is_permutation` fails
      `test_reorder_tracks_refuses_a_dropped_id` (and the invented-id test); dropping the
      `ensure_name_free` call fails `test_create_album_refuses_a_duplicate_name`.

## Out of scope

- Album delete (not in the phase scope; the `mint_track_id` invariant note in
  `create_core::project.rs` already anticipates it).
- Any schema change, frontend, or UI -- those are T-403b/T-403c.
- Renaming tracks or projects.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/library/src/projects.rs --read crates/library/src/lib.rs --read crates/create-core/src/project.rs --read src-tauri/src/tracks.rs --read src-tauri/src/projects.rs --read src-tauri/src/projectctx.rs --read src-tauri/src/lib.rs --file crates/library/src/albums.rs --file crates/library/src/lib.rs --file src-tauri/src/albums.rs --file src-tauri/src/lib.rs
```
