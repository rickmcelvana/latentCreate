//! Custom workflow import (ARCHITECTURE 5b).
//!
//! Takes a path the user picked, decides whether this app can drive it, keeps a
//! **copy**, and reports everything the mapping screen needs.
//!
//! **The copy is the point, and it is an owner decision** (PROJECT.md decisions
//! log, 2026-08-30): a profile pointing at a live file in the user's ComfyUI
//! folder would silently change behaviour when they edited it there, and every
//! provenance sidecar written before that edit would quietly become a lie. A
//! sidecar records the *inputs*; reproducing a track means the graph those
//! inputs were resolved against must still be the same graph.
//!
//! Two things follow, and they are why this module is not four lines:
//!
//! - The stored copy is the artifact of record, so what gets **validated** is
//!   the copy, never the file the user picked.
//! - A refused import leaves **nothing** behind, so the copy is staged under a
//!   dot-name and only renamed into place once it has passed.

use std::path::Path;

use create_core::workflow::{detect_format, WorkflowFormat};
use mcp_bridge::{Finding, LocalComfy, Slot, Verdict};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::ConfigDir;

/// Directory holding imported workflows, under the app config dir.
const WORKFLOWS_DIR: &str = "workflows";

/// What an import produced: where it was stored, and what it exposes.
#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    /// Stable id for the stored copy; the filename stem under `workflows/`.
    pub workflow_id: String,
    /// Absolute path of the stored copy -- what a profile's `comfy.workflow`
    /// will point at.
    pub stored_path: String,
    /// Every slot the graph exposes, each already carrying its node class and
    /// widget type (MCP-SURFACE 29.5). This is what T-313c ranks into role
    /// suggestions.
    pub slots: Vec<Slot>,
    /// Advisory findings from validation.
    ///
    /// **Never a reason to refuse.** The executed MiniMax graph from the T-315
    /// run -- which produced a playable FLAC -- carries three of these, all
    /// `COMFY_MATCHTYPE_V3` noise (MCP-SURFACE 29.3). Blocking on warnings
    /// would reject this project's own reference model.
    pub warnings: Vec<String>,
}

/// Import the workflow at `source`, storing a copy and reporting its slots.
#[tauri::command]
pub async fn import_workflow(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    source: String,
) -> Result<ImportReport, String> {
    let comfy = match ensure_connected(&state, &config_dir, None).await {
        Ok(comfy) => comfy,
        Err(EnsureError::Comfy(e)) => return Err(e.to_string()),
        Err(EnsureError::Log(detail)) => return Err(detail),
    };
    import_into(&comfy, &config_dir.0, Path::new(&source)).await
}

/// The body of [`import_workflow`], taking its inputs directly so a test can
/// drive it with a mock transport and a temp directory.
pub(crate) async fn import_into(
    comfy: &LocalComfy,
    root: &Path,
    source: &Path,
) -> Result<ImportReport, String> {
    // 1. Read and parse what the user picked. Both messages name *their* file:
    //    an internal path tells someone who chose the wrong thing nothing they
    //    can act on -- the defect T-313a's review caught, which must not come
    //    back through a new door.
    let text = std::fs::read_to_string(source)
        .map_err(|e| format!("Could not read {}: {e}", source.display()))?;
    let graph: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{} is not valid JSON ({e}). Pick a workflow exported from ComfyUI.",
            source.display()
        )
    })?;

    // 2. Decide the shape before anything is written. `validate_workflow`
    //    cannot do this job -- it accepts an API export and calls it valid
    //    (MCP-SURFACE 29.1).
    match detect_format(&graph) {
        Some(WorkflowFormat::Frontend) => {}
        Some(WorkflowFormat::Api) => {
            return Err(format!(
                "{} is a File > Export (API) workflow, which cannot be edited. \
                 In ComfyUI use File > Save (As) to export the editing format, \
                 then import that.",
                source.display()
            ))
        }
        None => {
            return Err(format!(
                "{} is not a ComfyUI workflow. Export one from ComfyUI with File > Save (As).",
                source.display()
            ))
        }
    }

    // 3. Stage the copy. Everything from here validates and reads *this* file,
    //    so the report describes the bytes that were kept.
    let dir = root.join(WORKFLOWS_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let workflow_id = free_id(&dir, source)?;
    let staged = dir.join(format!(".staging-{workflow_id}.json"));
    let stored = dir.join(format!("{workflow_id}.json"));
    std::fs::copy(source, &staged).map_err(|e| format!("{}: {e}", staged.display()))?;

    let (slots, warnings) = match inspect(comfy, &staged).await {
        Ok(inspected) => inspected,
        Err(e) => {
            // A refused import leaves nothing behind.
            let _ = std::fs::remove_file(&staged);
            return Err(e);
        }
    };

    std::fs::rename(&staged, &stored).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        format!("{}: {e}", stored.display())
    })?;

    Ok(ImportReport {
        workflow_id,
        stored_path: stored.display().to_string(),
        slots,
        warnings,
    })
}

