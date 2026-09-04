//! Ingest finished ComfyUI outputs as tracks or artwork with provenance sidecars.
//!
//! Keeps every MCP call out of this module: tests run on real temp files with
//! no transport.

use std::path::Path;

use create_core::generation::{GenerationSpec, ResolvedSlots};
use create_core::profile::ModelKind;
use create_core::project::{ArtId, Project, TrackId};
use create_core::provenance::{Artwork, ComfyServerInfo, Provenance, Track};
use mcp_bridge::{OutputBatch, OutputFile};
use thiserror::Error;

/// Everything a finished job needs to become a track or artwork, captured when it
/// was submitted.
///
/// Held in memory only. An app restart mid-job loses that job's provenance,
/// and that is deliberate rather than overlooked: the queue itself is in-memory
/// and does not survive a restart either.
#[derive(Debug, Clone)]
pub struct PendingOutput {
    /// Project the asset is filed under.
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
    /// Which kind of asset this job's outputs become, taken from the profile at
    /// submit time. The *record* decides, not the file extension: an image job
    /// that somehow emits a `.flac` must not quietly become a track.
    pub kind: ModelKind,
}

/// Ingestion failed before all assets were saved.
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

/// Image extensions the app files as artwork.
///
/// Wider than the two save nodes `emit` recognises (`SaveImage` -> png,
/// `SaveImageWebP` -> webp) on purpose. A filter that is too tight drops a real
/// output **silently**, which is the exact failure this task exists to remove;
/// one that is too wide files something the user can see and delete.
const IMAGE_EXTS: &[&str] = &["png", "webp", "jpg", "jpeg"];

/// One asset a finished job produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Saved {
    Track(Track),
    Art(Artwork),
}

/// Ingest every file in a completed job's output batch.
///
/// Files that do not match the pending record's kind are skipped without error.
/// Returns the saved assets in the order they appeared in the batch.
pub fn ingest_outputs(
    root: &Path,
    pending: &PendingOutput,
    batch: &OutputBatch,
    created_at: &str,
    prompt_id: &str,
) -> Result<Vec<Saved>, IngestError> {
    let mut saved = Vec::new();
    for file in &batch.files {
        match pending.kind {
            ModelKind::Music => {
                if let Some(track) = ingest_one_file(root, pending, file, created_at, prompt_id)? {
                    saved.push(Saved::Track(track));
                }
            }
            ModelKind::Image => {
                if let Some(art) = ingest_one_art_file(root, pending, file, created_at, prompt_id)?
                {
                    saved.push(Saved::Art(art));
                }
            }
        }
    }
    Ok(saved)
}

/// Ingest one downloaded output file, returning `None` when it is not audio.
fn ingest_one_file(
    root: &Path,
    pending: &PendingOutput,
    file: &OutputFile,
    created_at: &str,
    prompt_id: &str,
) -> Result<Option<Track>, IngestError> {
    let ext = match filed_extension(&file.path, AUDIO_EXTS) {
        Some(e) => e,
        None => return Ok(None),
    };

    // Load the project, mint an id, and persist the counter before any file
    // write. A crash after this point burns an id rather than overwriting a
    // track the user already has.
    let (mut project, id) = mint_and_save_project(root, &pending.project_slug)?;
    let track = write_track_file(root, &id, &ext, &file.path, pending, created_at, prompt_id)?;
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
    id: &TrackId,
    ext: &str,
    src: &Path,
    pending: &PendingOutput,
    created_at: &str,
    prompt_id: &str,
) -> Result<Track, IngestError> {
    let slug = pending.project_slug.as_str();
    let tracks_dir = library::tracks::tracks_dir(root, slug)?;
    std::fs::create_dir_all(&tracks_dir)?;
    let dst = library::tracks::audio_path(root, slug, id, ext)?;
    std::fs::rename(src, &dst)?;
    let duration_s = library::tracks::duration_of(&dst);
    let file_rel = format!("tracks/{}.{}", id.0, ext);
    let track = build_track(id, &file_rel, duration_s, pending, created_at, prompt_id);
    library::tracks::save_track(root, slug, &track)?;
    Ok(track)
}

