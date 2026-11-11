//! The generation pipeline: a `GenerationSpec` becomes a queued ComfyUI job.
//!
//! ARCHITECTURE 7, in order: load the profile's API-format workflow, audit
//! every resolved address, write the values, apply the graph edits slots
//! cannot express, record the exact graph, `POST /prompt`, and hand the prompt
//! id to the job pump.
//!
//! Two signals this module deliberately does not trust:
//!
//! - **ComfyUI's acceptance is not evidence an edit took effect.** A LoRA chain
//!   spliced in but feeding nothing validates, runs, and writes audio with no
//!   LoRA applied (MCP-SURFACE 17.1). Reachability is asserted in
//!   `create-core`'s own tests, before anything is submitted.
//! - **A value written is not a value read** unless the input holds a literal.
//!   The audit refuses a link-fed or missing address before the first write
//!   (ARCHITECTURE 3a).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use comfy_bridge::{ComfyHttp, SystemStats};
use create_core::audit::{audit_slots, write_slot};
use create_core::generation::{GenerationSpec, ResolvedSlots};
use create_core::graph::{ensure_lossless_output, splice_loras, GraphError, LoraChoice};
use create_core::profile::{ModelKind, ModelProfile};
use create_core::provenance::ComfyServerInfo;
use create_core::workflow::{detect_format, WorkflowFormat};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::comfy::{ensure_connected, EnsureError};
use crate::ingest::PendingOutput;
use crate::jobs::ComfyState;
use crate::projectctx::selected_project;
use crate::{ConfigDir, ProfilesDir};

/// What was queued.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Submission {
    /// The handle every later job call keys on.
    pub prompt_id: String,
    /// The exact graph submitted, saved as the record of what ran.
    pub workflow_path: String,
    /// Addresses that could not be checked. Always empty since T-519 -- in API
    /// format every address is checkable, and an unresolvable one is refused --
    /// kept on the wire so the queue panel's notes line needs no change.
    pub unchecked_slots: Vec<String>,
    /// Ids of the LoRA loader nodes spliced in, in apply order.
    pub lora_nodes: Vec<String>,
    /// The save format the graph edit wrote, `None` when the profile opted out.
    pub output_format: Option<String>,
}

/// Queue one audio generation and start its pump.
#[tauri::command]
pub async fn generate_audio(
    app: AppHandle,
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    spec: GenerationSpec,
) -> Result<Submission, String> {
    queue_generation(app, state, config_dir, profiles_dir, spec, ModelKind::Music).await
}

/// Queue one image generation and start its pump.
#[tauri::command]
pub async fn generate_image(
    app: AppHandle,
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    spec: GenerationSpec,
) -> Result<Submission, String> {
    queue_generation(app, state, config_dir, profiles_dir, spec, ModelKind::Image).await
}

/// Shared body of `generate_audio` and `generate_image`. A kind mismatch is
/// refused before anything connects.
async fn queue_generation(
    app: AppHandle,
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    spec: GenerationSpec,
    kind: ModelKind,
) -> Result<Submission, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    let loaded = set
        .profiles
        .get(&spec.profile_id)
        .ok_or_else(|| format!("no profile named {}", spec.profile_id))?;
    let profile = loaded.profile.clone();
    if let Some(err) = kind_error(&profile, kind) {
        return Err(err);
    }
    let source = workflow_source(&profile, &loaded.path)?;

    let comfy = ensure_connected(&state, &config_dir)
        .await
        .map_err(EnsureError::into_message)?;

    let record = record_path(&config_dir.0, &mint_job_id());
    let (submission, resolved) =
        build_and_submit(&comfy, &source, &record, &profile, &spec).await?;
    let server_info = comfy
        .system_stats()
        .await
        .ok()
        .map(|stats| server_info_of(&comfy, &stats));
    let project = selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let pending = PendingOutput {
        project_slug: project.slug,
        profile_id: profile.id.clone(),
        profile_display_name: profile.display_name.clone(),
        model_license: profile.license.clone(),
        template: profile.comfy.template.clone(),
        spec: spec.clone(),
        resolved_slots: resolved,
        comfy: server_info,
        kind,
    };
    state.pump(
        app,
        comfy,
        submission.prompt_id.clone(),
        Some(pending),
        config_dir.0.clone(),
    );
    Ok(submission)
}

