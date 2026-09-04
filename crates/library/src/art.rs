//! Artwork sidecars and image files on disk: `<app config dir>/projects/<slug>/art/`.
//!
//! One sidecar per artwork (`<id>.json`) holds the full provenance; the image
//! file (`<id>.<ext>`) lives beside it. `project.json` holds only the id, so
//! this module is the only thing that reads or writes artwork records.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use create_core::image::png_dimensions;
use create_core::project::{ArtId, Project};
use create_core::provenance::Artwork;
use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::projects::project_dir;
use crate::LibraryError;

/// Directory inside a project holding its images and sidecars.
pub const ART_DIR: &str = "art";

/// Prefix every minted artwork id carries.
const ID_PREFIX: &str = "ar-";

/// Digits a minted id is padded to, so name order matches creation order.
const ID_DIGITS: usize = 4;

/// Something about listing a project's artwork the user should be told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtWarning {
    /// `project.json` lists an id with no sidecar behind it. Surfaced rather than
    /// dropped: an artwork the user generated is missing, and silence would read as
    /// "there was never one".
    Missing { id: String },
    /// The sidecar exists but could not be read.
    Unreadable { id: String, detail: String },
    /// The sidecar is not a valid artwork record.
    Malformed { id: String, detail: String },
}

/// Every artwork a project could offer, plus anything worth reporting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtSet {
    /// In `Project::art` order -- the order they were generated.
    pub art: Vec<Artwork>,
    #[serde(default)]
    pub warnings: Vec<ArtWarning>,
}

/// Whether an id may be joined onto a project directory.
///
/// A whitelist, for the same reason project slugs use one: ids reach this crate
/// from the frontend, and `..` or a separator would read and write outside the
/// library.
fn is_safe_art_id(id: &str) -> bool {
    id.len() > ID_PREFIX.len()
        && id.len() <= ID_PREFIX.len() + 12
        && id.starts_with(ID_PREFIX)
        && id[ID_PREFIX.len()..].chars().all(|c| c.is_ascii_digit())
}

/// The art directory for one project.
pub fn art_dir(root: &Path, slug: &str) -> Result<PathBuf, LibraryError> {
    Ok(project_dir(root, slug)?.join(ART_DIR))
}

/// The sidecar file for one artwork, refusing any id that could escape the project.
pub fn sidecar_path(root: &Path, slug: &str, id: &ArtId) -> Result<PathBuf, LibraryError> {
    if !is_safe_art_id(&id.0) {
        return Err(LibraryError::UnusableName(id.0.clone()));
    }
    Ok(art_dir(root, slug)?.join(format!("{}.json", id.0)))
}

/// The image file for one artwork, refusing any id or extension that could escape
/// the project or carry a separator.
///
/// `ext` is taken from the produced file, lowercased, and must be entirely
/// ASCII alphanumeric. The extension on `Artwork::file` is this app's record of
/// the real output format, which is why no separate format field is being added.
pub fn image_path(root: &Path, slug: &str, id: &ArtId, ext: &str) -> Result<PathBuf, LibraryError> {
    if !is_safe_art_id(&id.0) {
        return Err(LibraryError::UnusableName(id.0.clone()));
    }
    let ext_lower = ext.to_ascii_lowercase();
    if ext_lower.is_empty() || !ext_lower.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(LibraryError::UnusableName(ext.to_string()));
    }
    Ok(art_dir(root, slug)?.join(format!("{}.{}", id.0, ext_lower)))
}

/// Resolves an artwork's stored `file` -- relative to the project directory, e.g.
/// `"art/ar-0001.png"` -- to an absolute path the webview can display.
///
/// `Artwork.file` is written by this app, but it lives in a JSON sidecar the user
/// can open and edit: a hand-edited sidecar could name any file. An absolute
/// path, or one whose `..` walks out of the project, is refused rather than
/// handed to the webview as a path to serve. The asset protocol's own scope is
/// the second gate; this is the first.
pub fn resolve_art_file(root: &Path, slug: &str, file: &str) -> Result<PathBuf, LibraryError> {
    let rel = Path::new(file);
    let escapes = rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
    if escapes {
        return Err(LibraryError::UnusableName(file.to_string()));
    }
    Ok(project_dir(root, slug)?.join(rel))
}