/// Ingest one downloaded output file, returning `None` when it is not an image.
fn ingest_one_art_file(
    root: &Path,
    pending: &PendingOutput,
    file: &OutputFile,
    created_at: &str,
    prompt_id: &str,
) -> Result<Option<Artwork>, IngestError> {
    let ext = match filed_extension(&file.path, IMAGE_EXTS) {
        Some(e) => e,
        None => return Ok(None),
    };

    // Load the project, mint an id, and persist the counter before any file
    // write. A crash after this point burns an id rather than overwriting an
    // artwork the user already has.
    let (mut project, id) = mint_and_save_art_project(root, &pending.project_slug)?;
    let art = write_art_file(root, &id, &ext, &file.path, pending, created_at, prompt_id)?;
    project.art.push(id);
    library::projects::save_project(root, &project)?;
    Ok(Some(art))
}

/// Load the project, mint the next artwork id, and persist the counter.
fn mint_and_save_art_project(root: &Path, slug: &str) -> Result<(Project, ArtId), IngestError> {
    let mut project = library::projects::load_project(root, slug)?;
    let id = library::art::mint_art_id(&mut project);
    library::projects::save_project(root, &project)?;
    Ok((project, id))
}

/// Move the downloaded image into place and write its sidecar.
fn write_art_file(
    root: &Path,
    id: &ArtId,
    ext: &str,
    src: &Path,
    pending: &PendingOutput,
    created_at: &str,
    prompt_id: &str,
) -> Result<Artwork, IngestError> {
    let slug = pending.project_slug.as_str();
    let art_dir = library::art::art_dir(root, slug)?;
    std::fs::create_dir_all(&art_dir)?;
    let dst = library::art::image_path(root, slug, id, ext)?;
    std::fs::rename(src, &dst)?;
    let (width, height) = match library::art::dimensions_of(&dst) {
        Some((w, h)) => (Some(w), Some(h)),
        None => (None, None),
    };
    let file_rel = format!("art/{}.{}", id.0, ext);
    let art = build_artwork(id, &file_rel, width, height, pending, created_at, prompt_id);
    library::art::save_art(root, slug, &art)?;
    Ok(art)
}

/// The lowercased extension, when it is one of `allowed`; `None` otherwise --
/// which is how a file this app does not file is skipped.
///
/// One function over both lists rather than one per kind: they differed only in
/// the constant they consulted, and a fix applied to one copy of a filter is a
/// fix the other kind silently does not get.
fn filed_extension(path: &Path, allowed: &[&str]) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    if allowed.contains(&ext.as_str()) {
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
    pending: &PendingOutput,
    created_at: &str,
    prompt_id: &str,
) -> Track {
    Track {
        id: id.clone(),
        // The title the user named at generation, carried on the spec (T-409). A
        // snapshot copied here, not a link back to the lyric document: retitling
        // the doc later never retitles a track already made. `None` stays the
        // untitled state the Library renders as the id.
        title: pending.spec.title.clone(),
        // A new track has no cover until the user chooses one. Cover is not
        // provenance -- it is an editable pointer, not part of the recipe.
        cover: None,
        file: file.to_string(),
        duration_s,
        provenance: build_provenance(pending, created_at, prompt_id),
    }
}

/// Build an `Artwork` from the pending record and the on-disk facts.
fn build_artwork(
    id: &ArtId,
    file: &str,
    width: Option<u32>,
    height: Option<u32>,
    pending: &PendingOutput,
    created_at: &str,
    prompt_id: &str,
) -> Artwork {
    Artwork {
        id: id.clone(),
        // The title the user named at generation, carried on the spec, exactly
        // as `Track::title` is (T-409). A snapshot, not a link.
        title: pending.spec.title.clone(),
        file: file.to_string(),
        width,
        height,
        provenance: build_provenance(pending, created_at, prompt_id),
    }
}