/// Why this profile cannot be queued by this command, or `None` to go ahead.
///
/// A music profile queued as an image would file its FLAC nowhere -- a Done
/// row and no output -- so the mismatch is refused at submit, in a sentence
/// that says where the profile does belong.
fn kind_error(profile: &ModelProfile, wanted: ModelKind) -> Option<String> {
    if profile.kind == wanted {
        return None;
    }
    Some(match wanted {
        ModelKind::Music => format!(
            "{} is an image model, so it cannot generate audio. \
             Pick a music profile here, or generate artwork from Cover Art.",
            profile.id
        ),
        ModelKind::Image => format!(
            "{} is a music model, so it cannot generate cover art. \
             Pick an image profile here, or generate audio from the Audio Studio.",
            profile.id
        ),
    })
}

/// The profile's workflow file, resolved against the directory the profile
/// was loaded from (ARCHITECTURE 5).
fn workflow_source(profile: &ModelProfile, profile_file: &Path) -> Result<PathBuf, String> {
    let rel = profile.comfy.workflow.as_deref().ok_or_else(|| {
        format!(
            "{} has no workflow. Re-import it from a ComfyUI Workflow > Export (API) file.",
            profile.id
        )
    })?;
    let dir = profile_file.parent().unwrap_or_else(|| Path::new("."));
    Ok(dir.join(rel))
}

/// Read and check the profile's workflow, naming the user's file in every
/// message -- a path under `jobs/` would tell them nothing they can act on.
fn load_workflow(source: &Path, profile_id: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(source).map_err(|e| {
        format!(
            "{profile_id} could not read its workflow at {}: {e}. \
             Re-import it, or restore the file.",
            source.display()
        )
    })?;
    let graph: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{profile_id}'s workflow at {} is not valid JSON ({e}). \
             Re-export it from ComfyUI with Workflow > Export (API).",
            source.display()
        )
    })?;
    match detect_format(&graph) {
        Some(WorkflowFormat::Api) => Ok(graph),
        Some(WorkflowFormat::Frontend) => Err(format!(
            "{profile_id}'s workflow is a saved (editing) workflow, which latentCreate no longer \
             runs. In ComfyUI open it, choose Workflow > Export (API), and import that file."
        )),
        None => Err(format!(
            "{profile_id}'s workflow at {} is not a ComfyUI workflow.",
            source.display()
        )),
    }
}

/// Build this job's graph, record it at `record`, and submit it.
///
/// Returns the submission plus the resolved slots, so the caller can capture
/// them in the pending provenance record without recomputing.
pub(crate) async fn build_and_submit(
    comfy: &ComfyHttp,
    source: &Path,
    record: &Path,
    profile: &ModelProfile,
    spec: &GenerationSpec,
) -> Result<(Submission, ResolvedSlots), String> {
    // 1. This job's own in-memory copy of the profile's graph.
    let mut graph = load_workflow(source, &profile.id)?;

    // 2. Resolve, then refuse a write the engine would ignore or that has
    //    nowhere to go -- before the first write, so a profile bug costs nothing.
    let resolved = profile.resolve_slots(spec).map_err(|e| e.to_string())?;
    let addresses: Vec<String> = resolved.keys().map(|a| a.0.clone()).collect();
    let audit = audit_slots(&graph, &addresses);
    if !audit.missing.is_empty() {
        return Err(format!(
            "{} names {}, which its workflow does not have. Update latentCreate, or re-import \
             the workflow.",
            profile.id,
            audit.missing.join(", ")
        ));
    }
    if !audit.link_fed.is_empty() {
        let fed: Vec<&str> = audit.link_fed.iter().map(|f| f.address.as_str()).collect();
        return Err(format!(
            "{} writes {} to inputs another node drives, so the engine would ignore them",
            profile.id,
            fed.join(", ")
        ));
    }

    // 3. Values first, graph edits second: the edits add and retype nodes, and
    //    no address was checked against the edited graph.
    for (address, value) in &resolved {
        write_slot(&mut graph, &address.0, value.to_slot_value()).map_err(|e| e.to_string())?;
    }
    let edits = apply_graph_edits(&mut graph, profile, spec).map_err(|e| e.to_string())?;

    // 4. The record of what ran, written before submit so a refusal leaves the
    //    exact graph ComfyUI refused on disk for the bug report.
    if let Some(dir) = record.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&graph).map_err(|e| e.to_string())?;
    std::fs::write(record, text).map_err(|e| format!("{}: {e}", record.display()))?;

    // 5. ComfyUI validates on submit and names the node and input it refuses.
    let prompt_id = comfy.submit(&graph).await.map_err(|e| e.to_string())?;
    Ok((
        Submission {
            prompt_id,
            workflow_path: record.display().to_string(),
            unchecked_slots: Vec::new(),
            lora_nodes: edits.lora_nodes,
            output_format: edits.output_format,
        },
        resolved,
    ))
}

