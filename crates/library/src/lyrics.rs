//! Lyric documents on disk: `<app config dir>/projects/<slug>/lyrics/<doc-id>.json`.
//!
//! **One file per document, every version inside it** (PROJECT.md decisions log,
//! 2026-08-25). `LyricDoc` already models its versions inline; splitting the text
//! into per-version files would put a version's words in one place and its
//! `source`, `created_at` and approval in another, which is the two-files-
//! disagreeing hazard ARCHITECTURE 8's track rule exists to prevent -- for a few
//! kilobytes.
//!
//! `project.json` holds the ordered ids and nothing else, so this module is the
//! only thing that reads or writes lyric text.

use std::fs;
use std::path::{Path, PathBuf};

use create_core::generation::LyricDocId;
use create_core::project::{LyricDoc, Project};
use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::projects::project_dir;
use crate::LibraryError;

/// Directory inside a project holding its lyric documents.
pub const LYRICS_DIR: &str = "lyrics";

/// Prefix every minted document id carries.
const ID_PREFIX: &str = "ld-";

/// Digits a minted id is padded to.
///
/// Padded so that sorting the directory by name matches the order the documents
/// were created in -- `ld-10` would otherwise sort before `ld-2`.
const ID_DIGITS: usize = 4;

/// Something about listing a project's lyric documents the user should be told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LyricWarning {
    /// `project.json` lists an id with no file behind it. Surfaced rather than
    /// dropped: a document the user wrote is missing, and silence would read as
    /// "there was never one".
    Missing { id: String },
    /// The file exists but could not be read.
    Unreadable { id: String, detail: String },
    /// The file is not a valid lyric document.
    Malformed { id: String, detail: String },
}

/// Every lyric document a project could offer, plus anything worth reporting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricDocSet {
    /// In `Project::lyrics` order -- the order the user created them.
    pub docs: Vec<LyricDoc>,
    #[serde(default)]
    pub warnings: Vec<LyricWarning>,
}

/// Whether an id may be joined onto a project directory.
///
/// A whitelist, for the same reason project slugs use one: ids reach this crate
/// from the frontend, and `..` or a separator would read and write outside the
/// library.
fn is_safe_doc_id(id: &str) -> bool {
    id.len() > ID_PREFIX.len()
        && id.len() <= ID_PREFIX.len() + 12
        && id.starts_with(ID_PREFIX)
        && id[ID_PREFIX.len()..].chars().all(|c| c.is_ascii_digit())
}

/// The lyrics directory for one project.
pub fn lyrics_dir(root: &Path, slug: &str) -> Result<PathBuf, LibraryError> {
    Ok(project_dir(root, slug)?.join(LYRICS_DIR))
}

/// The file backing one document, refusing any id that could escape the project.
pub fn doc_path(root: &Path, slug: &str, id: &LyricDocId) -> Result<PathBuf, LibraryError> {
    if !is_safe_doc_id(&id.0) {
        return Err(LibraryError::UnusableName(id.0.clone()));
    }
    Ok(lyrics_dir(root, slug)?.join(format!("{}.json", id.0)))
}

/// Takes the next document id for `project` and advances its counter.
///
/// The counter is the only source of ids. Deriving one from the files present
/// would hand a deleted document's id to a later one, and a track's provenance
/// `LyricRef` would then resolve to lyrics nobody wrote for it.
pub fn mint_doc_id(project: &mut Project) -> LyricDocId {
    let id = LyricDocId(format!(
        "{ID_PREFIX}{:0width$}",
        project.next_lyric_seq,
        width = ID_DIGITS
    ));
    project.next_lyric_seq = project.next_lyric_seq.saturating_add(1);
    id
}

/// Creates an empty document, registers it on `project`, and writes both.
///
/// The document file is written first. If saving the project then fails, the
/// result is a file nothing references -- invisible to [`list_docs`], which
/// walks `Project::lyrics` -- rather than a project referencing a file that is
/// not there, which is the failure that loses a user's lyrics.
///
/// `project` is left as it was saved; the caller holds the updated record.
pub fn create_doc(
    root: &Path,
    project: &mut Project,
    title: Option<String>,
) -> Result<LyricDoc, LibraryError> {
    let id = mint_doc_id(project);
    let doc = LyricDoc {
        id: id.clone(),
        title,
        versions: Vec::new(),
        approved: None,
    };
    save_doc(root, &project.slug, &doc)?;
    project.lyrics.push(id);
    crate::projects::save_project(root, project)?;
    Ok(doc)
}

