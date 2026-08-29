//! Ingest finished ComfyUI outputs as tracks with provenance sidecars.
//!
//! Keeps every MCP call out of this module: tests run on real temp files with
//! no transport.

use std::path::Path;

use create_core::generation::{GenerationSpec, ResolvedSlots};
use create_core::project::{Project, TrackId};
use create_core::provenance::{ComfyServerInfo, Provenance, Track};
use mcp_bridge::{OutputBatch, OutputFile};
use thiserror::Error;

/// Everything a finished job needs to become a track, captured when it was
/// submitted.
///
/// Held in memory only. An app restart mid-job loses that job's provenance,
/// and that is deliberate rather than overlooked: the queue itself is in-memory
/// and does not survive a restart either.
#[derive(Debug, Clone)]
pub struct PendingTrack {
    /// Project the track is filed under.
    pub project_slug: String,
    pub profile_id: String,
    pub profile_display_name: String,
    pub model_license: String,
    pub template: Option<String>,
    /// What the user chose.
    pub spec: GenerationSpec,
    /// What ComfyUI actually received -- captured at submit, never recomputed.
    pub resolved_slots: ResolvedSlots,
    /// The server that ran it, when `server_info` could be read.
    pub comfy: Option<ComfyServerInfo>,
}

/// Ingestion failed before all tracks were saved.
#[derive(Debug, Error)]
pub enum IngestError {
    /// A library call failed.
    #[error("library: {0}")]
    Library(#[from] library::LibraryError),
    /// An IO operation failed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Audio extensions the app files as tracks.
const AUDIO_EXTS: &[&str] = &["flac", "wav", "mp3", "ogg", "opus", "m4a"];

/// Ingest every audio file in a completed job's output batch.
///
/// Non-audio files are skipped without error. Returns the saved tracks in the
/// order they appeared in the batch.
pub fn ingest_outputs(
    root: &Path,
    pending: &PendingTrack,
    batch: &OutputBatch,
    created_at: &str,
) -> Result<Vec<Track>, IngestError> {
    let mut tracks = Vec::new();
    for file in &batch.files {
        if let Some(track) = ingest_one_file(root, pending, file, created_at)? {
            tracks.push(track);
        }
    }
    Ok(tracks)
}

/// Ingest one downloaded output file, returning `None` when it is not audio.
fn ingest_one_file(
    root: &Path,
    pending: &PendingTrack,
    file: &OutputFile,
    created_at: &str,
) -> Result<Option<Track>, IngestError> {
    let ext = match audio_extension(&file.path) {
        Some(e) => e,
        None => return Ok(None),
    };

    // Load the project, mint an id, and persist the counter before any file
    // write. A crash after this point burns an id rather than overwriting a
    // track the user already has.
    let (mut project, id) = mint_and_save_project(root, &pending.project_slug)?;
    let track = write_track_file(
        root,
        &pending.project_slug,
        &id,
        &ext,
        &file.path,
        pending,
        created_at,
    )?;
    project.tracks.push(id);
    library::projects::save_project(root, &project)?;
    Ok(Some(track))
}

/// Load the project, mint the next track id, and persist the counter.
fn mint_and_save_project(root: &Path, slug: &str) -> Result<(Project, TrackId), IngestError> {
    let mut project = library::projects::load_project(root, slug)?;
    let id = library::tracks::mint_track_id(&mut project);
    library::projects::save_project(root, &project)?;
    Ok((project, id))
}

/// Move the downloaded audio into place and write its sidecar.
fn write_track_file(
    root: &Path,
    slug: &str,
    id: &TrackId,
    ext: &str,
    src: &Path,
    pending: &PendingTrack,
    created_at: &str,
) -> Result<Track, IngestError> {
    let tracks_dir = library::tracks::tracks_dir(root, slug)?;
    std::fs::create_dir_all(&tracks_dir)?;
    let dst = library::tracks::audio_path(root, slug, id, ext)?;
    std::fs::rename(src, &dst)?;
    let duration_s = library::tracks::duration_of(&dst);
    let file_rel = format!("tracks/{}.{}", id.0, ext);
    let track = build_track(id, &file_rel, duration_s, pending, created_at);
    library::tracks::save_track(root, slug, &track)?;
    Ok(track)
}

/// The lowercased audio extension, or `None` for files this app does not file.
fn audio_extension(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    if AUDIO_EXTS.contains(&ext.as_str()) {
        Some(ext)
    } else {
        None
    }
}

/// Build a `Track` from the pending record and the on-disk facts.
fn build_track(
    id: &TrackId,
    file: &str,
    duration_s: Option<f64>,
    pending: &PendingTrack,
    created_at: &str,
) -> Track {
    Track {
        id: id.clone(),
        title: None,
        file: file.to_string(),
        duration_s,
        provenance: Provenance {
            profile_id: pending.profile_id.clone(),
            profile_display_name: pending.profile_display_name.clone(),
            model_license: pending.model_license.clone(),
            template: pending.template.clone(),
            spec: pending.spec.clone(),
            resolved_slots: pending.resolved_slots.clone(),
            comfy: pending.comfy.clone(),
            created_at: created_at.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_core::generation::{InputValue, LoraRef, LyricDocId, LyricRef};
    use create_core::profile::SlotAddress;
    use mcp_bridge::OutputFile;
    use std::collections::BTreeMap;

    const NOW: &str = "2026-08-25T20:11:04Z";

    fn root_with_project() -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        let project = library::projects::create_project(root.path(), "Night Drive", NOW).unwrap();
        (root, project.slug)
    }

    fn pending(slug: &str, lyrics: bool, duration_s: f64) -> PendingTrack {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "tags".to_string(),
            InputValue::Text("synthwave".to_string()),
        );
        inputs.insert("duration_s".to_string(), InputValue::Float(duration_s));
        inputs.insert("seed".to_string(), InputValue::Seed(42));

        if lyrics {
            inputs.insert(
                "lyrics".to_string(),
                InputValue::Text("first line\nsecond line".to_string()),
            );
        }

        let mut resolved_slots = BTreeMap::new();
        resolved_slots.insert(
            SlotAddress("94.tags".to_string()),
            InputValue::Text("synthwave".to_string()),
        );
        resolved_slots.insert(
            SlotAddress("94.duration".to_string()),
            InputValue::Float(duration_s),
        );
        resolved_slots.insert(
            SlotAddress("98.seconds".to_string()),
            InputValue::Float(duration_s),
        );
        resolved_slots.insert(SlotAddress("94.seed".to_string()), InputValue::Seed(42));
        if lyrics {
            resolved_slots.insert(
                SlotAddress("94.lyrics".to_string()),
                InputValue::Text("first line\nsecond line".to_string()),
            );
        }

        PendingTrack {
            project_slug: slug.to_string(),
            profile_id: "ace-step-1.5-turbo".to_string(),
            profile_display_name: "ACE-Step 1.5 XL Turbo".to_string(),
            model_license: "Apache-2.0".to_string(),
            template: Some("audio_ace_step1_5_xl_turbo".to_string()),
            spec: GenerationSpec {
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
                lyrics: if lyrics {
                    Some(LyricRef {
                        doc_id: LyricDocId("ld-0001".to_string()),
                        version: 1,
                    })
                } else {
                    None
                },
            },
            resolved_slots,
            comfy: Some(ComfyServerInfo {
                comfyui_version: Some("0.3.26".to_string()),
                comfy_cli_version: Some("0.1.0".to_string()),
                url: Some("http://127.0.0.1:8188".to_string()),
            }),
        }
    }

    fn batch_with(path: &Path) -> OutputBatch {
        OutputBatch {
            prompt_id: Some("prompt-1".to_string()),
            out_dir: Some(path.parent().unwrap().to_path_buf()),
            files: vec![OutputFile {
                url: "http://example.com/output".to_string(),
                path: path.to_path_buf(),
                size: 0,
            }],
        }
    }

    /// Protects: the whole path writes audio, sidecar and project entry.
    #[test]
    fn test_ingest_writes_the_audio_the_sidecar_and_the_project_entry() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0);
        let batch = batch_with(&src);
        let tracks = ingest_outputs(root.path(), &pending, &batch, NOW).unwrap();

        assert_eq!(tracks.len(), 1);
        let track = &tracks[0];
        assert_eq!(track.id.0, "tr-0001");
        assert_eq!(track.file, "tracks/tr-0001.flac");

        let audio = library::tracks::audio_path(root.path(), &slug, &track.id, "flac").unwrap();
        assert!(audio.exists());
        let loaded = library::tracks::load_track(root.path(), &slug, &track.id).unwrap();
        assert_eq!(loaded.id, track.id);
        assert_eq!(loaded.file, track.file);

        let project = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(project.tracks, vec![track.id.clone()]);
    }

