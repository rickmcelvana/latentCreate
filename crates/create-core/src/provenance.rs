use crate::generation::{GenerationSpec, ResolvedSlots};
use crate::project::TrackId;
use serde::{Deserialize, Serialize};

/// Which ComfyUI produced a track, for when a result cannot be reproduced later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComfyServerInfo {
    /// ComfyUI server version, if reported.
    #[serde(default)]
    pub comfyui_version: Option<String>,
    /// comfy-cli or comfy-mcp version, if reported.
    #[serde(default)]
    pub comfy_cli_version: Option<String>,
    /// Endpoint the job was submitted to, e.g. `"http://127.0.0.1:8188"`.
    #[serde(default)]
    pub url: Option<String>,
}

/// The full recipe for one generated asset.
///
/// Complete enough to reproduce the result -- including the LoRA stack, which lives in
/// `spec`. A LoRA-generated track that cannot be recreated from its sidecar is a bug
/// (CONVENTIONS.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// `ModelProfile::id`.
    pub profile_id: String,
    /// Display name at the time of generation.
    pub profile_display_name: String,
    /// The model's licence, copied at generation time -- some weights are
    /// open-with-conditions, and the user may need it long after generating.
    pub model_license: String,
    /// Gallery template name, when one was used.
    #[serde(default)]
    pub template: Option<String>,
    /// What the user chose, in semantic terms.
    pub spec: GenerationSpec,
    /// What ComfyUI actually received, after one control fanned out to its slots.
    #[serde(default)]
    pub resolved_slots: ResolvedSlots,
    /// Server that ran the job.
    #[serde(default)]
    pub comfy: Option<ComfyServerInfo>,
    /// RFC 3339, when generation finished.
    pub created_at: String,
    /// ComfyUI prompt id of the run that produced this track.
    ///
    /// `None` for a sidecar written before this field existed, and for any
    /// track whose origin is not a ComfyUI run.
    ///
    /// Not needed to reproduce the track -- everything for that is in `spec`
    /// and `resolved_slots` -- but it is the key to `GET /history/<prompt_id>`,
    /// the only surface that reports what the engine actually executed
    /// (MCP-SURFACE 17.2). Without it, matching a sidecar to its run means
    /// comparing timestamps by hand, which is exactly what verifying T-311b
    /// took.
    #[serde(default)]
    pub prompt_id: Option<String>,
}

/// One generated audio file: the contents of `tracks/<id>.json`, the sidecar that is
/// the single source of truth for this track (ARCHITECTURE 8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Track id, minted by `library`.
    pub id: TrackId,
    /// User-facing title, if the user has set one.
    #[serde(default)]
    pub title: Option<String>,
    /// Path relative to the project directory, e.g. `"tracks/abc123.flac"`.
    pub file: String,
    /// Length in seconds, when known.
    #[serde(default)]
    pub duration_s: Option<f64>,
    /// Full generation recipe for this track.
    pub provenance: Provenance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::{InputValue, LoraRef, LyricDocId, LyricRef};
    use crate::profile::SlotAddress;
    use std::collections::BTreeMap;

    /// Invariant: a sidecar written before `prompt_id` existed still loads.
    ///
    /// Not hypothetical -- `projects/my-first-song/tracks/tr-0001.json` was
    /// written on 2026-08-29, before this field, and is the only track the
    /// project has. A required field would have made it unreadable, and a
    /// `String` default would have loaded it as `""`, which is the
    /// absent-versus-empty confusion that has produced four bugs here. `None`
    /// says absent.
    #[test]
    fn test_a_sidecar_written_before_prompt_id_still_loads() {
        let json = r#"{
            "id": "tr-0001",
            "title": null,
            "file": "tracks/tr-0001.flac",
            "duration_s": 120.0,
            "provenance": {
                "profile_id": "ace-step-1.5-turbo",
                "profile_display_name": "ACE-Step 1.5 XL Turbo",
                "model_license": "Apache-2.0",
                "template": "audio_ace_step1_5_xl_turbo",
                "spec": {"profile_id": "ace-step-1.5-turbo", "inputs": {}},
                "created_at": "2026-08-29T05:56:07Z"
            }
        }"#;

        let track: Track = serde_json::from_str(json).expect("an older sidecar still loads");

        assert_eq!(track.provenance.prompt_id, None);
        assert_eq!(track.file, "tracks/tr-0001.flac");
    }

    #[test]
    fn test_track_sidecar_roundtrips() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "tags".to_string(),
            InputValue::Text("synthwave".to_string()),
        );
        inputs.insert("duration_s".to_string(), InputValue::Float(120.0));
        inputs.insert("seed".to_string(), InputValue::Seed(42));

        let spec = GenerationSpec {
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
            lyrics: Some(LyricRef {
                doc_id: LyricDocId("ld-1".to_string()),
                version: 2,
            }),
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

        let original = Track {
            id: TrackId("track-1".to_string()),
            title: Some("Midnight Drive".to_string()),
            file: "tracks/track-1.flac".to_string(),
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
                prompt_id: Some("3f55e2fb-60e1-40b9-9cb7-61b37906622e".to_string()),
            },
        };

        let json = serde_json::to_string_pretty(&original).unwrap();
        let parsed: Track = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_resolved_slots_records_fan_out() {
        let mut inputs = BTreeMap::new();
        inputs.insert("duration_s".to_string(), InputValue::Float(120.0));
        let spec = GenerationSpec {
            title: None,
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs,
            loras: vec![],
            lyrics: None,
        };

        let mut resolved_slots = BTreeMap::new();
        resolved_slots.insert(
            SlotAddress("94.duration".to_string()),
            InputValue::Float(120.0),
        );
        resolved_slots.insert(
            SlotAddress("98.seconds".to_string()),
            InputValue::Float(120.0),
        );

        let provenance = Provenance {
            profile_id: "ace-step-1.5-turbo".to_string(),
            profile_display_name: "ACE-Step 1.5 XL Turbo".to_string(),
            model_license: "Apache-2.0".to_string(),
            template: Some("audio_ace_step1_5_xl_turbo".to_string()),
            spec,
            resolved_slots,
            comfy: None,
            created_at: "2026-08-23T18:31:24Z".to_string(),
            prompt_id: Some("3f55e2fb-60e1-40b9-9cb7-61b37906622e".to_string()),
        };

        let json = serde_json::to_string(&provenance).unwrap();
        let parsed: Provenance = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed
                .resolved_slots
                .get(&SlotAddress("94.duration".to_string())),
            Some(&InputValue::Float(120.0))
        );
        assert_eq!(
            parsed
                .resolved_slots
                .get(&SlotAddress("98.seconds".to_string())),
            Some(&InputValue::Float(120.0))
        );
        assert_eq!(parsed.resolved_slots.len(), 2);
    }
}