/// Writes one document atomically, versions and all.
pub fn save_doc(root: &Path, slug: &str, doc: &LyricDoc) -> Result<(), LibraryError> {
    let path = doc_path(root, slug, &doc.id)?;
    atomic::write_json(&path, doc)
}

/// Loads one document by id.
///
/// Fails when it is absent: the caller named this document, and an empty one
/// would look like the user's lyrics had been wiped.
pub fn load_doc(root: &Path, slug: &str, id: &LyricDocId) -> Result<LyricDoc, LibraryError> {
    let path = doc_path(root, slug, id)?;
    let text = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LibraryError::NotFound {
                kind: "lyric document",
                id: id.0.clone(),
            }
        } else {
            LibraryError::Io(e)
        }
    })?;
    Ok(serde_json::from_str(&text)?)
}

/// Reads every document `project` lists. **Never fails.**
///
/// Driven by `Project::lyrics` rather than by the directory, so the order is the
/// user's and a stray file left by a failed write is ignored rather than
/// appearing as a document. An id with nothing behind it becomes a warning.
pub fn list_docs(root: &Path, project: &Project) -> LyricDocSet {
    let mut docs = Vec::with_capacity(project.lyrics.len());
    let mut warnings = Vec::new();

    for id in &project.lyrics {
        let path = match doc_path(root, &project.slug, id) {
            Ok(path) => path,
            Err(e) => {
                warnings.push(LyricWarning::Unreadable {
                    id: id.0.clone(),
                    detail: e.to_string(),
                });
                continue;
            }
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warnings.push(LyricWarning::Missing { id: id.0.clone() });
                continue;
            }
            Err(e) => {
                warnings.push(LyricWarning::Unreadable {
                    id: id.0.clone(),
                    detail: e.to_string(),
                });
                continue;
            }
        };
        match serde_json::from_str::<LyricDoc>(&text) {
            Ok(doc) => docs.push(doc),
            Err(e) => warnings.push(LyricWarning::Malformed {
                id: id.0.clone(),
                detail: e.to_string(),
            }),
        }
    }

    LyricDocSet { docs, warnings }
}

/// Ids of tracks in `project` whose provenance references `doc_id`, in project
/// order. `version` narrows it to one version; `None` matches any version of the
/// document.
///
/// The single definition of "what points at these lyrics", shared by the version
/// delete and the document delete so the two refusals cannot drift. Malformed or
/// missing sidecars cannot be read to check and are surfaced as warnings
/// elsewhere (T-311e); a track that will not load is already a degraded record,
/// not a block applied here.
fn tracks_referencing(
    root: &Path,
    project: &Project,
    doc_id: &LyricDocId,
    version: Option<u32>,
) -> Vec<String> {
    crate::tracks::list_tracks(root, project)
        .tracks
        .into_iter()
        .filter(|t| match &t.provenance.spec.lyrics {
            Some(l) => &l.doc_id == doc_id && version.is_none_or(|v| l.version == v),
            None => false,
        })
        .map(|t| t.id.0)
        .collect()
}