    /// Protects: the sidecar alone carries enough to reproduce the run.
    #[test]
    fn test_ingest_reproduces_from_the_sidecar_alone() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, true, 120.0);
        let batch = batch_with(&src);
        let tracks = ingest_outputs(root.path(), &pending, &batch, NOW).unwrap();

        let loaded = library::tracks::load_track(root.path(), &slug, &tracks[0].id).unwrap();
        let spec = loaded.provenance.spec;
        assert_eq!(spec.loras.len(), 2);
        assert_eq!(spec.loras[0].file, "lora_a.safetensors");
        assert_eq!(spec.loras[0].strength, 1.0);
        assert_eq!(spec.loras[1].file, "lora_b.safetensors");
        assert_eq!(spec.loras[1].strength, 0.8);
        assert_eq!(spec.seed(), Some(42));
        assert_eq!(
            loaded
                .provenance
                .resolved_slots
                .get(&SlotAddress("94.tags".to_string())),
            Some(&InputValue::Text("synthwave".to_string()))
        );
        assert_eq!(
            spec.inputs.get("lyrics"),
            Some(&InputValue::Text("first line\nsecond line".to_string()))
        );
    }

    /// Protects: the counter is persisted before the sidecar is written, so a
    /// crash between the two cannot reuse the id and overwrite an existing track.
    #[test]
    fn test_ingest_advances_the_counter_before_writing() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0);

        // Phase 1: mint and persist the counter, then read the project back.
        let (mut project, id) = mint_and_save_project(root.path(), &slug).unwrap();
        let on_disk = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(
            on_disk.next_track_seq, 2,
            "counter must be persisted before the sidecar write"
        );

        // Phase 2: write the track file and sidecar.
        let track = write_track_file(root.path(), &slug, &id, "flac", &src, &pending, NOW).unwrap();
        assert_eq!(track.id, id);

        // Phase 3: register the id and persist again.
        project.tracks.push(id);
        library::projects::save_project(root.path(), &project).unwrap();

        let final_project = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(final_project.tracks.len(), 1);
    }

    /// Protects: a failed write handing the same id to the next track.
    ///
    /// The invariant behind the three-write order, exercised through
    /// `ingest_outputs` rather than by calling the helpers in the order the
    /// test chooses -- which is what the counter test above does, and why it
    /// cannot see a reordering of the production path.
    ///
    /// The rename fails because the download is not there. If the counter were
    /// persisted last, `next_track_seq` would still be 1 here, and the next
    /// generation would mint `tr-0001` again and overwrite the audio **and**
    /// the sidecar of a track the user already had. Burning an id costs
    /// nothing; ids are never reused by design.
    #[test]
    fn test_a_failed_write_burns_the_id_rather_than_reusing_it() {
        let (root, slug) = root_with_project();
        let missing = root.path().join("tracks").join("never_downloaded_000.flac");
        let pending = pending(&slug, false, 120.0);
        let batch = batch_with(&missing);

        let result = ingest_outputs(root.path(), &pending, &batch, NOW);
        assert!(
            result.is_err(),
            "a missing download must not report success"
        );

        let on_disk = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(
            on_disk.next_track_seq, 2,
            "the id must be burned, not offered to the next track"
        );
        assert!(
            on_disk.tracks.is_empty(),
            "a track that was never written must not be registered"
        );
    }

    /// Protects: a non-audio output is skipped without producing a track.
    #[test]
    fn test_ingest_skips_a_non_audio_output() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.png");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, b"fake png").unwrap();

        let pending = pending(&slug, false, 120.0);
        let batch = batch_with(&src);
        let tracks = ingest_outputs(root.path(), &pending, &batch, NOW).unwrap();
        assert!(tracks.is_empty());

        let project = library::projects::load_project(root.path(), &slug).unwrap();
        assert!(project.tracks.is_empty());
    }

    /// Protects: the real file extension is recorded, not the requested format.
    #[test]
    fn test_ingest_records_the_real_extension_not_the_requested_one() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.wav");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0);
        let batch = batch_with(&src);
        let tracks = ingest_outputs(root.path(), &pending, &batch, NOW).unwrap();

        assert_eq!(tracks[0].file, "tracks/tr-0001.wav");
        let audio = library::tracks::audio_path(root.path(), &slug, &tracks[0].id, "wav").unwrap();
        assert!(audio.exists());
    }

    /// Protects: duration is read from the audio header, not copied from the spec.
    #[test]
    fn test_ingest_duration_comes_from_the_file() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 90.0);
        let batch = batch_with(&src);
        let tracks = ingest_outputs(root.path(), &pending, &batch, NOW).unwrap();

        assert_eq!(tracks[0].duration_s, Some(120.0));
    }
}
