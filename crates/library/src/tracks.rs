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
}