/// Validate the staged copy and read its slots.
///
/// Split out so every failure between staging and committing funnels through
/// one `Err`, and therefore through one cleanup.
async fn inspect(comfy: &LocalComfy, staged: &Path) -> Result<(Vec<Slot>, Vec<String>), String> {
    let report = comfy.validate(staged).await.map_err(|e| e.to_string())?;
    match report.verdict() {
        Verdict::Valid => {}
        Verdict::Invalid => {
            return Err(format!(
                "ComfyUI rejected this workflow: {}",
                summarise_findings(&report.errors)
            ))
        }
        Verdict::Vacuous => {
            return Err(
                "validation examined no nodes, so it proves nothing about this workflow".into(),
            )
        }
    }

    // Warnings are collected, never gated on (MCP-SURFACE 29.3).
    let warnings = report
        .warnings
        .iter()
        .filter_map(|f| f.message.clone())
        .collect();

    let slots = comfy.list_slots(staged).await.map_err(|e| e.to_string())?;
    if slots.slots.is_empty() {
        return Err(
            "This workflow exposes no adjustable parameters, so there is nothing to map. \
             Check it has the widgets you expect in ComfyUI, then export it again."
                .into(),
        );
    }
    Ok((slots.slots, warnings))
}

/// One line per finding, node id included so the user can be pointed at it.
fn summarise_findings(findings: &[Finding]) -> String {
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

/// First unused id for `source`'s filename in `dir`.
///
/// Reuses `library::projects::slugify` rather than growing a second one, and
/// suffixes a taken name the way `free_slug` does for projects, for the same
/// reason: two people may import `song.json`, and silently overwriting the
/// earlier import is worse than a second file.
fn free_id(dir: &Path, source: &Path) -> Result<String, String> {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("workflow");
    // No safety check on the result, deliberately: `slugify` guarantees a safe
    // slug by construction, and its own test pins that with
    // `is_safe_slug(&slugify("../../etc/passwd"))`. Re-checking here would mean
    // making that private helper public to assert something its sibling already
    // promises.
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
    use mcp_bridge::mock::Reply;
    use mcp_bridge::test_helpers::client_and_log;
    use serde_json::json;

    fn workflows(root: &Path) -> Vec<String> {
        let dir = root.join(WORKFLOWS_DIR);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    fn named_fixture(name: &str) -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../testdata/workflows")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    /// A clean inspect: validate, then slots.
    fn ok_replies() -> Vec<Reply> {
        vec![
            Reply::Json(json!({
                "valid": true, "errors": [], "warnings": [],
                "converted_from_ui": true, "converted_node_count": 11
            })),
            Reply::Json(json!({
                "workflow": "staged", "count": 1,
                "slots": [{
                    "address": "94.tags", "name": "tags", "type": "STRING",
                    "current_value": "synthwave", "instance_id": "94",
                    "node_type": "TextEncodeAceStepAudio1.5"
                }]
            })),
        ]
    }

    fn place(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).expect("place the source");
        p
    }

    /// Protects: the stored file is the file that was validated.
    ///
    /// Read back from disk rather than trusting the report -- the copy is the
    /// artifact of record (decisions log, 2026-08-30), and a report describing
    /// bytes we did not keep is the failure the staging order exists to
    /// prevent.
    #[tokio::test]
    async fn test_a_valid_import_stores_the_bytes_it_validated() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(
            tmp.path(),
            "my song.json",
            &named_fixture("ace_step_1_5_xl_turbo.json"),
        );

        let (comfy, calls) = client_and_log(ok_replies()).await;
        let report = import_into(&comfy, tmp.path(), &source)
            .await
            .expect("a frontend workflow imports");

        // **What was inspected is what was kept.** Every ComfyUI call names the
        // staged copy, never the path the user picked. Without this the whole
        // staging design is unasserted: swapping `&staged` for `source` passes
        // every other test in this module, because a copy compares equal to its
        // own source (measured, not assumed -- the mutation was run).
        let seen = calls.lock().expect("calls");
        assert_eq!(seen.len(), 2, "validate then slots");
        for call in seen.iter() {
            let path = call
                .pointer("/arguments/workflow_path")
                .and_then(|p| p.as_str())
                .expect("every call names a workflow");
            let name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            assert!(
                name.starts_with(".staging-"),
                "inspected {name}, which is not the staged copy"
            );
        }
        drop(seen);

        assert_eq!(report.workflow_id, "my-song");
        assert_eq!(workflows(tmp.path()), vec!["my-song.json".to_string()]);
        assert_eq!(
            std::fs::read_to_string(&report.stored_path).expect("stored file"),
            std::fs::read_to_string(&source).expect("source file"),
            "the stored copy is byte-for-byte what was picked"
        );
        assert_eq!(report.slots.len(), 1);
        assert_eq!(report.slots[0].node_type, "TextEncodeAceStepAudio1.5");
    }

    /// Protects: an API export is refused locally, before ComfyUI is asked
    /// anything and before a byte is written.
    ///
    /// `validate_workflow` would have called this file valid (MCP-SURFACE
    /// 29.1), so the format check happens first or not at all.
    #[tokio::test]
    async fn test_an_api_export_is_refused_before_anything_is_stored() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(
            tmp.path(),
            "exported.json",
            &named_fixture("minimax_music3.api-format.json"),
        );

        let (comfy, calls) = client_and_log(ok_replies()).await;
        let err = import_into(&comfy, tmp.path(), &source)
            .await
            .expect_err("an API export cannot be driven");

        assert!(err.contains("File > Save (As)"), "{err}");
        assert!(
            calls.lock().expect("calls").is_empty(),
            "ComfyUI is not asked"
        );
        assert!(workflows(tmp.path()).is_empty(), "nothing is stored");
    }

    /// Protects: a refused import leaves nothing behind -- staging included.
    ///
    /// The one that would rot silently: a leftover `.staging-*.json` is
    /// invisible until the directory is listed, and the next import of the same
    /// name would collide with it.
    #[tokio::test]
    async fn test_an_invalid_workflow_leaves_nothing_behind() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(
            tmp.path(),
            "broken.json",
            &named_fixture("ace_step_1_5_xl_turbo.json"),
        );

        let replies = vec![Reply::Json(json!({
            "valid": false,
            "errors": [{ "node_id": "104", "message": "not in 2 known options" }],
            "warnings": []
        }))];
        let (comfy, _calls) = client_and_log(replies).await;
        let err = import_into(&comfy, tmp.path(), &source)
            .await
            .expect_err("ComfyUI rejected it");

        assert!(err.contains("node 104"), "{err}");
        assert!(
            workflows(tmp.path()).is_empty(),
            "no file and no staging file: {:?}",
            workflows(tmp.path())
        );
    }

    /// Protects: MCP-SURFACE 29.3 -- warnings never refuse an import.
    ///
    /// This warning comes off the executed MiniMax graph from the T-315 run,
    /// which produced a playable FLAC. Gating on it would reject this project's
    /// own reference model.
    #[tokio::test]
    async fn test_warnings_do_not_refuse_an_import() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(
            tmp.path(),
            "warny.json",
            &named_fixture("ace_step_1_5_xl_turbo.json"),
        );

        let mut replies = ok_replies();
        replies[0] = Reply::Json(json!({
            "valid": true, "errors": [],
            "warnings": [{
                "node_id": "35", "code": "edge_type_mismatch",
                "message": "input audio expects AUDIO but ComfySwitchNode produces COMFY_MATCHTYPE_V3"
            }],
            "converted_from_ui": true, "converted_node_count": 11
        }));

        let (comfy, _calls) = client_and_log(replies).await;
        let report = import_into(&comfy, tmp.path(), &source)
            .await
            .expect("warnings are advisory");

        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("COMFY_MATCHTYPE_V3"));
        assert_eq!(workflows(tmp.path()), vec!["warny.json".to_string()]);
    }

    /// Protects: a graph with nothing to map is refused rather than stored.
    /// A profile built from it would have no controls at all.
    #[tokio::test]
    async fn test_a_graph_with_no_slots_is_refused() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(
            tmp.path(),
            "bare.json",
            &named_fixture("ace_step_1_5_xl_turbo.json"),
        );

        let mut replies = ok_replies();
        replies[1] = Reply::Json(json!({ "workflow": "staged", "count": 0, "slots": [] }));

        let (comfy, _calls) = client_and_log(replies).await;
        let err = import_into(&comfy, tmp.path(), &source)
            .await
            .expect_err("nothing to map");

        assert!(err.contains("no adjustable parameters"), "{err}");
        assert!(workflows(tmp.path()).is_empty());
    }

    /// Protects: two imports sharing a filename produce two workflows.
    ///
    /// Same rule `free_slug` uses for projects, for the same reason: silently
    /// overwriting someone's earlier import is worse than a second file.
    #[tokio::test]
    async fn test_a_second_import_of_the_same_filename_does_not_overwrite_the_first() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let body = named_fixture("ace_step_1_5_xl_turbo.json");
        let first_dir = tmp.path().join("a");
        let second_dir = tmp.path().join("b");
        std::fs::create_dir_all(&first_dir).expect("mkdir");
        std::fs::create_dir_all(&second_dir).expect("mkdir");
        let first = place(&first_dir, "song.json", &body);
        let second = place(&second_dir, "song.json", &body);

        let (comfy, _calls) = client_and_log(ok_replies()).await;
        let one = import_into(&comfy, tmp.path(), &first)
            .await
            .expect("first");
        let (comfy, _calls) = client_and_log(ok_replies()).await;
        let two = import_into(&comfy, tmp.path(), &second)
            .await
            .expect("second");

        assert_eq!(one.workflow_id, "song");
        assert_eq!(two.workflow_id, "song-2");
        assert_eq!(
            workflows(tmp.path()),
            vec!["song-2.json".to_string(), "song.json".to_string()]
        );
    }

    /// Protects: a file that is not a workflow at all is told so, rather than
    /// being told to re-export it from ComfyUI. Three shapes, three messages.
    #[tokio::test]
    async fn test_something_that_is_not_a_workflow_is_not_told_to_re_export() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(tmp.path(), "notes.json", "{\"hello\":\"world\"}");

        let (comfy, _calls) = client_and_log(ok_replies()).await;
        let err = import_into(&comfy, tmp.path(), &source)
            .await
            .expect_err("not a workflow");

        assert!(err.contains("is not a ComfyUI workflow"), "{err}");
        assert!(workflows(tmp.path()).is_empty());
    }

    /// Protects: the message names the user's file, not an internal path --
    /// the defect T-313a's review caught, arriving through a new door.
    #[tokio::test]
    async fn test_a_file_that_is_not_json_names_the_users_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = place(tmp.path(), "cover-art.png", "not json at all");

        let (comfy, _calls) = client_and_log(ok_replies()).await;
        let err = import_into(&comfy, tmp.path(), &source)
            .await
            .expect_err("a PNG is not a workflow");

        assert!(err.contains("cover-art.png"), "{err}");
        assert!(err.contains("not valid JSON"), "{err}");
    }
}
