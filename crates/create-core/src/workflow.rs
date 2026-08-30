//! Which of ComfyUI's two export shapes a file is.
//!
//! ComfyUI writes two different JSONs and comfy-mcp's tools do not accept the
//! same one (MCP-SURFACE 29). Everything this app does to a graph -- slots, the
//! audit, the T-305 edits -- needs the **frontend** shape.

use serde_json::Value;

/// One of ComfyUI's two export shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowFormat {
    /// `File > Save (As)` -- `nodes[]`/`links[]`. The editable shape, and the
    /// only one this app can drive.
    Frontend,
    /// `File > Export (API)` -- a flat map of id -> `{class_type, inputs}`.
    /// Runnable, but nothing can enumerate its parameters.
    Api,
}

/// Which shape `graph` is, or `None` for neither.
///
/// **API format is detected positively rather than inferred from "not
/// frontend".** The three outcomes are three different messages: a frontend
/// file proceeds, an API export gets told the menu item that produces the right
/// file, and something that is neither gets told it is not a workflow at all.
/// Collapsing the last two would tell a user who picked their tax return to
/// re-export it from ComfyUI.
pub fn detect_format(graph: &Value) -> Option<WorkflowFormat> {
    if graph.get("nodes").and_then(Value::as_array).is_some() {
        return Some(WorkflowFormat::Frontend);
    }
    let object = graph.as_object()?;
    // "Every", not "any": a frontend file carrying a stray `class_type`
    // somewhere must not be mistaken for an API one, and an empty object says
    // nothing at all about which shape it is.
    if !object.is_empty()
        && object
            .values()
            .all(|node| node.get("class_type").and_then(Value::as_str).is_some())
    {
        return Some(WorkflowFormat::Api);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(name: &str) -> Value {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/workflows")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        serde_json::from_str(&text).expect("fixture decodes")
    }

    /// Protects: the three outcomes stay three outcomes.
    ///
    /// Against **real** captured exports, not hand-made JSON. The API fixture
    /// is the executed graph of the T-315 crash-path run -- a file ComfyUI
    /// itself produced and then ran.
    #[test]
    fn test_the_three_shapes_are_told_apart() {
        assert_eq!(
            detect_format(&fixture("ace_step_1_5_xl_turbo.json")),
            Some(WorkflowFormat::Frontend)
        );
        assert_eq!(
            detect_format(&fixture("minimax_music3_int8.json")),
            Some(WorkflowFormat::Frontend)
        );
        assert_eq!(
            detect_format(&fixture("minimax_music3.api-format.json")),
            Some(WorkflowFormat::Api)
        );

        assert_eq!(
            detect_format(&json!({})),
            None,
            "an empty object says nothing"
        );
        assert_eq!(detect_format(&json!([1, 2])), None);
        assert_eq!(detect_format(&json!({ "a": 1 })), None);
        assert_eq!(detect_format(&json!("just a string")), None);
    }

    /// Protects: one node missing `class_type` is not an API export.
    ///
    /// `all` rather than `any` -- a map where most entries look right and one
    /// does not is something this app has never seen, and guessing it is an API
    /// export would route the user to the wrong remedy.
    #[test]
    fn test_a_partial_map_is_not_an_api_export() {
        let mixed = json!({
            "1": { "class_type": "KSampler", "inputs": {} },
            "2": { "inputs": {} }
        });
        assert_eq!(detect_format(&mixed), None);
    }

    /// Protects: `nodes[]` wins even when the file also has map-shaped
    /// entries. A frontend export is decided by its own marker, never by the
    /// absence of the other shape's.
    #[test]
    fn test_frontend_is_decided_by_its_own_marker() {
        let odd = json!({
            "nodes": [],
            "extra": { "class_type": "KSampler" }
        });
        assert_eq!(detect_format(&odd), Some(WorkflowFormat::Frontend));
    }
}
