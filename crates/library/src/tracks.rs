//! Track sidecars and audio files on disk: `<app config dir>/projects/<slug>/tracks/`.
//!
//! One sidecar per track (`<id>.json`) holds the full provenance; the audio
//! file (`<id>.<ext>`) lives beside it. `project.json` holds only the id, so
//! this module is the only thing that reads or writes track records.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use create_core::audio::flac_duration_s;
use create_core::project::{Project, TrackId};
use create_core::provenance::Track;
use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::projects::project_dir;
use crate::LibraryError;

/// Directory inside a project holding its audio files and sidecars.
pub const TRACKS_DIR: &str = "tracks";

/// Prefix every minted track id carries.
const ID_PREFIX: &str = "tr-";

/// Digits a minted id is padded to.
///
/// Padded so that sorting the directory by name matches the order the tracks
/// were created in -- `tr-10` would otherwise sort before `tr-2`.
const ID_DIGITS: usize = 4;

/// Something about listing a project's tracks the user should be told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrackWarning {
    /// `project.json` lists an id with no sidecar behind it. Surfaced rather than
    /// dropped: a track the user generated is missing, and silence would read as
    /// "there was never one".
    Missing { id: String },
    /// The sidecar exists but could not be read.
    Unreadable { id: String, detail: String },
    /// The sidecar is not a valid track record.
    Malformed { id: String, detail: String },
}

/// Every track a project could offer, plus anything worth reporting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackSet {
    /// In `Project::tracks` order -- the order they were generated.
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub warnings: Vec<TrackWarning>,
}

/// Whether an id may be joined onto a project directory.
///
/// A whitelist, for the same reason project slugs use one: ids reach this crate
/// from the frontend, and `..` or a separator would read and write outside the
/// library.
fn is_safe_track_id(id: &str) -> bool {
    id.len() > ID_PREFIX.len()
        && id.len() <= ID_PREFIX.len() + 12
        && id.starts_with(ID_PREFIX)
        && id[ID_PREFIX.len()..].chars().all(|c| c.is_ascii_digit())
}

/// The tracks directory for one project.
pub fn tracks_dir(root: &Path, slug: &str) -> Result<PathBuf, LibraryError> {
    Ok(project_dir(root, slug)?.join(TRACKS_DIR))
}

/// The sidecar file for one track, refusing any id that could escape the project.
pub fn sidecar_path(root: &Path, slug: &str, id: &TrackId) -> Result<PathBuf, LibraryError> {
    if !is_safe_track_id(&id.0) {
        return Err(LibraryError::UnusableName(id.0.clone()));
    }
    Ok(tracks_dir(root, slug)?.join(format!("{}.json", id.0)))
}

/// The audio file for one track, refusing any id or extension that could escape
/// the project or carry a separator.
///
/// `ext` is taken from the produced file, lowercased, and must be entirely
/// ASCII alphanumeric. The extension on `Track::file` is this app's record of
/// the real output format, which is why no separate format field is being added.
pub fn audio_path(
    root: &Path,
    slug: &str,
    id: &TrackId,
    ext: &str,
) -> Result<PathBuf, LibraryError> {
    if !is_safe_track_id(&id.0) {
        return Err(LibraryError::UnusableName(id.0.clone()));
    }
    let ext_lower = ext.to_ascii_lowercase();
    if ext_lower.is_empty() || !ext_lower.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(LibraryError::UnusableName(ext.to_string()));
    }
    Ok(tracks_dir(root, slug)?.join(format!("{}.{}", id.0, ext_lower)))
}

