//! Custom workflow import (ARCHITECTURE 5b): a ComfyUI **Workflow > Export
//! (API)** file becomes a user profile.
//!
//! Two commands, two steps. `import_workflow` checks the file, copies it into
//! the app's own `workflows/` directory and suggests which input carries which
//! role. `save_imported_profile` turns the user's accepted mapping into a
//! profile beside the shipped ones.
//!
//! **Why Export (API)** (owner decision 2026-11-11): it lands in Downloads
//! where people can find it, and it is the format ComfyUI executes -- so what
//! the user maps is exactly what runs, and every input's writability is a
//! property of the file itself (ARCHITECTURE 3a).
//!
//! **The workflow is copied, not referenced** (decisions log 2026-08-30): the
//! copy is the artifact of record, so a sidecar cannot be staled by the user
//! editing the original later.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use comfy_bridge::{ComfyError, ComfyHttp, NodeOptions, NodeSchema};
use create_core::audit::{as_link, node_ids};
use create_core::emit::{build_profile, resolve_model_files, Bounds, MappedSlot};
use create_core::readiness::ModelInventory;
use create_core::roles::{slots_of, suggest_roles, Role, RoleSuggestion};
use create_core::workflow::{detect_format, WorkflowFormat};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::ConfigDir;

/// Where imported workflows are kept, under the app data root.
const WORKFLOWS_DIR: &str = "workflows";

/// One writable input of the imported graph, for the mapping screen. The field
/// names are the wire shape the import view already reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slot {
    /// `node.input`.
    pub address: String,
    /// The input name.
    pub name: String,
    /// The registry's declared type (`STRING`, `INT`, `FLOAT`, `COMBO`, ...).
    #[serde(rename = "type")]
    pub ty: String,
    /// The literal the user's graph holds.
    pub current_value: Value,
    /// The node id.
    pub instance_id: String,
    /// The node class.
    pub node_type: String,
}

/// What an import produced.
#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub workflow_id: String,
    pub stored_path: String,
    /// Every writable input, in node order.
    pub slots: Vec<Slot>,
    pub suggestions: Vec<RoleSuggestion>,
    pub warnings: Vec<String>,
}

/// Check, copy and analyse one exported workflow.
#[tauri::command]
pub async fn import_workflow(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    source: String,
) -> Result<ImportReport, String> {
    let comfy = ensure_connected(&state, &config_dir)
        .await
        .map_err(EnsureError::into_message)?;
    import_into(&comfy, &config_dir.0, Path::new(&source)).await
}

/// The body of [`import_workflow`], on a client a test can script.
pub(crate) async fn import_into(
    comfy: &ComfyHttp,
    root: &Path,
    source: &Path,
) -> Result<ImportReport, String> {
    let text = std::fs::read_to_string(source)
        .map_err(|e| format!("Could not read {}: {e}", source.display()))?;
    let graph: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{} is not valid JSON ({e}). In ComfyUI choose Workflow > Export (API) and pick \
             the file it downloads.",
            source.display()
        )
    })?;
    match detect_format(&graph) {
        Some(WorkflowFormat::Api) => {}
        Some(WorkflowFormat::Frontend) => {
            return Err(format!(
                "{} is a saved workflow. latentCreate imports the API export instead: open it \
                 in ComfyUI, choose Workflow > Export (API), and import the file it downloads.",
                source.display()
            ))
        }
        None => {
            return Err(format!(
                "{} is not a ComfyUI workflow. In ComfyUI choose Workflow > Export (API) and \
                 import the file it downloads.",
                source.display()
            ))
        }
    }

    // Every class must exist in this ComfyUI; a missing custom node is named
    // now rather than discovered on the first Generate.
    let schemas = schemas_for(comfy, &graph).await?;
    let slots = writable_slots(&graph, &schemas);
    if slots.is_empty() {
        return Err(
            "This workflow has no adjustable values, so there is nothing to map. Check it in \
             ComfyUI, then export it again."
                .into(),
        );
    }
    let mut warnings = Vec::new();
    if !create_core::emit::has_audio_save_node(&graph) && !has_class(&graph, "SaveImage") {
        warnings.push(
            "This workflow has no Save Audio or Save Image node, so latentCreate would have \
             nothing to collect. Add one in ComfyUI and export again."
                .to_string(),
        );
    }

    let dir = root.join(WORKFLOWS_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let workflow_id = free_id(&dir, source)?;
    let stored = dir.join(format!("{workflow_id}.json"));
    // Written from the parsed graph, so the copy is exactly what was checked.
    let pretty = serde_json::to_string_pretty(&graph).map_err(|e| e.to_string())?;
    std::fs::write(&stored, pretty).map_err(|e| format!("{}: {e}", stored.display()))?;

    let types = type_lookup(&schemas);
    let suggestions = suggest_roles(&graph, &slots_of(&graph, &types));
    Ok(ImportReport {
        workflow_id,
        stored_path: stored.display().to_string(),
        slots,
        suggestions,
        warnings,
    })
}