/// Delete one version from a document, refusing when a track's provenance points
/// at it. Returns the updated document.
///
/// **The refusal is the feature** (PROJECT.md decisions log, 2026-09-01): a
/// track's sidecar records `(doc_id, version)`, so deleting a referenced version
/// would leave that track's recipe pointing at lyrics nobody could show. The
/// error names every track holding it -- the way T-403 renders a dangling id as
/// "Missing track" -- so "cannot delete this version" is never a dead end.
///
/// **Versions are never renumbered.** The chosen version is removed and every
/// other keeps its `number`, so a hole is legal -- exactly what
/// [`LyricDoc::push_version`] already assumes when it counts from the highest
/// present. Renumbering would silently repoint every surviving sidecar's
/// `LyricRef`, the very hazard the refusal exists to prevent. The one number
/// this can free for reuse is a *deleted top* version, and that is safe: the
/// refusal guarantees no sidecar referenced it, so a later `push_version`
/// minting it again cannot collide with any track's recipe. A per-document
/// version counter is deliberately not added -- there is nothing for it to
/// protect that the refusal does not.
///
/// **Deleting the approved version clears the approval** rather than being
/// refused: approval is the user's current working pointer for AudioStudio, not
/// provenance, and a document with none is an ordinary state (a fresh one has
/// none). The track-reference rule is the only bar to deletion.
///
/// There is no OS trash here -- a version is an element inside the document
/// file, not a file of its own -- so this rewrites the one document atomically
/// through [`save_doc`], the same write every other version edit uses.
pub fn delete_version(
    root: &Path,
    project: &Project,
    doc_id: &LyricDocId,
    version: u32,
) -> Result<LyricDoc, LibraryError> {
    let mut doc = load_doc(root, &project.slug, doc_id)?;
    if !doc.versions.iter().any(|v| v.number == version) {
        return Err(LibraryError::NotFound {
            kind: "lyric version",
            id: format!("{}#{version}", doc_id.0),
        });
    }

    // Refuse before touching the document: a version any track references stays.
    let referencing = tracks_referencing(root, project, doc_id, Some(version));
    if !referencing.is_empty() {
        return Err(LibraryError::VersionReferenced {
            doc_id: doc_id.0.clone(),
            version,
            tracks: referencing,
        });
    }

    doc.versions.retain(|v| v.number != version);
    if doc.approved == Some(version) {
        doc.approved = None;
    }
    save_doc(root, &project.slug, &doc)?;
    Ok(doc)
}