/// What the graph edits changed, for the submission record.
#[derive(Debug, Clone, PartialEq)]
struct GraphEdits {
    lora_nodes: Vec<String>,
    output_format: Option<String>,
}

/// Apply the edits no slot can express: the LoRA stack, then the save format.
fn apply_graph_edits(
    graph: &mut Value,
    profile: &ModelProfile,
    spec: &GenerationSpec,
) -> Result<GraphEdits, GraphError> {
    let stack: Vec<LoraChoice> = spec
        .active_loras()
        .map(|lora| LoraChoice {
            name: lora.file.clone(),
            strength: lora.strength,
        })
        .collect();
    let lora_nodes = match (&profile.loras, stack.is_empty()) {
        (_, true) => Vec::new(),
        (Some(support), false) => splice_loras(graph, support, &stack)?.nodes,
        (None, false) => {
            return Err(GraphError::Malformed {
                detail: format!("{} declares no LoRA support", profile.id),
            })
        }
    };
    let save = if profile.kind == ModelKind::Music {
        ensure_lossless_output(graph, &profile.comfy.output)?.format
    } else {
        None
    };
    Ok(GraphEdits {
        lora_nodes,
        output_format: save,
    })
}

/// The provenance record of the server that ran a job. comfy-cli no longer
/// exists in the chain, so its version is recorded as absent.
fn server_info_of(comfy: &ComfyHttp, stats: &SystemStats) -> ComfyServerInfo {
    ComfyServerInfo {
        comfyui_version: stats.system.comfyui_version.clone(),
        comfy_cli_version: None,
        url: Some(comfy.base_url().to_string()),
    }
}

/// Where one job's submitted graph is recorded.
fn record_path(root: &Path, job_id: &str) -> PathBuf {
    root.join("jobs").join(job_id).join("workflow.json")
}

