//! Lyric documents on disk: `library/projects/<slug>/lyrics/<doc-id>.json`.
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

#[cfg(test)]
mod tests {
    use create_core::project::LyricSource;

    use super::*;
    use crate::projects::create_project;

    const NOW: &str = "2026-08-25T20:11:04Z";

    fn project(root: &Path) -> Project {
        create_project(root, "Night Drive", NOW).unwrap()
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