/// Resolves a track's stored `file` -- relative to the project directory, e.g.
/// `"tracks/tr-0001.flac"` -- to an absolute path the webview can play.
///
/// `Track.file` is written by this app, but it lives in a JSON sidecar the user
/// can open and edit: a hand-edited sidecar could name any file. An absolute
/// path, or one whose `..` walks out of the project, is refused rather than
/// handed to the webview as a path to serve. The asset protocol's own scope is
/// the second gate; this is the first.
pub fn resolve_track_file(root: &Path, slug: &str, file: &str) -> Result<PathBuf, LibraryError> {
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

/// Takes the next track id for `project` and advances its counter.
///
/// The counter is the only source of ids. Deriving one from the files present
/// would hand a deleted track's id to a later one, and an `AlbumList` still
/// holding the old `TrackId` would then point at unrelated audio.
pub fn mint_track_id(project: &mut Project) -> TrackId {
    let id = TrackId(format!(
        "{ID_PREFIX}{:0width$}",
        project.next_track_seq,
        width = ID_DIGITS
    ));
    project.next_track_seq = project.next_track_seq.saturating_add(1);
    id
}

/// Writes one track sidecar.
///
/// The sidecar is written before `project.json` gains the id. If the project
/// write then fails, the result is a sidecar nothing references -- invisible,
/// recoverable, harmless -- rather than a project listing a track whose file is
/// not there, which is the state that makes the Library view lie.
pub fn save_track(root: &Path, slug: &str, track: &Track) -> Result<(), LibraryError> {
    let path = sidecar_path(root, slug, &track.id)?;
    atomic::write_json(&path, track)
}

/// Loads one track sidecar by id.
///
/// Fails when it is absent: the caller named this track, and an empty one
/// would look like the user's provenance had been wiped.
pub fn load_track(root: &Path, slug: &str, id: &TrackId) -> Result<Track, LibraryError> {
    let path = sidecar_path(root, slug, id)?;
    let text = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LibraryError::NotFound {
                kind: "track sidecar",
                id: id.0.clone(),
            }
        } else {
            LibraryError::Io(e)
        }
    })?;
    Ok(serde_json::from_str(&text)?)
}

/// Reads every track `project` lists. **Never fails.**
///
/// Driven by `Project::tracks` rather than by the directory, so the order is the
/// user's and a stray file left by a failed write is ignored rather than
/// appearing as a track. An id with nothing behind it becomes a warning.
pub fn list_tracks(root: &Path, project: &Project) -> TrackSet {
    let mut tracks = Vec::with_capacity(project.tracks.len());
    let mut warnings = Vec::new();

    for id in &project.tracks {
        let path = match sidecar_path(root, &project.slug, id) {
            Ok(path) => path,
            Err(e) => {
                warnings.push(TrackWarning::Unreadable {
                    id: id.0.clone(),
                    detail: e.to_string(),
                });
                continue;
            }
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warnings.push(TrackWarning::Missing { id: id.0.clone() });
                continue;
            }
            Err(e) => {
                warnings.push(TrackWarning::Unreadable {
                    id: id.0.clone(),
                    detail: e.to_string(),
                });
                continue;
            }
        };
        match serde_json::from_str::<Track>(&text) {
            Ok(track) => tracks.push(track),
            Err(e) => warnings.push(TrackWarning::Malformed {
                id: id.0.clone(),
                detail: e.to_string(),
            }),
        }
    }

    TrackSet { tracks, warnings }
}