/// A per-job directory name: epoch millis plus a counter, so two jobs queued in
/// the same millisecond still get their own record.
fn mint_job_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    format!("{millis:013}-{:04}", SEQ.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use comfy_bridge::transport::{FakeTransport, Method};
    use create_core::generation::{InputValue, LoraRef};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn profile(id: &str) -> (ModelProfile, PathBuf) {
        let file = repo().join("profiles").join(format!("{id}.json"));
        let text = std::fs::read_to_string(&file).expect("profile");
        let p: ModelProfile = serde_json::from_str(&text).expect("decodes");
        let source = workflow_source(&p, &file).expect("workflow");
        (p, source)
    }

    fn spec(profile_id: &str, inputs: &[(&str, InputValue)]) -> GenerationSpec {
        GenerationSpec {
            title: None,
            profile_id: profile_id.to_string(),
            inputs: inputs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect::<BTreeMap<_, _>>(),
            loras: Vec::new(),
            lyrics: None,
        }
    }

    fn ace_spec() -> GenerationSpec {
        spec(
            "ace-step-1.5-turbo",
            &[
                ("tags", InputValue::Text("synthwave".into())),
                ("seed", InputValue::Seed(u64::MAX)),
                ("duration_s", InputValue::Float(30.0)),
            ],
        )
    }

    fn accepting() -> Arc<FakeTransport> {
        let fake = Arc::new(FakeTransport::new());
        fake.on_json(
            Method::Post,
            "/prompt",
            &json!({"prompt_id": "abc-123", "number": 1, "node_errors": {}}),
        );
        fake
    }

    async fn submit(
        fake: &Arc<FakeTransport>,
        id: &str,
        spec: &GenerationSpec,
    ) -> (
        Result<(Submission, ResolvedSlots), String>,
        PathBuf,
        tempfile::TempDir,
    ) {
        let (p, source) = profile(id);
        let dir = tempfile::tempdir().expect("tempdir");
        let record = record_path(dir.path(), "job");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake.clone(), None);
        let out = build_and_submit(&comfy, &source, &record, &p, spec).await;
        (out, record, dir)
    }

    /// Protects: the submitted graph carries the user's values at the profile's
    /// addresses -- the seed as an exact u64, the duration in **both** places --
    /// is made lossless, is recorded on disk, and is what was POSTed.
    #[tokio::test]
    async fn test_pipeline_writes_values_edits_records_and_submits_one_graph() {
        let fake = accepting();
        let (out, record, _dir) = submit(&fake, "ace-step-1.5-turbo", &ace_spec()).await;
        let (submission, resolved) = out.expect("queued");
        assert_eq!(submission.prompt_id, "abc-123");
        assert_eq!(submission.output_format.as_deref(), Some("flac"));
        assert!(!resolved.is_empty());

        assert_eq!(fake.call_lines(), ["POST /prompt"]);
        let sent = fake.body_of(0)["prompt"].clone();
        assert_eq!(sent["109"]["inputs"]["value"], json!(u64::MAX));
        assert_eq!(sent["94"]["inputs"]["tags"], "synthwave");
        assert_eq!(sent["94"]["inputs"]["duration"], json!(30.0));
        assert_eq!(sent["98"]["inputs"]["seconds"], json!(30.0));
        assert_eq!(sent["111"]["inputs"]["format"], "flac");
        assert!(sent["111"]["inputs"].get("format.quality").is_none());
        assert_eq!(
            sent["3"]["inputs"]["seed"],
            json!(["109", 0]),
            "the link is untouched"
        );

        let on_disk: Value =
            serde_json::from_str(&std::fs::read_to_string(record).expect("record")).expect("json");
        assert_eq!(on_disk, sent, "the record is exactly what ran");
    }

    /// Protects: an enabled LoRA reaches the submitted graph, on the MODEL path;
    /// a bypassed one does not.
    #[tokio::test]
    async fn test_an_enabled_lora_is_spliced_and_a_bypassed_one_is_not() {
        let fake = accepting();
        let mut s = ace_spec();
        s.loras = vec![
            LoraRef {
                file: "mine\\style.safetensors".into(),
                strength: 0.8,
                enabled: true,
            },
            LoraRef {
                file: "off.safetensors".into(),
                strength: 1.0,
                enabled: false,
            },
        ];
        let (out, _, _dir) = submit(&fake, "ace-step-1.5-turbo", &s).await;
        let (submission, _) = out.expect("queued");
        assert_eq!(submission.lora_nodes, ["112"]);
        let sent = fake.body_of(0)["prompt"].clone();
        assert_eq!(
            sent["112"]["inputs"]["lora_name"],
            "mine\\style.safetensors"
        );
        assert_eq!(sent["78"]["inputs"]["model"], json!(["112", 0]));
        assert_eq!(sent["112"]["inputs"]["model"], json!(["104", 0]));
        assert!(!sent.to_string().contains("off.safetensors"));
    }

    /// Protects: MiniMax's flattened subgraph addresses and its int8 override
    /// both land.
    #[tokio::test]
    async fn test_minimax_writes_through_flattened_subgraph_ids() {
        let fake = accepting();
        let s = spec(
            "minimax-music-3",
            &[
                ("caption", InputValue::Text("lofi".into())),
                ("seed", InputValue::Seed(7)),
            ],
        );
        let (out, _, _dir) = submit(&fake, "minimax-music-3", &s).await;
        out.expect("queued");
        let sent = fake.body_of(0)["prompt"].clone();
        assert_eq!(sent["37:13"]["inputs"]["caption"], "lofi");
        assert_eq!(sent["37:38"]["inputs"]["seed"], json!(7));
        assert_eq!(
            sent["37:6"]["inputs"]["unet_name"],
            "minimax_music3_dit_int8_convrot.safetensors"
        );
    }

    /// Protects: an image profile is never given an audio save edit.
    #[tokio::test]
    async fn test_image_pipeline_makes_no_save_node_edit() {
        let fake = accepting();
        let s = spec(
            "flux-1-schnell-fp8",
            &[("tags", InputValue::Text("a fox".into()))],
        );
        let (out, _, _dir) = submit(&fake, "flux-1-schnell-fp8", &s).await;
        let (submission, _) = out.expect("queued");
        assert_eq!(submission.output_format, None);
        assert_eq!(fake.body_of(0)["prompt"]["6"]["inputs"]["text"], "a fox");
    }

    /// Protects: ComfyUI's refusal reaches the user as one sentence naming the
    /// node and input, and the refused graph is on disk for the report.
    #[tokio::test]
    async fn test_a_refusal_names_the_node_and_keeps_the_record() {
        let fake = Arc::new(FakeTransport::new());
        let path = repo().join("testdata/comfy-http/prompt_400_node_errors.json");
        let body: Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("f")).expect("j");
        fake.on(Method::Post, "/prompt", 400, body["body"].to_string());
        let (out, record, _dir) = submit(
            &fake,
            "flux-1-schnell-fp8",
            &spec("flux-1-schnell-fp8", &[]),
        )
        .await;
        let err = out.expect_err("refused");
        assert!(
            err.starts_with("ComfyUI rejected the workflow: EmptySD3LatentImage (#27)"),
            "{err}"
        );
        assert!(record.exists());
    }

    /// Protects: a profile address the workflow lacks stops the run before any
    /// request, naming the address.
    #[tokio::test]
    async fn test_an_address_the_workflow_lacks_stops_the_run() {
        let fake = accepting();
        let (mut p, source) = profile("flux-1-schnell-fp8");
        let mut spec = spec(
            "flux-1-schnell-fp8",
            &[("tags", InputValue::Text("x".into()))],
        );
        if let create_core::profile::InputSpec::Text { slots, .. } =
            p.inputs.get_mut("tags").expect("tags")
        {
            *slots = vec![create_core::profile::SlotAddress("999.text".into())];
        }
        spec.profile_id = p.id.clone();
        let dir = tempfile::tempdir().expect("tempdir");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake.clone(), None);
        let err = build_and_submit(&comfy, &source, &record_path(dir.path(), "j"), &p, &spec)
            .await
            .expect_err("refused");
        assert!(err.contains("999.text"), "{err}");
        assert!(fake.calls().is_empty());
    }

    /// Protects: a link-fed address (the 18.1 trap) stops the run before any
    /// request.
    #[tokio::test]
    async fn test_a_link_fed_address_stops_the_run() {
        let fake = accepting();
        let (mut p, source) = profile("ace-step-1.5-turbo");
        if let create_core::profile::InputSpec::Seed { slots, .. } =
            p.inputs.get_mut("seed").expect("seed")
        {
            *slots = vec![create_core::profile::SlotAddress("3.seed".into())];
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake.clone(), None);
        let err = build_and_submit(
            &comfy,
            &source,
            &record_path(dir.path(), "j"),
            &p,
            &ace_spec(),
        )
        .await
        .expect_err("refused");
        assert!(
            err.contains("3.seed") && err.contains("another node drives"),
            "{err}"
        );
        assert!(fake.calls().is_empty());
    }

    /// Protects: an old frontend-format import is refused with the menu path to
    /// the export that works -- not half-run.
    #[test]
    fn test_a_saved_editing_workflow_is_refused_with_the_menu_item() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("old.json");
        std::fs::write(&file, r#"{"nodes": [], "links": []}"#).expect("write");
        let err = load_workflow(&file, "mine").expect_err("refused");
        assert!(err.contains("Workflow > Export (API)"), "{err}");
    }

    #[test]
    fn test_a_missing_workflow_names_the_file_and_the_fix() {
        let err = load_workflow(Path::new("/nope/wf.json"), "mine").expect_err("missing");
        assert!(
            err.contains("wf.json") && err.contains("Re-import"),
            "{err}"
        );
    }

    #[test]
    fn test_workflow_resolves_beside_the_profile_and_absolute_paths_pass_through() {
        let (p, _) = profile("flux-1-schnell-fp8");
        let got = workflow_source(&p, Path::new("/x/profiles/flux.json")).expect("ok");
        assert_eq!(
            got,
            Path::new("/x/profiles").join("workflows/flux-1-schnell-fp8.json")
        );
    }

    #[test]
    fn test_job_ids_do_not_collide_within_a_millisecond() {
        assert_ne!(mint_job_id(), mint_job_id());
    }

    #[test]
    fn test_kind_error_refuses_the_wrong_command_and_permits_the_right_one() {
        let (ace, _) = profile("ace-step-1.5-turbo");
        let (flux, _) = profile("flux-1-schnell-fp8");
        assert!(kind_error(&ace, ModelKind::Image)
            .expect("refused")
            .contains("ace-step-1.5-turbo"));
        assert!(kind_error(&flux, ModelKind::Music)
            .expect("refused")
            .contains("flux-1-schnell-fp8"));
        assert!(kind_error(&ace, ModelKind::Music).is_none());
        assert!(kind_error(&flux, ModelKind::Image).is_none());
    }
}