/// The registry schema of every class the graph uses. An unknown class is a
/// refusal naming it; any other failure is passed on.
async fn schemas_for(
    comfy: &ComfyHttp,
    graph: &Value,
) -> Result<HashMap<String, NodeSchema>, String> {
    let classes: BTreeSet<String> = graph
        .as_object()
        .map(|nodes| {
            nodes
                .values()
                .filter_map(|n| n.get("class_type").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut schemas = HashMap::new();
    let mut missing = Vec::new();
    for class in classes {
        match comfy.object_info(&class).await {
            Ok(schema) => {
                schemas.insert(class, schema);
            }
            Err(ComfyError::UnknownNode(c)) => missing.push(c),
            Err(e) => return Err(e.to_string()),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "This workflow uses {}, which this ComfyUI does not have. Install the custom nodes \
             that provide them, restart ComfyUI, then import again.",
            missing.join(", ")
        ));
    }
    Ok(schemas)
}

/// `class, input -> declared type`, from the registry.
fn type_lookup(
    schemas: &HashMap<String, NodeSchema>,
) -> impl Fn(&str, &str) -> Option<String> + '_ {
    move |class: &str, input: &str| {
        schemas
            .get(class)
            .and_then(|s| s.input(input))
            .map(|i| i.ty.clone())
    }
}

/// Every literal input, typed by the registry (falling back to the literal's
/// JSON shape for an input the registry does not declare, such as a dynamic
/// combo's dotted sub-input).
fn writable_slots(graph: &Value, schemas: &HashMap<String, NodeSchema>) -> Vec<Slot> {
    let types = type_lookup(schemas);
    let mut out = Vec::new();
    for id in node_ids(graph) {
        let node = &graph[&id];
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or("");
        let Some(inputs) = node.get("inputs").and_then(Value::as_object) else {
            continue;
        };
        for (name, value) in inputs {
            if as_link(value).is_some() {
                continue;
            }
            let ty = types(class, name).unwrap_or_else(|| match value {
                Value::String(_) => "STRING".into(),
                Value::Bool(_) => "BOOLEAN".into(),
                Value::Number(n) if n.is_f64() => "FLOAT".into(),
                Value::Number(_) => "INT".into(),
                _ => "OTHER".into(),
            });
            out.push(Slot {
                address: format!("{id}.{name}"),
                name: name.clone(),
                ty,
                current_value: value.clone(),
                instance_id: id.clone(),
                node_type: class.to_string(),
            });
        }
    }
    out
}

fn has_class(graph: &Value, class: &str) -> bool {
    graph.as_object().is_some_and(|nodes| {
        nodes
            .values()
            .any(|n| n.get("class_type").and_then(Value::as_str) == Some(class))
    })
}

/// One role the user accepted, and the slots it maps to.
#[derive(Debug, Clone, Deserialize)]
pub struct RoleMapping {
    pub role: Role,
    pub addresses: Vec<String>,
}

/// Where the emitted profile went.
#[derive(Debug, Clone, Serialize)]
pub struct SavedProfile {
    pub profile_id: String,
    pub path: String,
}

/// Turn an accepted mapping into a user profile.
#[tauri::command]
pub async fn save_imported_profile(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    workflow_id: String,
    display_name: String,
    mappings: Vec<RoleMapping>,
) -> Result<SavedProfile, String> {
    let comfy = ensure_connected(&state, &config_dir)
        .await
        .map_err(EnsureError::into_message)?;
    emit_profile(
        &comfy,
        &config_dir.0,
        &workflow_id,
        &display_name,
        &mappings,
    )
    .await
}

/// The body of [`save_imported_profile`].
pub(crate) async fn emit_profile(
    comfy: &ComfyHttp,
    root: &Path,
    workflow_id: &str,
    display_name: &str,
    mappings: &[RoleMapping],
) -> Result<SavedProfile, String> {
    let stored = root.join(WORKFLOWS_DIR).join(format!("{workflow_id}.json"));
    if !stored.exists() {
        return Err(format!(
            "No imported workflow named {workflow_id}. Import it again."
        ));
    }
    let graph: Value = serde_json::from_str(
        &std::fs::read_to_string(&stored).map_err(|e| format!("{}: {e}", stored.display()))?,
    )
    .map_err(|e| format!("{}: {e}", stored.display()))?;
    let schemas = schemas_for(comfy, &graph).await?;
    let slots = writable_slots(&graph, &schemas);

    let mut resolved = Vec::new();
    for mapping in mappings {
        let mut mapped = Vec::new();
        for address in &mapping.addresses {
            let slot = slots
                .iter()
                .find(|s| &s.address == address)
                .ok_or_else(|| {
                    format!(
                        "{address} is not an adjustable value in this workflow. Import it again."
                    )
                })?;
            let bounds = schemas
                .get(&slot.node_type)
                .and_then(|s| s.input(&slot.name))
                .and_then(|i| bounds_of(&i.options));
            mapped.push(MappedSlot {
                address: slot.address.clone(),
                widget_type: slot.ty.clone(),
                current_value: slot.current_value.clone(),
                bounds,
            });
        }
        resolved.push((mapping.role, mapped));
    }

    let profile_id = free_profile_id(root, display_name)?;
    let mut profile = build_profile(
        &profile_id,
        display_name,
        &graph,
        &stored.display().to_string(),
        &resolved,
    )
    .map_err(|e| e.to_string())?;

    // Loader inputs are combos holding filenames; a value found in a model
    // folder becomes a declared file, so readiness works for imports too.
    let candidates: Vec<String> = slots
        .iter()
        .filter(|s| s.ty == "COMBO")
        .filter_map(|s| s.current_value.as_str().map(str::to_string))
        .collect();
    let inventory = full_inventory(comfy).await;
    profile.comfy.models = resolve_model_files(&candidates, &inventory);

    let dir = root.join("profiles");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{profile_id}.json"));
    let text = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(SavedProfile {
        profile_id,
        path: path.display().to_string(),
    })
}

/// Every folder's files. A failure leaves an empty inventory, so the profile
/// declares no files and reads "cannot check" rather than ready.
async fn full_inventory(comfy: &ComfyHttp) -> ModelInventory {
    let Ok(folders) = comfy.model_folders().await else {
        return ModelInventory::default();
    };
    let mut listed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for folder in folders {
        if let Ok(files) = comfy.models_in(&folder).await {
            listed.insert(folder, files.into_iter().collect());
        }
    }
    ModelInventory::new(listed)
}

/// The registry's numeric bounds, when it reports both ends.
fn bounds_of(options: &NodeOptions) -> Option<Bounds> {
    Some(Bounds {
        min: options.min.as_ref().and_then(Value::as_f64)?,
        max: options.max.as_ref().and_then(Value::as_f64)?,
        step: options.step.as_ref().and_then(Value::as_f64),
    })
}

/// A profile id that does not overwrite one the user already has.
fn free_profile_id(root: &Path, display_name: &str) -> Result<String, String> {
    let dir = root.join("profiles");
    let base = library::projects::slugify(display_name);
    for n in 1..1000u32 {
        let candidate = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        if !dir.join(format!("{candidate}.json")).exists() {
            return Ok(candidate);
        }
    }
    Err(format!("too many profiles named {base}"))
}

/// A workflow id from the source file name that does not overwrite an earlier
/// import of a file with the same name.
fn free_id(dir: &Path, source: &Path) -> Result<String, String> {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("workflow");
    let base = library::projects::slugify(stem);
    for n in 1..1000u32 {
        let candidate = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        if !dir.join(format!("{candidate}.json")).exists() {
            return Ok(candidate);
        }
    }
    Err(format!("too many workflows named {base}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use comfy_bridge::transport::{FakeTransport, Method};
    use serde_json::json;
    use std::sync::Arc;

    fn repo() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn shipped_workflow(id: &str) -> String {
        std::fs::read_to_string(repo().join("profiles/workflows").join(format!("{id}.json")))
            .expect("workflow")
    }

    /// A ComfyUI that knows every class, with a minimal schema each, plus the
    /// real KSampler/Lora/Empty-latent schemas from the captured subset.
    fn registry_for(graph: &Value) -> Arc<FakeTransport> {
        let fake = Arc::new(FakeTransport::new());
        let subset: Value = serde_json::from_str(
            &std::fs::read_to_string(repo().join("testdata/comfy-http/object_info_subset.json"))
                .expect("f"),
        )
        .expect("j");
        let classes: BTreeSet<&str> = graph
            .as_object()
            .into_iter()
            .flat_map(|o| o.values())
            .filter_map(|n| n["class_type"].as_str())
            .collect();
        for class in classes {
            let entry = match class {
                // The loader's filename is a combo -- what makes it a model file.
                "CheckpointLoaderSimple" => json!({"input": {"required": {
                    "ckpt_name": [["flux1-schnell-fp8.safetensors"]]
                }}}),
                _ => subset
                    .get(class)
                    .cloned()
                    .unwrap_or_else(|| json!({"input": {"required": {}}})),
            };
            fake.on_json(
                Method::Get,
                &format!("/object_info/{}", comfy_bridge::encode_segment(class)),
                &json!({ class: entry }),
            );
        }
        fake.on_json(Method::Get, "/models", &json!(["checkpoints"]));
        fake.on_json(
            Method::Get,
            "/models/checkpoints",
            &json!(["flux1-schnell-fp8.safetensors"]),
        );
        fake
    }

    fn place(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).expect("place the source");
        p
    }

    async fn import(text: &str, name: &str) -> (Result<ImportReport, String>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(tmp.path(), name, text);
        let graph: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let fake = registry_for(&graph);
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake, None);
        (import_into(&comfy, tmp.path(), &source).await, tmp)
    }

    /// Protects: an Export (API) file is stored as checked and offered with its
    /// writable inputs and role suggestions -- polarity included, so the
    /// negative encoder is never offered as the prompt.
    #[tokio::test]
    async fn test_an_api_export_imports_with_slots_and_suggestions() {
        let (report, tmp) = import(&shipped_workflow("flux-1-schnell-fp8"), "My Flux.json").await;
        let report = report.expect("imports");
        assert_eq!(report.workflow_id, "my-flux");
        let stored: Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join("workflows/my-flux.json")).expect("stored"),
        )
        .expect("json");
        assert_eq!(
            stored,
            serde_json::from_str::<Value>(&shipped_workflow("flux-1-schnell-fp8")).expect("j")
        );
        assert!(report
            .slots
            .iter()
            .any(|s| s.address == "31.seed" && s.ty == "INT"));
        assert!(
            !report.slots.iter().any(|s| s.address == "31.model"),
            "links are not slots"
        );
        let tags = report
            .suggestions
            .iter()
            .find(|s| s.role == Role::Tags)
            .expect("tags");
        assert!(tags.candidates.iter().any(|c| c.address == "6.text"));
        assert!(!tags.candidates.iter().any(|c| c.address == "33.text"));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// Protects: a saved (frontend) workflow is refused with the menu path, and
    /// nothing is stored.
    #[tokio::test]
    async fn test_a_saved_workflow_is_refused_with_the_export_menu_item() {
        let (report, tmp) = import(r#"{"nodes": [], "links": []}"#, "saved.json").await;
        let err = report.expect_err("refused");
        assert!(err.contains("Workflow > Export (API)"), "{err}");
        assert!(!tmp.path().join(WORKFLOWS_DIR).exists());
    }

    #[tokio::test]
    async fn test_something_that_is_not_a_workflow_says_what_to_export() {
        let (report, _tmp) = import(r#"{"hello": 1}"#, "x.json").await;
        assert!(report
            .expect_err("refused")
            .contains("is not a ComfyUI workflow"));
        let (report, _tmp) = import("not json", "y.json").await;
        assert!(report.expect_err("refused").contains("is not valid JSON"));
    }

    /// Protects: a workflow needing a custom node this ComfyUI lacks is refused
    /// naming the class -- at import, not on the first Generate.
    #[tokio::test]
    async fn test_a_missing_custom_node_is_named_at_import() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let text = r#"{"1": {"class_type": "FancyCustomNode", "inputs": {"x": 1}}}"#;
        let source = place(tmp.path(), "custom.json", text);
        let fake = Arc::new(FakeTransport::new());
        fake.on(Method::Get, "/object_info/FancyCustomNode", 200, "{}");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake, None);
        let err = import_into(&comfy, tmp.path(), &source)
            .await
            .expect_err("refused");
        assert!(
            err.contains("FancyCustomNode") && err.contains("Install the custom nodes"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_a_second_import_of_the_same_filename_does_not_overwrite_the_first() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let text = shipped_workflow("flux-1-schnell-fp8");
        let source = place(tmp.path(), "song.json", &text);
        let graph: Value = serde_json::from_str(&text).expect("j");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", registry_for(&graph), None);
        let first = import_into(&comfy, tmp.path(), &source)
            .await
            .expect("first");
        let second = import_into(&comfy, tmp.path(), &source)
            .await
            .expect("second");
        assert_eq!(first.workflow_id, "song");
        assert_eq!(second.workflow_id, "song-2");
    }

    /// Protects: an emitted profile loads through the real loader, carries the
    /// registry's bounds, and declares the loader's model file -- so an
    /// imported model reads Ready on the Models step like a shipped one.
    #[tokio::test]
    async fn test_an_emitted_profile_loads_through_the_real_loader() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let text = shipped_workflow("flux-1-schnell-fp8");
        let source = place(tmp.path(), "flux.json", &text);
        let graph: Value = serde_json::from_str(&text).expect("j");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", registry_for(&graph), None);
        import_into(&comfy, tmp.path(), &source)
            .await
            .expect("imports");
        let saved = emit_profile(
            &comfy,
            tmp.path(),
            "flux",
            "My Flux",
            &[
                RoleMapping {
                    role: Role::Tags,
                    addresses: vec!["6.text".into()],
                },
                RoleMapping {
                    role: Role::Seed,
                    addresses: vec!["31.seed".into()],
                },
                RoleMapping {
                    role: Role::Steps,
                    addresses: vec!["31.steps".into()],
                },
            ],
        )
        .await
        .expect("emits");
        assert_eq!(saved.profile_id, "my-flux");

        let set = library::profiles::load(&repo().join("profiles"), &tmp.path().join("profiles"));
        let loaded = set
            .profiles
            .get("my-flux")
            .expect("loads like a shipped profile");
        assert_eq!(loaded.profile.kind, create_core::profile::ModelKind::Image);
        assert_eq!(loaded.profile.comfy.models.len(), 1);
        assert_eq!(
            loaded.profile.comfy.models[0].file,
            "flux1-schnell-fp8.safetensors"
        );
    }

    #[tokio::test]
    async fn test_mapping_a_link_or_unknown_address_is_refused() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let text = shipped_workflow("flux-1-schnell-fp8");
        let source = place(tmp.path(), "flux.json", &text);
        let graph: Value = serde_json::from_str(&text).expect("j");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", registry_for(&graph), None);
        import_into(&comfy, tmp.path(), &source)
            .await
            .expect("imports");
        let err = emit_profile(
            &comfy,
            tmp.path(),
            "flux",
            "X",
            &[RoleMapping {
                role: Role::Tags,
                addresses: vec!["31.positive".into()],
            }],
        )
        .await
        .expect_err("a link is not writable");
        assert!(err.contains("31.positive"), "{err}");
        let err = emit_profile(&comfy, tmp.path(), "nope", "X", &[])
            .await
            .expect_err("unknown");
        assert!(err.contains("Import it again"), "{err}");
    }
}
