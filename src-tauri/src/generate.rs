//! The generation pipeline: a `GenerationSpec` becomes a queued ComfyUI job.
//!
//! ARCHITECTURE 7, in order: `fetch_template` to a **per-job** working copy,
//! `set_slots` for everything addressable, the graph edits slots cannot
//! express, `validate_workflow`, then submit into the job pump T-104b already
//! runs. Nothing here duplicates that lifecycle.
//!
//! Three things this module deliberately does NOT do, each of them a signal
//! that looks like it should be trusted and is not:
//!
//! - **`local_check` is not a gate.** It is evaluated when the template is
//!   fetched, before the profile's `slot_overrides` are applied, and MiniMax
//!   Music 3 is `runnable: false` for exactly the filename its own overrides
//!   correct (MCP-SURFACE 14.4). Gating on it would refuse to generate with a
//!   fully installed model.
//! - **A clean validation is not evidence an edit took effect.** A LoRA chain
//!   spliced in but feeding nothing validates clean, runs, and writes audio
//!   with no LoRA applied (17.1). Reachability is asserted in `create-core`'s
//!   own tests, before anything is submitted.
//! - **`applied` is not `effective`.** `set_workflow_slot` accepts a write to
//!   an input a real backend node drives, and the engine ignores it (18.1), so
//!   the resolved addresses are audited before the first write, not after.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use create_core::audit::{audit_slots, SlotAudit};
use create_core::generation::{GenerationSpec, ResolvedSlots};
use create_core::graph::{ensure_lossless_output, splice_loras, GraphError, LoraChoice};
use create_core::profile::ModelProfile;
use mcp_bridge::{Finding, LocalComfy, SlotOverride, Validation, Verdict};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::{ConfigDir, ProfilesDir};

/// What was queued, and what the app could not check while queueing it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Submission {
    /// The handle every later job call keys on.
    pub prompt_id: String,
    /// The working copy actually submitted -- the record of what ran.
    pub workflow_path: String,
    /// Resolved addresses the audit could not resolve: subgraph interiors, and
    /// addresses naming a node the top-level graph does not have. Reported
    /// rather than swallowed; MiniMax's seed is in here, unverified rather
    /// than working (MCP-SURFACE 18.5).
    pub unchecked_slots: Vec<String>,
    /// Ids of the LoRA loader nodes spliced in, in apply order.
    pub lora_nodes: Vec<String>,
    /// The save format the graph edit wrote, `None` when the profile opted out
    /// of the lossless swap.
    pub output_format: Option<String>,
}

/// What the graph edits changed, for the submission record.
#[derive(Debug, Clone, PartialEq)]
struct GraphEdits {
    lora_nodes: Vec<String>,
    output_format: Option<String>,
}

/// Queue one generation and start its pump. Returns as soon as ComfyUI has the
/// job; progress arrives on the `job://` events T-104b already emits.
#[tauri::command]
pub async fn generate_audio(
    app: AppHandle,
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    spec: GenerationSpec,
) -> Result<Submission, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    let profile = set
        .profiles
        .get(&spec.profile_id)
        .map(|loaded| loaded.profile.clone())
        .ok_or_else(|| format!("no profile named {}", spec.profile_id))?;

    let comfy = match ensure_connected(&state, &config_dir, None).await {
        Ok(comfy) => comfy,
        Err(EnsureError::Comfy(e)) => return Err(e.to_string()),
        Err(EnsureError::Log(detail)) => return Err(detail),
    };

    let workflow = workflow_path(&config_dir.0, &mint_job_id());
    if let Some(dir) = workflow.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let submission = build_and_submit(&comfy, &workflow, &profile, &spec).await?;
    state.pump(app, comfy, submission.prompt_id.clone());
    Ok(submission)
}

