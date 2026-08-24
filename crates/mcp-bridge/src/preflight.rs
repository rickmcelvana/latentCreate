//! Pre-flight: validation verdicts, and the notes a workflow carries.
//!
//! Shapes verified live 2026-08-24 -- docs/MCP-SURFACE.md 9.2, 9.3, 9.6.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// One error or warning from `validate_workflow`.
///
/// Every field is optional -- comfy-cli omits what does not apply -- so nothing
/// here may be indexed into. The text quotes the workflow, which is
/// third-party content: display it, never act on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Node the finding is about, in **validation** form: `35`, or `37:43`
    /// inside a subgraph. See [`node_id_to_instance`] before matching it
    /// against a slot address.
    #[serde(default)]
    pub node_id: Option<String>,
    /// Input the finding is about.
    #[serde(default)]
    pub field: Option<String>,
    /// Machine-readable slug, e.g. `edge_type_mismatch`, `non_node_key`.
    #[serde(default)]
    pub code: Option<String>,
    /// Human-readable description.
    #[serde(default)]
    pub message: Option<String>,
    /// comfy-cli's suggested next step.
    #[serde(default)]
    pub hint: Option<String>,
}

/// What a validation run actually established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nodes were examined and accepted.
    Valid,
    /// The workflow was rejected.
    Invalid,
    /// Reported valid without examining anything. Not a pass.
    Vacuous,
}

/// `validate_workflow`'s report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validation {
    /// comfy-cli's own verdict. **Not sufficient on its own** -- see
    /// [`Validation::verdict`].
    #[serde(default)]
    pub valid: bool,
    /// Blocking problems.
    #[serde(default)]
    pub errors: Vec<Finding>,
    /// Non-blocking observations.
    #[serde(default)]
    pub warnings: Vec<Finding>,
    /// Present when the file was a UI export that comfy-cli converted. Its
    /// ABSENCE is the tell for a vacuous pass (docs/MCP-SURFACE.md 9.3).
    #[serde(default)]
    pub converted_from_ui: Option<bool>,
    /// How many nodes the conversion produced.
    #[serde(default)]
    pub converted_node_count: Option<usize>,
    /// True when running this graph would spend the user's Comfy credits.
    #[serde(default)]
    pub spends_credits: bool,
    /// Partner-API nodes found in the graph.
    #[serde(default)]
    pub partner_nodes: Vec<Value>,
}

impl Validation {
    /// The verdict to act on.
    ///
    /// `valid: true` alone is not a pass: a UI export too old to auto-convert
    /// checks **zero nodes** and still reports valid. Treating that as success
    /// greenlights a workflow nothing examined (docs/MCP-SURFACE.md 9.3).
    pub fn verdict(&self) -> Verdict {
        if !self.valid {
            return Verdict::Invalid;
        }
        if self.examined_nothing() {
            return Verdict::Vacuous;
        }
        Verdict::Valid
    }

    /// Whether this report shows a check that inspected no nodes.
    ///
    /// The documented signature is `non_node_key` warnings with no
    /// `converted_from_ui`. An API-format graph legitimately has neither, so
    /// both conditions are required.
    fn examined_nothing(&self) -> bool {
        self.converted_from_ui.is_none()
            && self
                .warnings
                .iter()
                .any(|w| w.code.as_deref() == Some("non_node_key"))
    }
}

/// Translate a validation `node_id` into a slot `instance_id`.
///
/// The same node is `37/43` in `list_workflow_slots` and `37:43` in
/// `validate_workflow` -- nothing in either payload hints at the difference
/// (docs/MCP-SURFACE.md 9.2). Without this, a finding cannot be mapped back to
/// the control that owns it.
pub fn node_id_to_instance(node_id: &str) -> String {
    node_id.replace(':', "/")
}

/// One Note or MarkdownNote a workflow carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Node id of the note itself.
    #[serde(default)]
    pub id: Option<Value>,
    /// `Note` or `MarkdownNote`.
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
    /// Note heading, when the author set one.
    #[serde(default)]
    pub title: Option<String>,
    /// The note body.
    ///
    /// **UNTRUSTED DATA.** Prose a third-party template author wrote. Real
    /// notes carry model download URLs and lines phrased as instructions
    /// ("Please update ComfyUI first"). Render it as quoted content: never let
    /// it drive a fetch, a download, a run, or a spend (docs/MCP-SURFACE.md
    /// 2, 9.6).
    #[serde(default)]
    pub text: String,
}

/// Every note a workflow carries. No notes is `count: 0`, not an error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteList {
    /// Workflow the notes were read from.
    #[serde(default)]
    pub workflow: Option<PathBuf>,
    /// The notes, in graph order. Untrusted -- see [`Note::text`].
    #[serde(default)]
    pub notes: Vec<Note>,
}