/// Takes the next artwork id for `project` and advances its counter.
///
/// The counter is the only source of ids. Deriving one from the files present
/// would hand a deleted artwork's id to a later one, and a cover reference still
/// holding the old `ArtId` would then point at unrelated image.
pub fn mint_art_id(project: &mut Project) -> ArtId {
    let id = ArtId(format!(
        "{ID_PREFIX}{:0width$}",
        project.next_art_seq,
        width = ID_DIGITS
    ));
    project.next_art_seq = project.next_art_seq.saturating_add(1);
    id
}

/// Writes one artwork sidecar.
///
/// The sidecar is written before `project.json` gains the id. If the project
/// write then fails, the result is a sidecar nothing references -- invisible,
/// recoverable, harmless -- rather than a project listing an artwork whose file
/// is not there, which is the state that makes the Library view lie.
pub fn save_art(root: &Path, slug: &str, artwork: &Artwork) -> Result<(), LibraryError> {
    let path = sidecar_path(root, slug, &artwork.id)?;
    atomic::write_json(&path, artwork)
}

/// Loads one artwork sidecar by id.
///
/// Fails when it is absent: the caller named this artwork, and an empty one
/// would look like the user's provenance had been wiped.
pub fn load_art(root: &Path, slug: &str, id: &ArtId) -> Result<Artwork, LibraryError> {
    let path = sidecar_path(root, slug, id)?;
    let text = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LibraryError::NotFound {
                kind: "artwork sidecar",
                id: id.0.clone(),
            }
        } else {
            LibraryError::Io(e)
        }
    })?;
    Ok(serde_json::from_str(&text)?)
}

/// Reads every artwork `project` lists. **Never fails.**
///
/// Driven by `Project::art` rather than by the directory, so the order is the
/// user's and a stray file left by a failed write is ignored rather than
/// appearing as an artwork. An id with nothing behind it becomes a warning.
pub fn list_art(root: &Path, project: &Project) -> ArtSet {
    let mut art = Vec::with_capacity(project.art.len());
    let mut warnings = Vec::new();

    for id in &project.art {
        let path = match sidecar_path(root, &project.slug, id) {
            Ok(path) => path,
            Err(e) => {
                warnings.push(ArtWarning::Unreadable {
                    id: id.0.clone(),
                    detail: e.to_string(),
                });
                continue;
            }
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warnings.push(ArtWarning::Missing { id: id.0.clone() });
                continue;
            }
            Err(e) => {
                warnings.push(ArtWarning::Unreadable {
                    id: id.0.clone(),
                    detail: e.to_string(),
                });
                continue;
            }
        };
        match serde_json::from_str::<Artwork>(&text) {
            Ok(artwork) => art.push(artwork),
            Err(e) => warnings.push(ArtWarning::Malformed {
                id: id.0.clone(),
                detail: e.to_string(),
            }),
        }
    }

    ArtSet { art, warnings }
}

/// Pixel size, read from the image file's header.
///
/// Returns `None` when the file is missing, not a PNG, or its header does not
/// carry a known size. A missing size must never stop an artwork from being
/// recorded.
pub fn dimensions_of(path: &Path) -> Option<(u32, u32)> {
    let mut file = fs::File::open(path).ok()?;
    let mut head = [0u8; 24];
    // `read_exact`, not `read`: a single `read` may legally return fewer bytes
    // than asked for, and a short read on a perfectly good file would then be
    // indistinguishable from a truncated one and report no size. A file shorter
    // than 24 bytes cannot be a PNG anyway -- magic, length, chunk name and
    // dimensions come to 24 bytes before any image data -- so the `UnexpectedEof`
    // this returns for one is the right answer.
    file.read_exact(&mut head).ok()?;
    png_dimensions(&head)
}