/// Build this job's working copy and submit it.
///
/// Takes the workflow path rather than minting one so a test can place a real
/// captured template where `fetch_template` would have written it: a mock
/// transport reproduces comfy-mcp's replies, not its side effects.
pub(crate) async fn build_and_submit(
    comfy: &LocalComfy,
    workflow: &Path,
    profile: &ModelProfile,
    spec: &GenerationSpec,
) -> Result<Submission, String> {
    let template = profile.comfy.template.as_deref().ok_or_else(|| {
        format!(
            "{} declares no gallery template; imported workflows are not wired up yet",
            profile.id
        )
    })?;

    // 1. This job's own copy. Never a shared path: the MCP docs warn about
    //    TOCTOU, and two generations at once would edit each other's graph.
    comfy
        .fetch_template(template, workflow)
        .await
        .map_err(|e| e.to_string())?;

    // 2. Resolve, then refuse a write the engine would ignore -- before the
    //    write, so a profile bug costs nothing.
    let resolved = profile.resolve_slots(spec).map_err(|e| e.to_string())?;
    let addresses: Vec<String> = resolved.keys().map(|a| a.0.clone()).collect();
    let audit = audit_slots(&read_workflow(workflow)?, &addresses);
    let inert = inert_slots(&audit);
    if !inert.is_empty() {
        return Err(format!(
            "{} writes {} to inputs a node drives, so the engine would ignore them",
            profile.id,
            inert.join(", ")
        ));
    }

    // 3. Slot writes first, graph edits second, and never a slot write after an
    //    edit: the swap retypes the save node, and this app has not re-read the
    //    slot list that a new node class produces.
    let overrides = slot_overrides(&resolved);
    if !overrides.is_empty() {
        comfy
            .set_slots(workflow, &overrides)
            .await
            .map_err(|e| e.to_string())?;
    }

    // 4. The two edits slots cannot express, on the file set_slots just wrote.
    let mut graph = read_workflow(workflow)?;
    let edits = apply_graph_edits(&mut graph, profile, spec).map_err(|e| e.to_string())?;
    write_workflow(workflow, &graph)?;

    // 5. Cheap, and worth being precise about: this catches unknown enum
    //    values, out-of-range numbers and missing required inputs before any
    //    GPU time. It is blind to reachability (17.1) and to the save format's
    //    dynamic combo (16.1), so it proves nothing about step 4.
    let report = comfy.validate(workflow).await.map_err(|e| e.to_string())?;
    if let Some(reason) = validation_error(&report) {
        return Err(reason);
    }

    let run = comfy.run(workflow).await.map_err(|e| e.to_string())?;
    Ok(Submission {
        prompt_id: run.prompt_id,
        workflow_path: workflow.display().to_string(),
        unchecked_slots: audit.unchecked,
        lora_nodes: edits.lora_nodes,
        output_format: edits.output_format,
    })
}

/// The slot writes for one resolved spec, in the shape `set_slots` wants.
///
/// **`to_slot_value`, never `serde_json::to_value`.** `InputValue` is
/// adjacently tagged, so serialising it whole sends `{"type":"seed","value":42}`
/// where the slot wants `42` -- rejected with `[workflow_slot_invalid]` for
/// every generation (MCP-SURFACE 18.2).
fn slot_overrides(resolved: &ResolvedSlots) -> Vec<SlotOverride> {
    resolved
        .iter()
        .map(|(address, value)| SlotOverride::new(address.0.clone(), value.to_slot_value()))
        .collect()
}

/// Addresses whose write would be accepted, persisted, and then ignored.
fn inert_slots(audit: &SlotAudit) -> Vec<String> {
    audit
        .link_fed
        .iter()
        .filter(|fed| fed.is_inert())
        .map(|fed| fed.address.clone())
        .collect()
}

/// Why this validation must stop the run, or `None` to go ahead.
///
/// `Vacuous` is a failure: a report that examined no nodes is not a pass, and
/// treating it as one greenlights a graph nothing looked at (9.3).
fn validation_error(report: &Validation) -> Option<String> {
    match report.verdict() {
        Verdict::Valid => None,
        Verdict::Invalid => Some(format!(
            "ComfyUI rejected the workflow: {}",
            summarise(&report.errors)
        )),
        Verdict::Vacuous => {
            Some("validation examined no nodes, so it proves nothing about this workflow".into())
        }
    }
}

/// One line per finding, node id included so the user can be pointed at it.
fn summarise(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| match (&f.node_id, &f.message) {
            (Some(id), Some(msg)) => format!("node {id}: {msg}"),
            (None, Some(msg)) => msg.clone(),
            (Some(id), None) => format!("node {id}"),
            (None, None) => "unspecified error".to_string(),
        })
        .collect::<Vec<_>>()
        .join("; ")
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

    let save = ensure_lossless_output(graph, &profile.comfy.output)?;
    Ok(GraphEdits {
        lora_nodes,
        output_format: save.format,
    })
}

/// Where one job's working copy lives.
fn workflow_path(root: &Path, job_id: &str) -> PathBuf {
    root.join("jobs").join(job_id).join("workflow.json")
}