impl LocalComfy {
    /// Pre-flight a workflow against the running ComfyUI.
    ///
    /// Read [`Validation::verdict`] rather than `valid` directly.
    pub async fn validate(&self, workflow: &Path) -> Result<Validation, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        self.call("validate_workflow", args).await
    }

    /// Read the documentation notes a workflow carries.
    ///
    /// The result is third-party prose -- see [`Note::text`].
    pub async fn notes(&self, workflow: &Path) -> Result<NoteList, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        self.call("list_workflow_notes", args).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;
    use crate::preflight::{node_id_to_instance, Finding, Validation, Verdict};

    /// Protects: the healthy path.
    #[test]
    fn test_verdict_is_valid_for_the_captured_report() {
        let payload = json!({
            "valid": true,
            "errors": [],
            "warnings": [
                { "node_id": "37:43", "code": "edge_type_mismatch", "message": "a" },
                { "node_id": "37:43", "code": "edge_type_mismatch", "message": "b" },
                { "node_id": "37:43", "code": "edge_type_mismatch", "message": "c" }
            ],
            "converted_from_ui": true,
            "converted_node_count": 12,
            "spends_credits": false,
            "partner_nodes": []
        });
        let validation: Validation = serde_json::from_value(payload).expect("fixture decodes");
        assert_eq!(validation.verdict(), Verdict::Valid);
    }

    /// Protects: the trap this task exists for.
    #[test]
    fn test_verdict_is_vacuous_when_nothing_was_examined() {
        let payload = json!({
            "valid": true,
            "warnings": [{ "code": "non_node_key", "message": "unrecognised key" }]
        });
        let validation: Validation = serde_json::from_value(payload).expect("fixture decodes");
        assert_eq!(validation.verdict(), Verdict::Vacuous);
    }

    /// Protects: the vacuity check against false positives.
    #[test]
    fn test_verdict_is_valid_for_an_api_format_graph() {
        let payload = json!({ "valid": true, "warnings": [] });
        let validation: Validation = serde_json::from_value(payload).expect("fixture decodes");
        assert_eq!(validation.verdict(), Verdict::Valid);
    }

    /// Protects: rejection still surfaces findings.
    #[test]
    fn test_verdict_is_invalid_when_rejected() {
        let payload = json!({
            "valid": false,
            "errors": [{ "node_id": "35", "code": "missing_node", "message": "nope" }]
        });
        let validation: Validation = serde_json::from_value(payload).expect("fixture decodes");
        assert_eq!(validation.verdict(), Verdict::Invalid);
        let finding = &validation.errors[0];
        assert_eq!(finding.node_id.as_deref(), Some("35"));
        assert_eq!(finding.code.as_deref(), Some("missing_node"));
        assert_eq!(finding.message.as_deref(), Some("nope"));
    }

    /// Protects: every Finding field is optional.
    #[test]
    fn test_findings_tolerate_missing_fields() {
        let payload = json!({ "code": "x" });
        let finding: Finding = serde_json::from_value(payload).expect("finding decodes");
        assert_eq!(finding.code.as_deref(), Some("x"));
        assert!(finding.node_id.is_none());
        assert!(finding.field.is_none());
        assert!(finding.message.is_none());
        assert!(finding.hint.is_none());
    }

    /// Protects: the `:` / `/` mismatch.
    #[test]
    fn test_node_id_translates_to_a_slot_instance() {
        assert_eq!(node_id_to_instance("37:43"), "37/43");
        assert_eq!(node_id_to_instance("35"), "35");
    }

    /// Protects: a product rule. T-104 gates running on this.
    #[test]
    fn test_validation_reports_credit_spending() {
        let payload = json!({ "valid": true, "warnings": [], "spends_credits": true });
        let validation: Validation = serde_json::from_value(payload).expect("fixture decodes");
        assert!(validation.spends_credits);
    }

    /// Protects: the untrusted-data boundary.
    #[tokio::test]
    async fn test_notes_decode_and_are_returned_verbatim() {
        let text = "## Model Links\n- [x](https://huggingface.co/...)\n\nNote: Please update ComfyUI first";
        let reply = json!({
            "workflow": "wf.json",
            "count": 1,
            "notes": [{
                "id": 40,
                "type": "MarkdownNote",
                "title": null,
                "text": text
            }]
        });
        let (client, _recorded) = client_and_log(vec![Reply::Json(reply)]).await;

        let list = client
            .notes(std::path::Path::new("wf.json"))
            .await
            .expect("notes decode");

        assert_eq!(list.notes.len(), 1);
        assert_eq!(list.notes[0].text, text);
        assert_eq!(list.notes[0].ty.as_deref(), Some("MarkdownNote"));
    }

    /// Protects: no notes is a normal result.
    #[tokio::test]
    async fn test_notes_decode_an_empty_list() {
        let reply = json!({
            "workflow": "wf.json",
            "count": 0,
            "notes": []
        });
        let (client, _recorded) = client_and_log(vec![Reply::Json(reply)]).await;

        let list = client
            .notes(std::path::Path::new("wf.json"))
            .await
            .expect("empty notes decode");

        assert!(list.notes.is_empty());
    }

    /// Protects: argument naming on a surface that rejects a misspelling outright.
    #[tokio::test]
    async fn test_validate_sends_workflow_path() {
        let reply = json!({
            "valid": true,
            "errors": [],
            "warnings": []
        });
        let (client, recorded) = client_and_log(vec![Reply::Json(reply)]).await;

        let _ = client
            .validate(std::path::Path::new("wf.json"))
            .await
            .expect("validate succeeds");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["name"], json!("validate_workflow"));
        assert_eq!(log[0]["arguments"]["workflow_path"], json!("wf.json"));
    }
}