/// Build the shared `Provenance` record for any generated asset.
fn build_provenance(pending: &PendingOutput, created_at: &str, prompt_id: &str) -> Provenance {
    Provenance {
        profile_id: pending.profile_id.clone(),
        profile_display_name: pending.profile_display_name.clone(),
        model_license: pending.model_license.clone(),
        template: pending.template.clone(),
        spec: pending.spec.clone(),
        resolved_slots: pending.resolved_slots.clone(),
        comfy: pending.comfy.clone(),
        created_at: created_at.to_string(),
        prompt_id: Some(prompt_id.to_string()),
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
    const PROMPT: &str = "3f55e2fb-60e1-40b9-9cb7-61b37906622e";

    fn root_with_project() -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        let project = library::projects::create_project(root.path(), "Night Drive", NOW).unwrap();
        (root, project.slug)
    }

    fn pending(slug: &str, lyrics: bool, duration_s: f64, kind: ModelKind) -> PendingOutput {
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

        PendingOutput {
            project_slug: slug.to_string(),
            profile_id: "ace-step-1.5-turbo".to_string(),
            profile_display_name: "ACE-Step 1.5 XL Turbo".to_string(),
            model_license: "Apache-2.0".to_string(),
            template: Some("audio_ace_step1_5_xl_turbo".to_string()),
            spec: GenerationSpec {
                title: None,
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
            kind,
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

        let pending = pending(&slug, false, 120.0, ModelKind::Music);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        assert_eq!(saved.len(), 1);
        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
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

    /// Protects: the track takes its title from the spec the user generated
    /// with (T-409), not a hardcoded `None`. A snapshot copied at ingest -- the
    /// whole reason the title travels on the spec rather than being read from
    /// the lyric document later.
    #[test]
    fn test_ingest_carries_the_specs_title_onto_the_track() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let mut pending = pending(&slug, false, 120.0, ModelKind::Music);
        pending.spec.title = Some("Midnight Drive".to_string());
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
        assert_eq!(track.title.as_deref(), Some("Midnight Drive"));
        // On the sidecar, not just the returned value.
        let loaded = library::tracks::load_track(root.path(), &slug, &track.id).unwrap();
        assert_eq!(loaded.title.as_deref(), Some("Midnight Drive"));
    }

    /// Protects: a spec with no title yields an untitled track (`None`), which
    /// the Library renders as the id. Kills a mutation that fabricates a title.
    #[test]
    fn test_ingest_leaves_an_untitled_spec_untitled() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Music); // spec.title is None
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
        assert_eq!(track.title, None);
    }

    /// Protects: a new track has no cover. Cover is not provenance and must not
    /// be invented during ingest.
    #[test]
    fn test_ingest_leaves_a_new_track_without_a_cover() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Music);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
        assert_eq!(track.cover, None);
    }

    /// Protects: the sidecar alone carries enough to reproduce the run.
    #[test]
    fn test_ingest_reproduces_from_the_sidecar_alone() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, true, 120.0, ModelKind::Music);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
        let loaded = library::tracks::load_track(root.path(), &slug, &track.id).unwrap();
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

        let pending = pending(&slug, false, 120.0, ModelKind::Music);

        // Phase 1: mint and persist the counter, then read the project back.
        let (mut project, id) = mint_and_save_project(root.path(), &slug).unwrap();
        let on_disk = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(
            on_disk.next_track_seq, 2,
            "counter must be persisted before the sidecar write"
        );

        // Phase 2: write the track file and sidecar.
        let track =
            write_track_file(root.path(), &id, "flac", &src, &pending, NOW, PROMPT).unwrap();
        assert_eq!(track.id, id);

        // Phase 3: register the id and persist again.
        project.tracks.push(id);
        library::projects::save_project(root.path(), &project).unwrap();

        let final_project = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(final_project.tracks.len(), 1);
    }

    /// Protects: a track that cannot be traced to the run that made it.
    ///
    /// `GET /history/<prompt_id>` is the only surface reporting what the
    /// engine actually executed (MCP-SURFACE 17.2), so without this the check
    /// performed on T-311b means matching timestamps by hand.
    ///
    /// Asserted from the file rather than the returned value: the sidecar is
    /// the artifact, and a field that never reaches disk is not provenance.
    #[test]
    fn test_ingest_records_the_prompt_id_that_produced_the_track() {
        let (root, slug) = root_with_project();
        let src = root.path().join("tracks").join("prompt_000.flac");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/audio/ace-step.flac.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Music);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
        let loaded = library::tracks::load_track(root.path(), &slug, &track.id).unwrap();
        assert_eq!(loaded.provenance.prompt_id, Some(PROMPT.to_string()));
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
        let pending = pending(&slug, false, 120.0, ModelKind::Music);
        let batch = batch_with(&missing);

        let result = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT);
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

        let pending = pending(&slug, false, 120.0, ModelKind::Music);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();
        assert!(saved.is_empty());

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

        let pending = pending(&slug, false, 120.0, ModelKind::Music);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
        assert_eq!(track.file, "tracks/tr-0001.wav");
        let audio = library::tracks::audio_path(root.path(), &slug, &track.id, "wav").unwrap();
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

        let pending = pending(&slug, false, 90.0, ModelKind::Music);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let track = match &saved[0] {
            Saved::Track(t) => t,
            _ => panic!("expected track"),
        };
        assert_eq!(track.duration_s, Some(120.0));
    }

    /// Protects: an image output is filed as artwork rather than skipped.
    ///
    /// This is the regression the whole task exists for: before it, the same
    /// call returned an empty list and wrote nothing.
    #[test]
    fn test_an_image_output_is_filed_as_artwork_rather_than_skipped() {
        let (root, slug) = root_with_project();
        let src = root.path().join("art").join("prompt_000.png");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/images/klein-cover.png.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Image);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        assert_eq!(saved.len(), 1);
        let art = match &saved[0] {
            Saved::Art(a) => a,
            _ => panic!("expected artwork"),
        };
        assert_eq!(art.id.0, "ar-0001");
        assert_eq!(art.file, "art/ar-0001.png");

        let image = library::art::image_path(root.path(), &slug, &art.id, "png").unwrap();
        assert!(image.exists());
        let loaded = library::art::load_art(root.path(), &slug, &art.id).unwrap();
        assert_eq!(loaded.id, art.id);
        assert_eq!(loaded.file, art.file);

        let project = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(project.art, vec![art.id.clone()]);
    }

    /// Protects: the pending record's kind decides what is filed, not the file
    /// extension. A dispatch that read the extension first would pass one of
    /// these two directions.
    #[test]
    fn test_the_kind_decides_not_the_extension() {
        let (root, slug) = root_with_project();
        let png = root.path().join("tracks").join("prompt_000.png");
        let flac = root.path().join("art").join("prompt_000.flac");
        std::fs::create_dir_all(png.parent().unwrap()).unwrap();
        std::fs::create_dir_all(flac.parent().unwrap()).unwrap();
        std::fs::write(&png, b"fake png").unwrap();
        std::fs::write(&flac, b"fake flac").unwrap();

        let image_pending = pending(&slug, false, 120.0, ModelKind::Image);
        let image_batch = batch_with(&flac);
        let image_saved =
            ingest_outputs(root.path(), &image_pending, &image_batch, NOW, PROMPT).unwrap();
        assert!(image_saved.is_empty(), "image pending must not file a flac");

        let music_pending = pending(&slug, false, 120.0, ModelKind::Music);
        let music_batch = batch_with(&png);
        let music_saved =
            ingest_outputs(root.path(), &music_pending, &music_batch, NOW, PROMPT).unwrap();
        assert!(music_saved.is_empty(), "music pending must not file a png");
    }

    /// Protects: the artwork's provenance is built by the same constructor as
    /// the track's, so a field added to one cannot be missing from the other.
    #[test]
    fn test_artwork_provenance_matches_the_pending_record() {
        let (root, slug) = root_with_project();
        let src = root.path().join("art").join("prompt_000.png");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/images/klein-cover.png.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Image);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let art = match &saved[0] {
            Saved::Art(a) => a,
            _ => panic!("expected artwork"),
        };
        assert_eq!(art.provenance.profile_id, pending.profile_id);
        assert_eq!(
            art.provenance.profile_display_name,
            pending.profile_display_name
        );
        assert_eq!(art.provenance.model_license, pending.model_license);
        assert_eq!(art.provenance.template, pending.template);
        assert_eq!(art.provenance.spec, pending.spec);
        assert_eq!(art.provenance.resolved_slots, pending.resolved_slots);
        assert_eq!(art.provenance.comfy, pending.comfy);
        assert_eq!(art.provenance.prompt_id, Some(PROMPT.to_string()));
    }

    /// Protects: the pixel size is read from the file that was written.
    #[test]
    fn test_artwork_size_is_read_from_the_file() {
        let (root, slug) = root_with_project();
        let src = root.path().join("art").join("prompt_000.png");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/images/klein-cover.png.head");
        std::fs::write(&src, head).unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Image);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let art = match &saved[0] {
            Saved::Art(a) => a,
            _ => panic!("expected artwork"),
        };
        assert_eq!(art.width, Some(768));
        assert_eq!(art.height, Some(768));

        let loaded = library::art::load_art(root.path(), &slug, &art.id).unwrap();
        assert_eq!(loaded.width, Some(768));
        assert_eq!(loaded.height, Some(768));
    }

    /// Protects: a file with no readable header still becomes an artwork, with
    /// `None` for both dimensions.
    #[test]
    fn test_artwork_with_no_readable_header_records_none_for_size() {
        let (root, slug) = root_with_project();
        let src = root.path().join("art").join("prompt_000.png");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, b"not a png").unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Image);
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        let art = match &saved[0] {
            Saved::Art(a) => a,
            _ => panic!("expected artwork"),
        };
        assert_eq!(art.width, None);
        assert_eq!(art.height, None);
    }

    /// Protects: a failed write handing the same artwork id to the next one.
    ///
    /// The art mirror of the track test above, and written the same way for the
    /// same reason: through `ingest_outputs`, not by calling the helpers in the
    /// order the test chooses. The first version of this test minted and wrote
    /// by hand and so asserted only that `mint_and_save_art_project` saves --
    /// moving the save *after* the file write in the production path left it
    /// green.
    ///
    /// The rename fails because the download is not there. If the counter were
    /// persisted last, `next_art_seq` would still be 1 here and the next
    /// generation would mint `ar-0001` again, overwriting the image **and** the
    /// sidecar of artwork the user already had.
    #[test]
    fn test_a_failed_art_write_burns_the_id_rather_than_reusing_it() {
        let (root, slug) = root_with_project();
        let missing = root.path().join("art").join("never_downloaded_000.png");
        let pending = pending(&slug, false, 120.0, ModelKind::Image);
        let batch = batch_with(&missing);

        let result = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT);
        assert!(
            result.is_err(),
            "a missing download must not report success"
        );

        let on_disk = library::projects::load_project(root.path(), &slug).unwrap();
        assert_eq!(
            on_disk.next_art_seq, 2,
            "the id must be burned, not offered to the next artwork"
        );
        assert!(
            on_disk.art.is_empty(),
            "artwork that was never written must not be registered"
        );
    }

    /// Protects: two images in one batch become two artworks, in batch order.
    #[test]
    fn test_two_images_in_one_batch_become_two_artworks() {
        let (root, slug) = root_with_project();
        let first = root.path().join("art").join("prompt_000.png");
        let second = root.path().join("art").join("prompt_001.png");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/images/klein-cover.png.head");
        std::fs::write(&first, head).unwrap();
        std::fs::write(&second, head).unwrap();

        let pending = pending(&slug, false, 120.0, ModelKind::Image);
        let batch = OutputBatch {
            prompt_id: Some("prompt-1".to_string()),
            out_dir: Some(first.parent().unwrap().to_path_buf()),
            files: vec![
                OutputFile {
                    url: "http://example.com/output".to_string(),
                    path: first.to_path_buf(),
                    size: 0,
                },
                OutputFile {
                    url: "http://example.com/output".to_string(),
                    path: second.to_path_buf(),
                    size: 0,
                },
            ],
        };
        let saved = ingest_outputs(root.path(), &pending, &batch, NOW, PROMPT).unwrap();

        assert_eq!(saved.len(), 2);
        let ids: Vec<&str> = saved
            .iter()
            .map(|s| match s {
                Saved::Art(a) => a.id.0.as_str(),
                _ => panic!("expected artwork"),
            })
            .collect();
        assert_eq!(ids, vec!["ar-0001", "ar-0002"]);
    }

    /// Protects: the artwork's title travels from the spec, and `None` when the
    /// spec has none.
    #[test]
    fn test_artwork_title_travels_from_the_spec() {
        let (root, slug) = root_with_project();
        let src = root.path().join("art").join("prompt_000.png");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let head = include_bytes!("../../testdata/images/klein-cover.png.head");
        std::fs::write(&src, head).unwrap();

        let mut titled = pending(&slug, false, 120.0, ModelKind::Image);
        titled.spec.title = Some("Neon City".to_string());
        let batch = batch_with(&src);
        let saved = ingest_outputs(root.path(), &titled, &batch, NOW, PROMPT).unwrap();
        let art = match &saved[0] {
            Saved::Art(a) => a,
            _ => panic!("expected artwork"),
        };
        assert_eq!(art.title.as_deref(), Some("Neon City"));

        // Ingest **moves** the output into place, so the second run needs its
        // own file: reusing `src` was reading a path that is no longer there.
        let second = root.path().join("art").join("prompt_001.png");
        std::fs::write(&second, head).unwrap();

        let untitled = pending(&slug, false, 120.0, ModelKind::Image);
        let batch = batch_with(&second);
        let saved = ingest_outputs(root.path(), &untitled, &batch, NOW, PROMPT).unwrap();
        let art = match &saved[0] {
            Saved::Art(a) => a,
            _ => panic!("expected artwork"),
        };
        assert_eq!(art.title, None);
    }
}