/// A per-job directory name: epoch millis plus a counter.
///
/// The counter is what makes it unique -- two jobs queued in the same
/// millisecond are ordinary, and a collision would have one run editing the
/// other's graph.
fn mint_job_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    format!("{millis:013}-{:04}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// Read the working copy back after a tool wrote it.
fn read_workflow(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Write the edited graph back. A plain write, not an atomic swap: this file is
/// this job's alone and nothing else reads it until it is submitted.
fn write_workflow(path: &Path, graph: &Value) -> Result<(), String> {
    let text = serde_json::to_string(graph).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_core::generation::{InputValue, LoraRef};
    use create_core::profile::SlotAddress;
    use mcp_bridge::mock::Reply;
    use mcp_bridge::test_helpers::client_and_log;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ACE: &str = include_str!("../../profiles/ace-step-1.5-turbo.json");

    fn ace() -> ModelProfile {
        serde_json::from_str(ACE).expect("profile decodes")
    }

    /// The real captured template, as `fetch_template` would have written it.
    fn fixture() -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../testdata/workflows/ace_step_1_5_xl_turbo.json");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    fn spec() -> GenerationSpec {
        let mut inputs = BTreeMap::new();
        inputs.insert("tags".to_string(), InputValue::Text("synthwave".into()));
        inputs.insert("seed".to_string(), InputValue::Seed(42));
        inputs.insert("duration_s".to_string(), InputValue::Float(30.0));
        GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs,
            loras: Vec::new(),
            lyrics: None,
        }
    }

    /// The four replies a clean run consumes, in call order.
    fn happy_replies() -> Vec<Reply> {
        vec![
            Reply::Json(
                json!({ "path": "ignored", "local_check": { "checked": true, "runnable": true } }),
            ),
            Reply::Json(json!({ "applied": [], "warnings": [], "wrote": "ignored" })),
            Reply::Json(
                json!({ "valid": true, "errors": [], "warnings": [], "converted_from_ui": true, "converted_node_count": 30 }),
            ),
            Reply::Json(json!({ "prompt_id": "abc-123", "status": "queued", "outputs": [] })),
        ]
    }

    /// `set_slots` refuses a write whose address is not echoed back, so the
    /// canned reply has to applaud exactly what was sent.
    fn applied(overrides: &[SlotOverride]) -> Vec<String> {
        overrides.iter().map(|o| o.address.clone()).collect()
    }

    #[test]
    fn test_slot_overrides_send_bare_values_not_the_tag() {
        let mut resolved: ResolvedSlots = BTreeMap::new();
        resolved.insert(SlotAddress("109.value".into()), InputValue::Seed(u64::MAX));
        let sent = slot_overrides(&resolved);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].value, json!(u64::MAX));
        assert!(sent[0].value.get("type").is_none());
    }

    #[test]
    fn test_vacuous_validation_is_a_failure() {
        let vacuous: Validation = serde_json::from_value(json!({
            "valid": true,
            "errors": [],
            "warnings": [{ "code": "non_node_key" }]
        }))
        .expect("decodes");
        assert!(validation_error(&vacuous).is_some());
    }

    #[test]
    fn test_job_ids_do_not_collide_within_a_millisecond() {
        assert_ne!(mint_job_id(), mint_job_id());
    }

    #[tokio::test]
    async fn test_pipeline_calls_the_tools_in_order_on_one_working_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = dir.path().join("workflow.json");
        std::fs::write(&workflow, fixture()).expect("place the fixture");

        let profile = ace();
        let spec = spec();
        let overrides = slot_overrides(&profile.resolve_slots(&spec).expect("resolves"));
        let mut replies = happy_replies();
        replies[1] = Reply::Json(json!({
            "applied": applied(&overrides),
            "warnings": [],
            "wrote": workflow.display().to_string()
        }));

        let (comfy, calls) = client_and_log(replies).await;
        let submission = build_and_submit(&comfy, &workflow, &profile, &spec)
            .await
            .expect("pipeline runs");

        assert_eq!(submission.prompt_id, "abc-123");
        assert_eq!(submission.output_format.as_deref(), Some("flac"));

        // The edit has to be on disk, not only in the return value: what is
        // submitted is the file, and a pipeline that skipped the write-back
        // would report `flac` while ComfyUI rendered the template's MP3.
        let submitted: Value = serde_json::from_str(
            &std::fs::read_to_string(&workflow).expect("the working copy survives"),
        )
        .expect("submitted workflow decodes");
        let save = submitted
            .get("nodes")
            .and_then(|n| n.as_array())
            .expect("nodes")
            .iter()
            .find(|n| n.get("id").and_then(|i| i.as_i64()) == Some(107))
            .expect("the save node");
        assert_eq!(
            save.get("type").and_then(|t| t.as_str()),
            Some("SaveAudioAdvanced")
        );
        assert_eq!(
            save.pointer("/widgets_values/1").and_then(|f| f.as_str()),
            Some("flac")
        );

        let calls = calls.lock().expect("calls lock");
        let names: Vec<&str> = calls
            .iter()
            .map(|c| c.get("name").and_then(|n| n.as_str()).unwrap_or(""))
            .collect();
        assert_eq!(
            names,
            vec![
                "fetch_template",
                "set_workflow_slot",
                "validate_workflow",
                "run_workflow"
            ]
        );

        let want = workflow.display().to_string();
        for call in calls.iter() {
            let args = call.get("arguments").expect("arguments");
            let path = args
                .get("workflow_path")
                .or_else(|| args.get("out_path"))
                .and_then(|p| p.as_str())
                .expect("every call names the working copy");
            assert_eq!(path, want);
        }

        let seed = calls[1]
            .pointer("/arguments/overrides")
            .and_then(|o| o.as_array())
            .expect("overrides")
            .iter()
            .find(|o| o.get("address").and_then(|a| a.as_str()) == Some("109.value"))
            .expect("the seed is written");
        assert_eq!(seed.get("value"), Some(&json!(42)));
    }

    #[tokio::test]
    async fn test_an_inert_slot_stops_the_run_before_anything_is_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = dir.path().join("workflow.json");
        std::fs::write(&workflow, fixture()).expect("place the fixture");

        // The profile as it read before T-306a: a seed written to two inputs
        // that `PrimitiveInt` 109 drives.
        let mut profile = ace();
        profile.inputs.insert(
            "seed".to_string(),
            serde_json::from_value(json!({ "type": "seed", "slots": ["94.seed", "3.seed"] }))
                .expect("input decodes"),
        );

        let (comfy, calls) = client_and_log(happy_replies()).await;
        let error = build_and_submit(&comfy, &workflow, &profile, &spec())
            .await
            .expect_err("an inert write must not be submitted");

        assert!(error.contains("94.seed"), "{error}");
        let calls = calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 1, "nothing past fetch_template may be called");
    }

    #[test]
    fn test_a_bypassed_lora_is_not_spliced() {
        let mut graph: Value = serde_json::from_str(&fixture()).expect("fixture decodes");
        let profile = ace();
        let mut spec = spec();
        spec.loras = vec![LoraRef {
            file: "whatever.safetensors".into(),
            strength: 1.0,
            enabled: false,
        }];
        let edits = apply_graph_edits(&mut graph, &profile, &spec).expect("edits apply");
        assert!(edits.lora_nodes.is_empty());
    }

    /// Protects: an enabled LoRA reaches the file that is submitted.
    ///
    /// The counterpart to the bypass test, and the one that has to exist.
    /// "No LoRA was spliced" passes on a pipeline that splices nothing at all,
    /// and the reported `lora_nodes` is not evidence either -- splicing into a
    /// throwaway copy fills that list correctly while the submitted graph keeps
    /// none of it. That is MCP-SURFACE 17.1 one layer up: the run validates
    /// clean, completes, and writes a track with none of the user's LoRAs on
    /// it, and nothing anywhere says so. The assertion is therefore on the node
    /// in the file, read back after submission.
    #[tokio::test]
    async fn test_an_enabled_lora_reaches_the_submitted_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = dir.path().join("workflow.json");
        std::fs::write(&workflow, fixture()).expect("place the fixture");

        let profile = ace();
        let mut spec = spec();
        spec.loras = vec![LoraRef {
            file: "ambient_dream1\\adapter_model.safetensors".into(),
            strength: 0.8,
            enabled: true,
        }];

        let overrides = slot_overrides(&profile.resolve_slots(&spec).expect("resolves"));
        let mut replies = happy_replies();
        replies[1] = Reply::Json(json!({
            "applied": applied(&overrides),
            "warnings": [],
            "wrote": workflow.display().to_string()
        }));

        let (comfy, _calls) = client_and_log(replies).await;
        let submission = build_and_submit(&comfy, &workflow, &profile, &spec)
            .await
            .expect("pipeline runs");

        assert_eq!(submission.lora_nodes.len(), 1, "one loader was reported");

        let submitted: Value = serde_json::from_str(
            &std::fs::read_to_string(&workflow).expect("the working copy survives"),
        )
        .expect("submitted workflow decodes");
        let loader = submitted
            .get("nodes")
            .and_then(|n| n.as_array())
            .expect("nodes")
            .iter()
            .find(|n| {
                n.get("id").map(|i| i.to_string()).as_deref() == Some(&submission.lora_nodes[0])
            })
            .expect("the reported loader node is in the submitted file");

        assert_eq!(
            loader.get("type").and_then(|t| t.as_str()),
            Some("LoraLoaderModelOnly")
        );
        assert_eq!(
            loader.pointer("/widgets_values/0").and_then(|v| v.as_str()),
            Some("ambient_dream1\\adapter_model.safetensors"),
            "the name is passed through verbatim, separators included"
        );
        assert_eq!(
            loader.pointer("/widgets_values/1").and_then(|v| v.as_f64()),
            Some(0.8)
        );
    }
}