/// Length in seconds, read from the audio file's header.
///
/// Returns `None` when the file is missing, not a FLAC, or its header does not
/// carry a known length. A missing duration must never stop a track from being
/// recorded.
pub fn duration_of(path: &Path) -> Option<f64> {
    let mut file = fs::File::open(path).ok()?;
    let mut head = [0u8; 42];
    // `read_exact`, not `read`: a single `read` may legally return fewer bytes
    // than asked for, and a short read on a perfectly good file would then be
    // indistinguishable from a truncated one and report no duration. A file
    // shorter than 42 bytes cannot be a FLAC anyway -- magic, block header and
    // STREAMINFO come to 42 bytes before any audio -- so the `UnexpectedEof`
    // this returns for one is the right answer.
    file.read_exact(&mut head).ok()?;
    flac_duration_s(&head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_core::generation::{GenerationSpec, InputValue, LoraRef};
    use create_core::profile::SlotAddress;
    use create_core::provenance::{ComfyServerInfo, Provenance};
    use std::collections::BTreeMap;

    const NOW: &str = "2026-08-25T20:11:04Z";

    fn project(root: &Path) -> Project {
        crate::projects::create_project(root, "Night Drive", NOW).unwrap()
    }

    fn sample_track(id: TrackId, file: String) -> Track {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "tags".to_string(),
            InputValue::Text("synthwave".to_string()),
        );
        inputs.insert("duration_s".to_string(), InputValue::Float(120.0));
        inputs.insert("seed".to_string(), InputValue::Seed(42));

        let spec = GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs,
            loras: vec![
                LoraRef {
                    file: "lora_a.safetensors".to_string(),
                    strength: 1.0,
                    enabled: true,
                },
                LoraRef {
                    file: "lora_b.safetensors".to_string(),
                    strength: 0.8,
                    enabled: true,
                },
            ],
            lyrics: None,
        };

        let mut resolved_slots = BTreeMap::new();
        resolved_slots.insert(
            SlotAddress("94.tags".to_string()),
            InputValue::Text("synthwave".to_string()),
        );
        resolved_slots.insert(
            SlotAddress("94.duration".to_string()),
            InputValue::Float(120.0),
        );
        resolved_slots.insert(
            SlotAddress("98.seconds".to_string()),
            InputValue::Float(120.0),
        );
        resolved_slots.insert(SlotAddress("94.seed".to_string()), InputValue::Seed(42));

        Track {
            id,
            title: Some("Midnight Drive".to_string()),
            file,
            duration_s: Some(120.0),
            provenance: Provenance {
                profile_id: "ace-step-1.5-turbo".to_string(),
                profile_display_name: "ACE-Step 1.5 XL Turbo".to_string(),
                model_license: "Apache-2.0".to_string(),
                template: Some("audio_ace_step1_5_xl_turbo".to_string()),
                spec,
                resolved_slots,
                comfy: Some(ComfyServerInfo {
                    comfyui_version: Some("0.3.26".to_string()),
                    comfy_cli_version: Some("0.1.0".to_string()),
                    url: Some("http://127.0.0.1:8188".to_string()),
                }),
                created_at: "2026-08-23T18:31:24Z".to_string(),
                prompt_id: None,
            },
        }
    }

    /// Invariant: ids come from the counter, never from the files present. A
    /// deleted track's id must not be handed to a later one, or an AlbumList
    /// would point at audio generated for a different song.
    #[test]
    fn test_mint_track_id_never_reuses_a_deleted_id() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = mint_track_id(&mut proj);
        assert_eq!(first.0, "tr-0001");

        // Simulate a delete: id removed from the project, file ignored.
        proj.tracks.clear();

        let second = mint_track_id(&mut proj);
        assert_ne!(second.0, first.0);
        assert_eq!(second.0, "tr-0002");
    }

    /// Invariant: a track's stored path resolves to an absolute file under the
    /// project. It must be absolute or `convertFileSrc` cannot turn it into an
    /// asset URL.
    #[test]
    fn test_resolve_track_file_returns_an_absolute_path() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        std::fs::create_dir_all(tracks_dir(root.path(), &proj.slug).unwrap()).unwrap();

        let resolved = resolve_track_file(root.path(), &proj.slug, "tracks/tr-0001.flac").unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with(Path::new("tracks").join("tr-0001.flac")));
    }

    /// Invariant: a sidecar edited to name an absolute path is refused, not
    /// served. `Path::join` replaces its base with an absolute right-hand side,
    /// so this must be caught before the join.
    #[test]
    fn test_resolve_track_file_refuses_an_absolute_path() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let abs = std::env::temp_dir().join("outside.flac");

        let err = resolve_track_file(root.path(), &proj.slug, abs.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, LibraryError::UnusableName(_)));
    }

    /// Invariant: a `..` in a hand-edited sidecar cannot walk out of the
    /// project, wherever it appears in the path.
    #[test]
    fn test_resolve_track_file_refuses_a_parent_escape() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        std::fs::create_dir_all(tracks_dir(root.path(), &proj.slug).unwrap()).unwrap();

        let leading = Path::new("..").join("config.json");
        for file in ["tracks/../outside.flac", leading.to_str().unwrap()] {
            let err = resolve_track_file(root.path(), &proj.slug, file).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "file {file:?} should be refused"
            );
        }
    }

    /// Invariant: a track sidecar round-trips, including a two-LoRA spec.
    #[test]
    fn test_save_track_then_load_track_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let id = TrackId("tr-0001".to_string());
        let track = sample_track(id, "tracks/tr-0001.flac".to_string());

        save_track(root.path(), &proj.slug, &track).unwrap();
        let loaded = load_track(root.path(), &proj.slug, &track.id).unwrap();
        assert_eq!(loaded, track);
    }

    /// Invariant: a slug from the frontend cannot address anything outside the
    /// project's tracks directory.
    #[test]
    fn test_track_paths_refuse_a_slug_that_escapes_the_root() {
        let root = tempfile::tempdir().unwrap();
        let id = TrackId("tr-0001".to_string());
        for slug in ["../secrets", "a/b", "C:\\Windows", "Night Drive", ""] {
            let err = sidecar_path(root.path(), slug, &id).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "slug {slug:?} should be refused"
            );
        }
    }

    /// Invariant: the duration comes off the real file, not the spec.
    ///
    /// Uses the committed 42-byte head of a real generated FLAC. Its length is
    /// exactly the read size, which is the case `read_exact` must handle: the
    /// `read` this replaced could legally have returned fewer bytes and
    /// reported no duration for a perfectly good file.
    #[test]
    fn test_duration_of_reads_a_real_flac_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tr-0001.flac");
        let head = include_bytes!("../../../testdata/audio/ace-step.flac.head");
        fs::write(&path, head).unwrap();

        assert_eq!(duration_of(&path), Some(120.0));
    }

    /// Invariant: a missing or unreadable file is a missing duration, never an
    /// error. A track whose audio is fine must not fail to be recorded because
    /// its header could not be read.
    #[test]
    fn test_duration_of_is_none_for_a_file_that_is_not_there() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(duration_of(&dir.path().join("nope.flac")), None);
    }

    /// Invariant: a file too short to hold a header is refused, not panicked on.
    #[test]
    fn test_duration_of_is_none_for_a_file_shorter_than_a_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stub.flac");
        fs::write(&path, b"fLaC").unwrap();

        assert_eq!(duration_of(&path), None);
    }

    /// Invariant: a track id from the frontend cannot address anything outside
    /// the tracks directory.
    ///
    /// Separate from the slug test above, and it has to be: that one pairs a
    /// **valid** id with hostile slugs, so `project_dir` refuses them and
    /// `is_safe_track_id` is never reached. Disabling the id whitelist entirely
    /// left the whole suite green until this test existed.
    #[test]
    fn test_track_paths_refuse_an_id_that_escapes_the_tracks_dir() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        for bad in [
            "../../config",
            "tr-0001/../../secrets",
            "tr-0001.json",
            "tr-",
            "",
            "ld-0001",
            "tr-abcd",
        ] {
            let id = TrackId(bad.to_string());
            assert!(
                matches!(
                    sidecar_path(root.path(), &proj.slug, &id),
                    Err(LibraryError::UnusableName(_))
                ),
                "sidecar_path should refuse id {bad:?}"
            );
            assert!(
                matches!(
                    audio_path(root.path(), &proj.slug, &id, "flac"),
                    Err(LibraryError::UnusableName(_))
                ),
                "audio_path should refuse id {bad:?}"
            );
        }
    }

    /// Invariant: an extension that is not purely ASCII alphanumeric is refused.
    /// It reaches this function from a filename comfy-cli chose.
    #[test]
    fn test_audio_path_refuses_an_extension_that_is_not_alphanumeric() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let id = TrackId("tr-0001".to_string());
        for ext in ["fl/ac", "flac!", ".flac", ""] {
            let err = audio_path(root.path(), &proj.slug, &id, ext).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "ext {ext:?} should be refused"
            );
        }
    }

    /// Invariant: the listing follows the project's own order, and a stray file
    /// nothing references is not mistaken for a track.
    #[test]
    fn test_list_tracks_returns_them_in_project_order() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = sample_track(
            TrackId("tr-0001".to_string()),
            "tracks/tr-0001.flac".to_string(),
        );
        let second = sample_track(
            TrackId("tr-0002".to_string()),
            "tracks/tr-0002.flac".to_string(),
        );
        let third = sample_track(
            TrackId("tr-0003".to_string()),
            "tracks/tr-0003.flac".to_string(),
        );
        save_track(root.path(), &proj.slug, &first).unwrap();
        save_track(root.path(), &proj.slug, &second).unwrap();
        save_track(root.path(), &proj.slug, &third).unwrap();

        proj.tracks = vec![third.id.clone(), first.id.clone(), second.id.clone()];
        let set = list_tracks(root.path(), &proj);
        let ids: Vec<&str> = set.tracks.iter().map(|t| t.id.0.as_str()).collect();
        assert_eq!(ids, vec!["tr-0003", "tr-0001", "tr-0002"]);
        assert!(set.warnings.is_empty());
    }

    /// Invariant: one malformed sidecar does not hide the others, and it is
    /// named so the user can find it.
    #[test]
    fn test_a_malformed_sidecar_costs_one_track_not_the_library() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let first = sample_track(
            TrackId("tr-0001".to_string()),
            "tracks/tr-0001.flac".to_string(),
        );
        let bad = sample_track(
            TrackId("tr-0002".to_string()),
            "tracks/tr-0002.flac".to_string(),
        );
        let third = sample_track(
            TrackId("tr-0003".to_string()),
            "tracks/tr-0003.flac".to_string(),
        );
        save_track(root.path(), &proj.slug, &first).unwrap();
        save_track(root.path(), &proj.slug, &third).unwrap();
        fs::write(
            sidecar_path(root.path(), &proj.slug, &bad.id).unwrap(),
            "{ not json",
        )
        .unwrap();

        proj.tracks = vec![first.id.clone(), bad.id.clone(), third.id.clone()];
        let set = list_tracks(root.path(), &proj);
        assert_eq!(set.tracks.len(), 2);
        assert_eq!(set.tracks[0].id, first.id);
        assert_eq!(set.tracks[1].id, third.id);
        assert!(matches!(
            set.warnings.as_slice(),
            [TrackWarning::Malformed { id, .. }] if id == &bad.id.0
        ));
    }

    /// Invariant: an id the project lists with no sidecar behind it is surfaced.
    /// Dropping it silently would tell the user they never generated the track.
    #[test]
    fn test_a_missing_sidecar_is_a_warning_not_a_silence() {
        let root = tempfile::tempdir().unwrap();
        let mut proj = project(root.path());
        let present = sample_track(
            TrackId("tr-0001".to_string()),
            "tracks/tr-0001.flac".to_string(),
        );
        let missing = sample_track(
            TrackId("tr-0002".to_string()),
            "tracks/tr-0002.flac".to_string(),
        );
        save_track(root.path(), &proj.slug, &present).unwrap();

        proj.tracks = vec![present.id.clone(), missing.id.clone()];
        let set = list_tracks(root.path(), &proj);
        assert_eq!(set.tracks.len(), 1);
        assert_eq!(set.tracks[0].id, present.id);
        assert_eq!(
            set.warnings,
            vec![TrackWarning::Missing {
                id: missing.id.0.clone()
            }]
        );
    }

    /// Invariant: a project with no tracks offers an empty library, not an error.
    #[test]
    fn test_list_tracks_is_empty_for_a_project_with_none() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let set = list_tracks(root.path(), &proj);
        assert!(set.tracks.is_empty());
        assert!(set.warnings.is_empty());
    }
}