/// Delete one artwork -- image and sidecar to OS trash, id unlisted, and any
/// cover references cleared.
///
/// **A cover reference does not block the delete; it is cleared.** This is the
/// opposite of `lyrics::delete_doc`: a `LyricRef` is part of the recipe and must
/// stay reproducible, but a cover is an editable pointer like `title`. Nothing
/// about reproducing a track depends on it, so refusing would force the user to
/// detach a cover from every track and album before deleting it -- friction
/// bought with no protection.
///
/// **Order: files first, record last, missing files tolerated.** A crash after
/// trashing but before the save leaves the project listing an artwork whose
/// files are gone -- the "Missing" state `list_art` already renders -- and a
/// retry completes cleanly because the trash step skips what is already missing.
/// The reverse order would strand files nothing references with no id left to
/// retry.
///
/// **Cover clearing is N separate atomic writes, not one transaction.** A crash
/// part-way leaves some tracks with no cover and some naming a deleted one; both
/// are states the view has to render anyway, which is why this is tolerable
/// rather than hidden.
///
/// `next_art_seq` is untouched, so the freed id is never reissued and a
/// surviving cover reference can never come to mean a different image.
pub fn delete_art<F>(root: &Path, slug: &str, id: &ArtId, trash: F) -> Result<(), LibraryError>
where
    F: Fn(&Path) -> Result<(), LibraryError>,
{
    let mut project = crate::projects::load_project(root, slug)?;
    if !project.art.contains(id) {
        return Err(LibraryError::NotFound {
            kind: "artwork",
            id: id.0.clone(),
        });
    }

    // Load the sidecar to learn the image filename, and trash the image. A
    // sidecar that will not load is tolerated: the record is still cleaned, and
    // at worst one orphan image is left for a degraded artwork the user is
    // deleting anyway.
    if let Ok(art) = load_art(root, slug, id) {
        let image = resolve_art_file(root, slug, &art.file)?;
        crate::tracks::trash_if_present(&image, &trash)?;
    }
    crate::tracks::trash_if_present(&sidecar_path(root, slug, id)?, &trash)?;

    // Clear the cover from every track sidecar naming it.
    for track_id in &project.tracks {
        if let Ok(mut track) = crate::tracks::load_track(root, slug, track_id) {
            if track.cover.as_ref() == Some(id) {
                track.cover = None;
                crate::tracks::save_track(root, slug, &track)?;
            }
        }
    }

    // Clear the cover from every album naming it.
    for album in &mut project.albums {
        if album.cover.as_ref() == Some(id) {
            album.cover = None;
        }
    }

    project.art.retain(|a| a != id);
    crate::projects::save_project(root, &project)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{load_project, save_project};
    use create_core::generation::{GenerationSpec, InputValue};
    use create_core::profile::SlotAddress;
    use create_core::project::TrackId;
    use create_core::provenance::{ComfyServerInfo, Provenance};
    use std::collections::BTreeMap;

    const NOW: &str = "2026-08-25T20:11:04Z";

    fn project(root: &Path) -> Project {
        crate::projects::create_project(root, "Night Drive", NOW).unwrap()
    }

    fn sample_artwork(id: ArtId, file: String) -> Artwork {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "prompt".to_string(),
            InputValue::Text("synthwave album cover, neon city".to_string()),
        );
        inputs.insert("seed".to_string(), InputValue::Seed(7));

        let spec = GenerationSpec {
            title: Some("Klein Cover".to_string()),
            profile_id: "klein".to_string(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let mut resolved_slots = BTreeMap::new();
        resolved_slots.insert(
            SlotAddress("5.text".to_string()),
            InputValue::Text("synthwave album cover, neon city".to_string()),
        );
        resolved_slots.insert(SlotAddress("17.value".to_string()), InputValue::Seed(7));

        Artwork {
            id,
            title: Some("Neon City".to_string()),
            file,
            width: Some(768),
            height: Some(768),
            provenance: Provenance {
                profile_id: "klein".to_string(),
                profile_display_name: "Klein".to_string(),
                model_license: "Apache-2.0".to_string(),
                template: Some("image_klein".to_string()),
                spec,
                resolved_slots,
                comfy: Some(ComfyServerInfo {
                    comfyui_version: Some("0.3.26".to_string()),
                    comfy_cli_version: Some("0.1.0".to_string()),
                    url: Some("http://127.0.0.1:8188".to_string()),
                }),
                created_at: "2026-09-03T12:00:00Z".to_string(),
                prompt_id: Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string()),
            },
        }
    }

    /// A fake trasher that records what it was asked to trash and moves it out
    /// of the way, so a later `exists()` check sees it gone -- without touching
    /// the real Recycle Bin. `RefCell` because the closure is `Fn`, not `FnMut`.
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

    /// Create a track sidecar with an optional cover, registered on the project.
    fn track_with_cover(
        root: &Path,
        slug: &str,
        proj: &mut Project,
        id: &str,
        cover: Option<ArtId>,
    ) -> create_core::provenance::Track {
        use create_core::provenance::Track;

        let track_id = TrackId(id.to_string());
        let track = Track {
            id: track_id.clone(),
            title: None,
            cover,
            file: format!("tracks/{}.flac", id),
            duration_s: None,
            provenance: Provenance {
                profile_id: "ace-step-1.5-turbo".to_string(),
                profile_display_name: "ACE-Step".to_string(),
                model_license: "Apache-2.0".to_string(),
                template: None,
                spec: GenerationSpec {
                    title: None,
                    profile_id: "ace-step-1.5-turbo".to_string(),
                    inputs: BTreeMap::new(),
                    loras: vec![],
                    lyrics: None,
                },
                resolved_slots: BTreeMap::new(),
                comfy: None,
                created_at: NOW.to_string(),
                prompt_id: None,
            },
        };
        crate::tracks::save_track(root, slug, &track).unwrap();
        proj.tracks.push(track_id);
        track
    }

    /// Invariant: ids come from the counter, padded so sorting by name matches
    /// creation order.
    #[test]
    fn test_mint_art_id_is_padded_and_advances_the_counter() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = mint_art_id(&mut proj);
        let second = mint_art_id(&mut proj);
        assert_eq!(first.0, "ar-0001");
        assert_eq!(second.0, "ar-0002");
        assert_eq!(proj.next_art_seq, 3);
    }

    /// Invariant: ids come from the counter, never from the files present. A
    /// deleted artwork's id must not be handed to a later one, or a cover
    /// reference would point at image generated for a different artwork.
    #[test]
    fn test_mint_art_id_never_reuses_a_deleted_id() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = mint_art_id(&mut proj);
        assert_eq!(first.0, "ar-0001");

        // Simulate a delete: id removed from the project, sidecar ignored.
        proj.art.clear();
        save_project(root.path(), &proj).unwrap();
        let mut reloaded = load_project(root.path(), &proj.slug).unwrap();

        let second = mint_art_id(&mut reloaded);
        assert_ne!(second.0, first.0);
        assert_eq!(second.0, "ar-0002");
    }

    /// Invariant: a project file written before artwork existed still loads,
    /// with an empty art list and the counter starting at 1.
    #[test]
    fn test_project_without_art_fields_still_loads() {
        let root = tempfile::tempdir().unwrap();
        let dir = project_dir(root.path(), "legacy").unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.json"),
            r#"{"slug":"legacy","name":"Legacy","created_at":"2026-08-25T10:00:00Z"}"#,
        )
        .unwrap();

        let project = load_project(root.path(), "legacy").unwrap();
        assert!(project.art.is_empty());
        assert_eq!(project.next_art_seq, 1);
    }

    /// Invariant: a slug from the frontend cannot address anything outside the
    /// project's art directory.
    #[test]
    fn test_art_paths_refuse_a_slug_that_escapes_the_root() {
        let root = tempfile::tempdir().unwrap();
        let id = ArtId("ar-0001".to_string());
        for slug in ["../secrets", "a/b", "C:\\Windows", "Night Drive", ""] {
            let err = sidecar_path(root.path(), slug, &id).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "slug {slug:?} should be refused"
            );
        }
    }

    /// Invariant: an id from the frontend cannot address anything outside the
    /// art directory.
    #[test]
    fn test_art_paths_refuse_an_id_that_escapes_the_art_dir() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        for bad in [
            "../../config",
            "ar-0001/../../secrets",
            "ar-0001.json",
            "ar-",
            "",
            "tr-0001",
            "ar-abcd",
        ] {
            let id = ArtId(bad.to_string());
            assert!(
                matches!(
                    sidecar_path(root.path(), &proj.slug, &id),
                    Err(LibraryError::UnusableName(_))
                ),
                "sidecar_path should refuse id {bad:?}"
            );
            assert!(
                matches!(
                    image_path(root.path(), &proj.slug, &id, "png"),
                    Err(LibraryError::UnusableName(_))
                ),
                "image_path should refuse id {bad:?}"
            );
        }
    }

    /// Invariant: an extension that is not purely ASCII alphanumeric is refused.
    /// It reaches this function from a filename comfy-cli chose.
    #[test]
    fn test_image_path_refuses_an_extension_that_is_not_alphanumeric() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let id = ArtId("ar-0001".to_string());
        for ext in ["pn/g", "png!", ".png", ""] {
            let err = image_path(root.path(), &proj.slug, &id, ext).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "ext {ext:?} should be refused"
            );
        }
    }

    /// Invariant: an artwork's stored path resolves to an absolute file under the
    /// project. It must be absolute or `convertFileSrc` cannot turn it into an
    /// asset URL.
    #[test]
    fn test_resolve_art_file_returns_an_absolute_path() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        std::fs::create_dir_all(art_dir(root.path(), &proj.slug).unwrap()).unwrap();

        let resolved = resolve_art_file(root.path(), &proj.slug, "art/ar-0001.png").unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with(Path::new("art").join("ar-0001.png")));
    }

    /// Invariant: a sidecar edited to name an absolute path is refused, not
    /// served. `Path::join` replaces its base with an absolute right-hand side,
    /// so this must be caught before the join.
    #[test]
    fn test_resolve_art_file_refuses_an_absolute_path() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let abs = std::env::temp_dir().join("outside.png");

        let err = resolve_art_file(root.path(), &proj.slug, abs.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, LibraryError::UnusableName(_)));
    }

    /// Invariant: a `..` in a hand-edited sidecar cannot walk out of the
    /// project, wherever it appears in the path.
    #[test]
    fn test_resolve_art_file_refuses_a_parent_escape() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        std::fs::create_dir_all(art_dir(root.path(), &proj.slug).unwrap()).unwrap();

        let leading = Path::new("..").join("config.json");
        for file in ["art/../outside.png", leading.to_str().unwrap()] {
            let err = resolve_art_file(root.path(), &proj.slug, file).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "file {file:?} should be refused"
            );
        }
    }

    /// Invariant: an artwork sidecar round-trips, preserving the whole record
    /// including its provenance.
    #[test]
    fn test_save_art_then_load_art_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let id = ArtId("ar-0001".to_string());
        let artwork = sample_artwork(id, "art/ar-0001.png".to_string());

        save_art(root.path(), &proj.slug, &artwork).unwrap();
        let loaded = load_art(root.path(), &proj.slug, &artwork.id).unwrap();
        assert_eq!(loaded, artwork);
    }

    /// Invariant: the listing follows the project's own order, and a stray file
    /// nothing references is not mistaken for an artwork.
    #[test]
    fn test_list_art_returns_them_in_project_order() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = sample_artwork(ArtId("ar-0001".to_string()), "art/ar-0001.png".to_string());
        let second = sample_artwork(ArtId("ar-0002".to_string()), "art/ar-0002.png".to_string());
        let third = sample_artwork(ArtId("ar-0003".to_string()), "art/ar-0003.png".to_string());
        save_art(root.path(), &proj.slug, &first).unwrap();
        save_art(root.path(), &proj.slug, &second).unwrap();
        save_art(root.path(), &proj.slug, &third).unwrap();

        proj.art = vec![third.id.clone(), first.id.clone(), second.id.clone()];
        let set = list_art(root.path(), &proj);
        let ids: Vec<&str> = set.art.iter().map(|a| a.id.0.as_str()).collect();
        assert_eq!(ids, vec!["ar-0003", "ar-0001", "ar-0002"]);
        assert!(set.warnings.is_empty());
    }

    /// Invariant: one missing sidecar does not hide the others, and it is named
    /// so the user can find it.
    #[test]
    fn test_a_missing_sidecar_is_a_warning_not_a_silence() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let present = sample_artwork(ArtId("ar-0001".to_string()), "art/ar-0001.png".to_string());
        let missing = sample_artwork(ArtId("ar-0002".to_string()), "art/ar-0002.png".to_string());
        save_art(root.path(), &proj.slug, &present).unwrap();

        proj.art = vec![present.id.clone(), missing.id.clone()];
        let set = list_art(root.path(), &proj);
        assert_eq!(set.art.len(), 1);
        assert_eq!(set.art[0].id, present.id);
        assert_eq!(
            set.warnings,
            vec![ArtWarning::Missing {
                id: missing.id.0.clone()
            }]
        );
    }

    /// Invariant: a project with no artwork offers an empty set, not an error.
    #[test]
    fn test_list_art_is_empty_for_a_project_with_none() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let set = list_art(root.path(), &proj);
        assert!(set.art.is_empty());
        assert!(set.warnings.is_empty());
    }

    /// Invariant: the size comes off the real file, not the spec.
    ///
    /// Uses the committed 24-byte head of a real generated PNG. Its length is
    /// exactly the read size, which is the case `read_exact` must handle: the
    /// `read` this replaced could legally have returned fewer bytes and
    /// reported no size for a perfectly good file.
    #[test]
    fn test_dimensions_of_reads_a_real_png_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ar-0001.png");
        let head = include_bytes!("../../../testdata/images/klein-cover.png.head");
        fs::write(&path, head).unwrap();

        assert_eq!(dimensions_of(&path), Some((768, 768)));
    }

    /// Invariant: a missing or unreadable file is a missing size, never an
    /// error. An artwork whose image is fine must not fail to be recorded
    /// because its header could not be read.
    #[test]
    fn test_dimensions_of_is_none_for_a_file_that_is_not_there() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(dimensions_of(&dir.path().join("nope.png")), None);
    }

    /// Invariant: a file too short to hold a header is refused, not panicked on.
    #[test]
    fn test_dimensions_of_is_none_for_a_file_shorter_than_a_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stub.png");
        fs::write(&path, b"\x89PNG").unwrap();

        assert_eq!(dimensions_of(&path), None);
    }

    /// Invariant: **files first, record last.** A trash failure part-way leaves
    /// the project still listing the artwork, so a retry completes -- the
    /// `delete_track` discipline this mirrors. Writing the record first would
    /// strand files nothing references with no id left to retry them under, and
    /// this is the one half of that ordering a test can reach without
    /// simulating a crash: trashing really can fail (a locked file, a volume
    /// with no Recycle Bin).
    #[test]
    fn test_a_failed_trash_leaves_the_record_intact_for_a_retry() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let id = mint_art_id(&mut proj);
        register_artwork(root.path(), &mut proj, &id);
        fs::write(
            image_path(root.path(), &proj.slug, &id, "png").unwrap(),
            b"PNGDATA",
        )
        .unwrap();
        save_project(root.path(), &proj).unwrap();

        // Succeeds on the image, fails on the sidecar.
        let calls = std::cell::RefCell::new(0usize);
        let failing = |_: &Path| -> Result<(), LibraryError> {
            let mut n = calls.borrow_mut();
            *n += 1;
            if *n == 1 {
                Ok(())
            } else {
                Err(LibraryError::Trash("locked".to_string()))
            }
        };

        let err = delete_art(root.path(), &proj.slug, &id, failing).unwrap_err();
        assert!(matches!(err, LibraryError::Trash(_)));

        let reloaded = load_project(root.path(), &proj.slug).unwrap();
        assert!(
            reloaded.art.contains(&id),
            "the record must survive a failed trash so the delete can be retried"
        );
    }

    /// Invariant: deleting an artwork trashes both its files and unlists the id.
    #[test]
    fn test_delete_art_trashes_both_files_and_unlists_the_id() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let id = mint_art_id(&mut proj);
        let art = sample_artwork(id.clone(), format!("art/{}.png", id.0));
        save_art(root.path(), &proj.slug, &art).unwrap();
        fs::write(
            image_path(root.path(), &proj.slug, &id, "png").unwrap(),
            b"PNGDATA",
        )
        .unwrap();
        proj.art.push(id.clone());
        save_project(root.path(), &proj).unwrap();

        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());

        delete_art(
            root.path(),
            &proj.slug,
            &id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        let trashed = seen.into_inner();
        assert_eq!(
            trashed.len(),
            2,
            "both the image and the sidecar are trashed"
        );
        assert!(trashed.iter().any(|p| p.ends_with(format!("{}.png", id.0))));
        assert!(trashed
            .iter()
            .any(|p| p.ends_with(format!("{}.json", id.0))));
        assert!(graveyard.join(format!("{}.png", id.0)).exists());
        assert!(graveyard.join(format!("{}.json", id.0)).exists());

        let reloaded = load_project(root.path(), &proj.slug).unwrap();
        assert!(!reloaded.art.contains(&id));
    }

    /// Invariant: a track whose cover names the deleted artwork loses the cover;
    /// a track naming a *different* artwork is untouched. The twin of the lyric
    /// version test: a clear-everything bug passes the first half and fails the
    /// second.
    #[test]
    fn test_delete_art_clears_only_matching_track_covers() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let deleted = mint_art_id(&mut proj);
        let other = mint_art_id(&mut proj);
        register_artwork(root.path(), &mut proj, &deleted);
        register_artwork(root.path(), &mut proj, &other);

        // The slug is copied out first: `&proj.slug` and `&mut proj` in one call
        // is an immutable and a mutable borrow of the same value.
        let slug = proj.slug.clone();
        let track_a = track_with_cover(
            root.path(),
            &slug,
            &mut proj,
            "tr-0001",
            Some(deleted.clone()),
        );
        let track_b = track_with_cover(
            root.path(),
            &slug,
            &mut proj,
            "tr-0002",
            Some(other.clone()),
        );
        save_project(root.path(), &proj).unwrap();

        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        delete_art(
            root.path(),
            &proj.slug,
            &deleted,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        let reloaded_a = crate::tracks::load_track(root.path(), &proj.slug, &track_a.id).unwrap();
        let reloaded_b = crate::tracks::load_track(root.path(), &proj.slug, &track_b.id).unwrap();
        assert_eq!(reloaded_a.cover, None);
        assert_eq!(reloaded_b.cover, Some(other));
    }

    /// Invariant: an album whose cover names the deleted artwork loses the cover.
    #[test]
    fn test_delete_art_clears_matching_album_cover() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let id = mint_art_id(&mut proj);
        register_artwork(root.path(), &mut proj, &id);

        proj.albums.push(create_core::project::AlbumList {
            name: "A".to_string(),
            tracks: vec![],
            cover: Some(id.clone()),
        });
        save_project(root.path(), &proj).unwrap();

        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        delete_art(
            root.path(),
            &proj.slug,
            &id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        let reloaded = load_project(root.path(), &proj.slug).unwrap();
        assert_eq!(reloaded.albums[0].cover, None);
    }

    /// Invariant: an unknown id is `NotFound` and the trasher is never called.
    #[test]
    fn test_delete_art_of_unknown_id_is_not_found_and_trashes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let seen = std::cell::RefCell::new(Vec::new());

        let err = delete_art(
            root.path(),
            &proj.slug,
            &ArtId("ar-9999".to_string()),
            recording_trasher(&seen, root.path()),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LibraryError::NotFound {
                kind: "artwork",
                ..
            }
        ));
        assert!(seen.into_inner().is_empty());
    }

    /// Invariant: a missing image file is tolerated -- the sidecar still goes
    /// and the id still unlists.
    #[test]
    fn test_delete_art_tolerates_a_missing_image_file() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let id = mint_art_id(&mut proj);
        let art = sample_artwork(id.clone(), format!("art/{}.png", id.0));
        save_art(root.path(), &proj.slug, &art).unwrap();
        // Deliberately do not write the image file.
        proj.art.push(id.clone());
        save_project(root.path(), &proj).unwrap();

        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        delete_art(
            root.path(),
            &proj.slug,
            &id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        let trashed = seen.into_inner();
        assert_eq!(trashed.len(), 1, "only the sidecar is trashed");
        assert!(trashed[0].ends_with(format!("{}.json", id.0)));
        assert!(!load_project(root.path(), &proj.slug)
            .unwrap()
            .art
            .contains(&id));
    }

    /// Invariant: the id is not reissued after a delete.
    #[test]
    fn test_delete_art_does_not_reissue_the_id() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let id = mint_art_id(&mut proj);
        register_artwork(root.path(), &mut proj, &id);
        save_project(root.path(), &proj).unwrap();

        let graveyard = root.path().join("graveyard");
        fs::create_dir_all(&graveyard).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        delete_art(
            root.path(),
            &proj.slug,
            &id,
            recording_trasher(&seen, &graveyard),
        )
        .unwrap();

        let mut reloaded = load_project(root.path(), &proj.slug).unwrap();
        let next = mint_art_id(&mut reloaded);
        assert_ne!(next, id, "the deleted id must not be minted again");
    }

    /// Write an artwork sidecar and list its id on the project, so the artwork
    /// is one this project owns. Built through `sample_artwork` rather than by
    /// hand -- a third `Artwork` literal in this file would be a third thing to
    /// update the next time the struct grows a field.
    fn register_artwork(root: &Path, project: &mut Project, id: &ArtId) {
        let art = sample_artwork(id.clone(), format!("art/{}.png", id.0));
        save_art(root, &project.slug, &art).unwrap();
        project.art.push(id.clone());
    }
}