/// Delete a whole lyric document -- file to OS trash, id unlisted -- refusing
/// when any track's provenance references any of its versions. Returns the
/// project's remaining documents.
///
/// **Same refusal as [`delete_version`], applied to the whole file:** a document
/// is deletable only when nothing points at it, and the error names the tracks
/// holding it so the refusal is never a dead end. `trash` is the injected trash
/// operation (production passes [`trash_to_os`], tests a fake), the shape T-405
/// established for the one destructive action a test must not really perform.
///
/// **Order: file first, record last, a missing file tolerated** -- the
/// [`delete_track`](crate::tracks::delete_track) discipline. A crash after
/// trashing but before the save leaves the project listing a document whose file
/// is gone -- the "Missing" state [`list_docs`] already renders -- and a retry
/// completes cleanly because the trash step skips a file already gone. The
/// reverse order would strand a file nothing references with no id left to retry.
/// `next_lyric_seq` is untouched, so the freed id is never reissued and a track's
/// `LyricRef` can never come to mean a different document.
pub fn delete_doc<F>(
    root: &Path,
    slug: &str,
    doc_id: &LyricDocId,
    trash: F,
) -> Result<LyricDocSet, LibraryError>
where
    F: Fn(&Path) -> Result<(), LibraryError>,
{
    let mut project = crate::projects::load_project(root, slug)?;
    if !project.lyrics.contains(doc_id) {
        return Err(LibraryError::NotFound {
            kind: "lyric document",
            id: doc_id.0.clone(),
        });
    }

    let referencing = tracks_referencing(root, &project, doc_id, None);
    if !referencing.is_empty() {
        return Err(LibraryError::DocumentReferenced {
            doc_id: doc_id.0.clone(),
            tracks: referencing,
        });
    }

    let path = doc_path(root, slug, doc_id)?;
    if path.exists() {
        trash(&path)?;
    }
    project.lyrics.retain(|id| id != doc_id);
    crate::projects::save_project(root, &project)?;
    Ok(list_docs(root, &project))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use create_core::generation::{GenerationSpec, LyricRef};
    use create_core::project::{LyricSource, TrackId};
    use create_core::provenance::{Provenance, Track};

    use super::*;
    use crate::projects::create_project;

    const NOW: &str = "2026-08-25T20:11:04Z";

    fn project(root: &Path) -> Project {
        create_project(root, "Night Drive", NOW).unwrap()
    }

    /// A document with `n` versions (numbers 1..=n), written to disk.
    fn doc_with_versions(root: &Path, proj: &mut Project, n: u32) -> LyricDoc {
        let mut doc = create_doc(root, proj, Some("Song".to_string())).unwrap();
        for i in 1..=n {
            doc.push_version(format!("draft {i}"), LyricSource::Human, NOW);
        }
        save_doc(root, &proj.slug, &doc).unwrap();
        doc
    }

    /// Register a track whose provenance points at `(doc_id, version)`, both on
    /// disk and on the project, so [`delete_version`]'s scan can see it.
    fn track_referencing(
        root: &Path,
        proj: &mut Project,
        doc_id: &LyricDocId,
        version: u32,
    ) -> TrackId {
        let id = crate::tracks::mint_track_id(proj);
        let spec = GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs: BTreeMap::new(),
            loras: Vec::new(),
            lyrics: Some(LyricRef {
                doc_id: doc_id.clone(),
                version,
            }),
        };
        let track = Track {
            id: id.clone(),
            title: None,
            file: format!("tracks/{}.flac", id.0),
            duration_s: None,
            provenance: Provenance {
                profile_id: "ace-step-1.5-turbo".to_string(),
                profile_display_name: "ACE-Step".to_string(),
                model_license: "Apache-2.0".to_string(),
                template: None,
                spec,
                resolved_slots: BTreeMap::new(),
                comfy: None,
                created_at: NOW.to_string(),
                prompt_id: None,
            },
        };
        crate::tracks::save_track(root, &proj.slug, &track).unwrap();
        proj.tracks.push(id.clone());
        crate::projects::save_project(root, proj).unwrap();
        id
    }

    /// Invariant: the chosen version goes and every other keeps its number -- a
    /// hole is legal and nothing is renumbered, or a surviving sidecar's
    /// `LyricRef` would silently point at different lyrics.
    #[test]
    fn test_delete_version_removes_it_and_keeps_the_others() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 3);

        let updated = delete_version(root.path(), &proj, &doc.id, 2).unwrap();
        let numbers: Vec<u32> = updated.versions.iter().map(|v| v.number).collect();
        assert_eq!(
            numbers,
            vec![1, 3],
            "v2 removed, 1 and 3 keep their numbers"
        );

        // Persisted, not just returned.
        let reloaded = load_doc(root.path(), &proj.slug, &doc.id).unwrap();
        assert_eq!(
            reloaded
                .versions
                .iter()
                .map(|v| v.number)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    /// Invariant: a version a track's provenance points at is refused, and the
    /// error names the track holding it -- the refusal is the feature, and a
    /// refusal with no subject is a dead end.
    #[test]
    fn test_delete_referenced_version_is_refused_and_names_the_track() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 2);
        let track = track_referencing(root.path(), &mut proj, &doc.id, 1);

        let err = delete_version(root.path(), &proj, &doc.id, 1).unwrap_err();
        match err {
            LibraryError::VersionReferenced {
                version, tracks, ..
            } => {
                assert_eq!(version, 1);
                assert_eq!(tracks, vec![track.0]);
            }
            other => panic!("expected VersionReferenced, got {other:?}"),
        }

        // The version is untouched on disk -- a refused delete changes nothing.
        let reloaded = load_doc(root.path(), &proj.slug, &doc.id).unwrap();
        assert!(reloaded.versions.iter().any(|v| v.number == 1));
    }

    /// Invariant: a track referencing a *different* version does not block the
    /// delete -- the scan matches on the exact `(doc_id, version)`, not just the
    /// document.
    #[test]
    fn test_a_track_referencing_another_version_does_not_block() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 2);
        track_referencing(root.path(), &mut proj, &doc.id, 1);

        // v2 is unreferenced even though the document has a referenced version.
        let updated = delete_version(root.path(), &proj, &doc.id, 2).unwrap();
        assert_eq!(
            updated
                .versions
                .iter()
                .map(|v| v.number)
                .collect::<Vec<_>>(),
            vec![1]
        );
    }

    /// Invariant: the reference match is on the exact document, not just the
    /// version number. Two documents both have a v1; a track referencing one
    /// document's v1 must not block deleting the *other* document's v1.
    #[test]
    fn test_a_reference_to_another_document_does_not_block() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc_a = doc_with_versions(root.path(), &mut proj, 1);
        let doc_b = doc_with_versions(root.path(), &mut proj, 1);
        // A track references doc B's v1; doc A's v1 shares the number, not the doc.
        track_referencing(root.path(), &mut proj, &doc_b.id, 1);

        let updated = delete_version(root.path(), &proj, &doc_a.id, 1).unwrap();
        assert!(updated.versions.is_empty(), "doc A's v1 was deletable");
        // And doc B's referenced v1 is still refused, proving the match works both ways.
        let err = delete_version(root.path(), &proj, &doc_b.id, 1).unwrap_err();
        assert!(matches!(err, LibraryError::VersionReferenced { .. }));
    }

    /// Invariant: deleting the approved version clears the approval rather than
    /// leaving `approved` pointing at a number with no version behind it.
    #[test]
    fn test_delete_approved_version_clears_the_approval() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let mut doc = doc_with_versions(root.path(), &mut proj, 2);
        assert!(doc.approve(2));
        save_doc(root.path(), &proj.slug, &doc).unwrap();

        let updated = delete_version(root.path(), &proj, &doc.id, 2).unwrap();
        assert_eq!(updated.approved, None);

        let reloaded = load_doc(root.path(), &proj.slug, &doc.id).unwrap();
        assert_eq!(reloaded.approved, None);
        assert_eq!(
            reloaded
                .versions
                .iter()
                .map(|v| v.number)
                .collect::<Vec<_>>(),
            vec![1]
        );
    }

    /// Invariant: deleting a version that keeps the approval of another one
    /// leaves that approval alone.
    #[test]
    fn test_deleting_an_unapproved_version_keeps_the_approval() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let mut doc = doc_with_versions(root.path(), &mut proj, 3);
        assert!(doc.approve(3));
        save_doc(root.path(), &proj.slug, &doc).unwrap();

        let updated = delete_version(root.path(), &proj, &doc.id, 1).unwrap();
        assert_eq!(updated.approved, Some(3));
    }

    /// Documents the reuse-is-safe property: deleting the top version frees its
    /// number, and a later `push_version` mints it again. Safe precisely because
    /// the refusal guaranteed no sidecar referenced the deleted version, so the
    /// reissued number cannot collide with any track's recipe.
    #[test]
    fn test_deleting_the_top_version_lets_push_reuse_its_number_safely() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 2);

        let mut updated = delete_version(root.path(), &proj, &doc.id, 2).unwrap();
        assert_eq!(
            updated
                .versions
                .iter()
                .map(|v| v.number)
                .collect::<Vec<_>>(),
            vec![1]
        );

        let minted = updated.push_version("new draft", LyricSource::Human, NOW);
        assert_eq!(minted, 2, "the freed top number is reissued");
    }

    /// Invariant: a version number the document does not have is Not Found, not
    /// a silent no-op -- the caller named something that is not there.
    #[test]
    fn test_delete_missing_version_is_not_found() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 2);

        let err = delete_version(root.path(), &proj, &doc.id, 99).unwrap_err();
        assert!(matches!(
            err,
            LibraryError::NotFound {
                kind: "lyric version",
                ..
            }
        ));
    }

    /// A fake trasher that records what it was asked to trash and moves it out of
    /// the way, so a later `exists()` check sees it gone -- without touching the
    /// real Recycle Bin. `RefCell` because the closure is `Fn`, not `FnMut`.
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

    /// Invariant: delete_doc trashes the file via the injected trasher (never a
    /// hard delete -- CONVENTIONS), unlists the id, and returns the remainder.
    #[test]
    fn test_delete_doc_trashes_the_file_and_unlists_the_id() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 1);
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        let remaining = delete_doc(
            root.path(),
            &proj.slug,
            &doc.id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        let trashed = seen.into_inner();
        assert_eq!(trashed.len(), 1, "the document file is trashed");
        assert!(trashed[0].ends_with("ld-0001.json"));
        // It really left the lyrics dir -- via the trasher, not a hard delete.
        assert!(graveyard.join("ld-0001.json").exists());
        assert!(remaining.docs.is_empty());

        let reloaded = crate::projects::load_project(root.path(), &proj.slug).unwrap();
        assert!(!reloaded.lyrics.contains(&doc.id));
    }

    /// Invariant: a document with any referenced version is refused, naming the
    /// track -- the whole-file counterpart of the version refusal.
    #[test]
    fn test_delete_referenced_doc_is_refused_and_names_the_track() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 2);
        let track = track_referencing(root.path(), &mut proj, &doc.id, 1);
        let seen = std::cell::RefCell::new(Vec::new());

        let err = delete_doc(
            root.path(),
            &proj.slug,
            &doc.id,
            recording_trasher(&seen, &root.path().join("nowhere")),
        )
        .unwrap_err();

        match err {
            LibraryError::DocumentReferenced { tracks, .. } => assert_eq!(tracks, vec![track.0]),
            other => panic!("expected DocumentReferenced, got {other:?}"),
        }
        // A refused delete trashes nothing and leaves the id listed.
        assert!(seen.into_inner().is_empty());
        let reloaded = crate::projects::load_project(root.path(), &proj.slug).unwrap();
        assert!(reloaded.lyrics.contains(&doc.id));
    }

    /// Invariant: a track referencing a *later* version of the document still
    /// blocks the whole-document delete -- `None` matches any version.
    #[test]
    fn test_delete_doc_is_blocked_by_a_reference_to_any_version() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 3);
        track_referencing(root.path(), &mut proj, &doc.id, 3);
        let seen = std::cell::RefCell::new(Vec::new());

        let err = delete_doc(
            root.path(),
            &proj.slug,
            &doc.id,
            recording_trasher(&seen, &root.path().join("nowhere")),
        )
        .unwrap_err();
        assert!(matches!(err, LibraryError::DocumentReferenced { .. }));
    }

    /// Invariant: deleting a document does not free its id for reuse --
    /// `next_lyric_seq` is untouched, so a later document's `LyricRef` can never
    /// come to mean a deleted one.
    #[test]
    fn test_delete_doc_does_not_free_the_id_for_reuse() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = create_doc(root.path(), &mut proj, None).unwrap();
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        delete_doc(
            root.path(),
            &proj.slug,
            &first.id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        // Reload so the minted-id counter reflects the delete, then create again.
        let mut reloaded = crate::projects::load_project(root.path(), &proj.slug).unwrap();
        let next = create_doc(root.path(), &mut reloaded, None).unwrap();
        assert_eq!(next.id, LyricDocId("ld-0002".to_string()));
        assert_ne!(next.id, first.id);
    }

    /// Invariant: a document listed with its file already gone is still unlisted
    /// -- the trasher is skipped for the missing file, not run against it (it
    /// would error), so a half-done prior delete self-heals on retry.
    #[test]
    fn test_delete_doc_tolerates_a_missing_file() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = doc_with_versions(root.path(), &mut proj, 1);
        fs::remove_file(doc_path(root.path(), &proj.slug, &doc.id).unwrap()).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        let remaining = delete_doc(
            root.path(),
            &proj.slug,
            &doc.id,
            recording_trasher(&seen, &root.path().join("nowhere")),
        )
        .unwrap();

        assert!(seen.into_inner().is_empty(), "nothing to trash");
        assert!(remaining.docs.is_empty());
        let reloaded = crate::projects::load_project(root.path(), &proj.slug).unwrap();
        assert!(!reloaded.lyrics.contains(&doc.id));
    }

    /// Invariant: a track referencing a *different* document does not block the
    /// delete -- the scan matches on the document, so one doc's tracks never pin
    /// another doc.
    #[test]
    fn test_delete_doc_ignores_a_reference_to_another_document() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc_a = doc_with_versions(root.path(), &mut proj, 1);
        let doc_b = doc_with_versions(root.path(), &mut proj, 1);
        track_referencing(root.path(), &mut proj, &doc_b.id, 1);
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        // doc A has no tracks; doc B's reference must not block it.
        let remaining = delete_doc(
            root.path(),
            &proj.slug,
            &doc_a.id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();
        assert_eq!(remaining.docs.len(), 1);
        assert_eq!(remaining.docs[0].id, doc_b.id);
    }

    /// Invariant: deleting one document leaves the others listed and on disk.
    #[test]
    fn test_delete_doc_leaves_the_other_documents() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let gone = doc_with_versions(root.path(), &mut proj, 1);
        let kept = doc_with_versions(root.path(), &mut proj, 1);
        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        let remaining = delete_doc(
            root.path(),
            &proj.slug,
            &gone.id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        assert_eq!(remaining.docs.len(), 1);
        assert_eq!(remaining.docs[0].id, kept.id);
        assert!(doc_path(root.path(), &proj.slug, &kept.id)
            .unwrap()
            .is_file());
    }

    /// Invariant: a document id the project does not list is Not Found, not a
    /// silent no-op.
    #[test]
    fn test_delete_missing_doc_is_not_found() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let seen = std::cell::RefCell::new(Vec::new());

        let err = delete_doc(
            root.path(),
            &proj.slug,
            &LyricDocId("ld-0042".to_string()),
            recording_trasher(&seen, &root.path().join("nowhere")),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LibraryError::NotFound {
                kind: "lyric document",
                ..
            }
        ));
    }

    /// Invariant: the new document is both on disk and listed by the project.
    /// Either half alone is a document the app cannot find again.
    #[test]
    fn test_create_doc_registers_the_id_and_writes_the_file() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let doc = create_doc(root.path(), &mut proj, Some("Opening".to_string())).unwrap();

        assert_eq!(doc.id, LyricDocId("ld-0001".to_string()));
        assert_eq!(proj.lyrics, vec![doc.id.clone()]);
        assert_eq!(proj.next_lyric_seq, 2);
        assert!(doc_path(root.path(), &proj.slug, &doc.id)
            .unwrap()
            .is_file());

        let reloaded = crate::projects::load_project(root.path(), &proj.slug).unwrap();
        assert_eq!(reloaded.lyrics, vec![doc.id]);
        assert_eq!(reloaded.next_lyric_seq, 2);
    }

    /// Invariant: ids come from the counter, never from the files present. A
    /// deleted document's id must not be handed to a later one, or a track's
    /// provenance would resolve to lyrics written for a different song.
    #[test]
    fn test_doc_ids_are_not_reused_after_a_delete() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = create_doc(root.path(), &mut proj, None).unwrap();
        create_doc(root.path(), &mut proj, None).unwrap();

        // Delete the first document the way the app will: file gone, id unlisted.
        fs::remove_file(doc_path(root.path(), &proj.slug, &first.id).unwrap()).unwrap();
        proj.lyrics.retain(|id| id != &first.id);
        crate::projects::save_project(root.path(), &proj).unwrap();

        let third = create_doc(root.path(), &mut proj, None).unwrap();
        assert_eq!(third.id, LyricDocId("ld-0003".to_string()));
    }

    /// Invariant: ids are zero-padded so a name-sorted directory listing is in
    /// creation order. `ld-10` before `ld-2` would misorder the studio's list.
    #[test]
    fn test_minted_ids_sort_in_creation_order() {
        let mut proj = Project::new("p", "P", NOW);
        proj.next_lyric_seq = 9;
        let ninth = mint_doc_id(&mut proj);
        let tenth = mint_doc_id(&mut proj);
        assert_eq!(ninth.0, "ld-0009");
        assert_eq!(tenth.0, "ld-0010");
        assert!(ninth.0 < tenth.0);
    }

    /// Invariant: every version, its source and the approval survive a write and
    /// a read. This is the record a track's provenance points into.
    #[test]
    fn test_save_doc_round_trips_versions_and_approval() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let mut doc = create_doc(root.path(), &mut proj, Some("Title".to_string())).unwrap();

        doc.push_version("first draft", LyricSource::Human, NOW);
        doc.push_version(
            "[Verse]\nneon on the dashboard",
            LyricSource::Llm {
                model: "gemma4:12b-32k".to_string(),
                prompt_optimized: true,
            },
            "2026-08-25T20:12:00Z",
        );
        assert!(doc.approve(2));
        save_doc(root.path(), &proj.slug, &doc).unwrap();

        let loaded = load_doc(root.path(), &proj.slug, &doc.id).unwrap();
        assert_eq!(loaded, doc);
        assert_eq!(loaded.approved, Some(2));
        assert_eq!(
            loaded.approved_version().map(|v| v.text.as_str()),
            Some("[Verse]\nneon on the dashboard")
        );
        match &loaded.versions[1].source {
            LyricSource::Llm {
                model,
                prompt_optimized,
            } => {
                assert_eq!(model, "gemma4:12b-32k");
                assert!(prompt_optimized, "the consent flag must survive the write");
            }
            other => panic!("expected Llm source, got {other:?}"),
        }
    }

    /// Invariant: a named document that is missing is reported as missing, not
    /// returned empty -- an empty document would read as lost lyrics.
    #[test]
    fn test_load_missing_doc_is_not_found() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let err =
            load_doc(root.path(), &proj.slug, &LyricDocId("ld-0042".to_string())).unwrap_err();
        assert!(matches!(
            err,
            LibraryError::NotFound {
                kind: "lyric document",
                ..
            }
        ));
    }

    /// Invariant: an id from the frontend cannot address anything outside the
    /// project's lyrics directory.
    #[test]
    fn test_unsafe_doc_ids_are_refused() {
        let root = tempfile::tempdir().unwrap();
        for id in ["../project", "ld-1/../../x", "ld-", "ld-1a", "project", ""] {
            let err =
                doc_path(root.path(), "night-drive", &LyricDocId(id.to_string())).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "id {id:?} should be refused"
            );
        }
    }

    /// Invariant: an id the project lists with no file behind it is surfaced.
    /// Dropping it silently would tell the user they never wrote the document.
    #[test]
    fn test_list_docs_reports_an_id_with_no_file() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let kept = create_doc(root.path(), &mut proj, None).unwrap();
        let lost = create_doc(root.path(), &mut proj, None).unwrap();
        fs::remove_file(doc_path(root.path(), &proj.slug, &lost.id).unwrap()).unwrap();

        let set = list_docs(root.path(), &proj);
        assert_eq!(set.docs.len(), 1);
        assert_eq!(set.docs[0].id, kept.id);
        assert_eq!(
            set.warnings,
            vec![LyricWarning::Missing {
                id: lost.id.0.clone()
            }]
        );
    }

    /// Invariant: the listing follows the project's own order, and a stray file
    /// nothing references is not mistaken for a document.
    #[test]
    fn test_list_docs_follows_project_order_and_ignores_strays() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = create_doc(root.path(), &mut proj, Some("One".to_string())).unwrap();
        let second = create_doc(root.path(), &mut proj, Some("Two".to_string())).unwrap();
        proj.lyrics = vec![second.id.clone(), first.id.clone()];

        let orphan = LyricDoc {
            id: LyricDocId("ld-0099".to_string()),
            title: Some("Never registered".to_string()),
            versions: Vec::new(),
            approved: None,
        };
        save_doc(root.path(), &proj.slug, &orphan).unwrap();

        let set = list_docs(root.path(), &proj);
        let ids: Vec<&str> = set.docs.iter().map(|d| d.id.0.as_str()).collect();
        assert_eq!(ids, vec!["ld-0002", "ld-0001"]);
        assert!(set.warnings.is_empty());
    }

    /// Invariant: one malformed document does not hide the others, and it is
    /// named so the user can find it.
    #[test]
    fn test_list_docs_reports_a_malformed_document_and_keeps_the_rest() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let good = create_doc(root.path(), &mut proj, None).unwrap();
        let bad = create_doc(root.path(), &mut proj, None).unwrap();
        fs::write(
            doc_path(root.path(), &proj.slug, &bad.id).unwrap(),
            "{ not json",
        )
        .unwrap();

        let set = list_docs(root.path(), &proj);
        assert_eq!(set.docs.len(), 1);
        assert_eq!(set.docs[0].id, good.id);
        assert!(matches!(
            set.warnings.as_slice(),
            [LyricWarning::Malformed { id, .. }] if id == &bad.id.0
        ));
    }
}
