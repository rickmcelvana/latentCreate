//! Pure workflow graph edits that slots cannot express.
//!
//! The only edit here is the lossless output swap: ComfyUI's `format` widget is a
//! dynamic combo that is not surfaced as a slot, so it must be rewritten as a
//! positional entry in the node's `widgets_values` (MCP-SURFACE 16.1).

use crate::profile::OutputSpec;
use serde_json::Value;

/// The audio save nodes ComfyUI ships.
///
/// `SaveAudio` (FLAC), `SaveAudioMP3` and `SaveAudioOpus` are all marked
/// DEPRECATED in the install this was verified against; `SaveAudioAdvanced` is
/// the current one (MCP-SURFACE 5). All four are recognised because a template
/// may ship any of them, and a save node this list misses is a graph the app
/// silently leaves writing MP3.
const SAVE_NODE_TYPES: [&str; 4] = [
    "SaveAudio",
    "SaveAudioMP3",
    "SaveAudioOpus",
    "SaveAudioAdvanced",
];

/// The only lossless format `SaveAudioAdvanced` offers.
///
/// Verified against the live node schema: the options are exactly `flac`,
/// `mp3` and `opus`, there is no WAV, and `flac` alone has no `quality`
/// sub-widget (MCP-SURFACE 16.1).
pub const LOSSLESS_FORMAT: &str = "flac";

/// Why a workflow could not be edited.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GraphError {
    /// The file is not a workflow shaped the way every template is.
    #[error("workflow is malformed: {detail}")]
    Malformed { detail: String },
    /// No audio save node anywhere in the graph.
    ///
    /// An error, never a silent pass: a workflow the app cannot make lossless
    /// is one it must refuse to run, because the alternative is handing MP3 to
    /// the mastering stage without saying so.
    #[error("no audio save node found")]
    NoSaveNode,
}

/// What `ensure_lossless_output` did, for provenance and for the caller's logs.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveNodeChange {
    /// Ids of the nodes rewritten, in the order found.
    pub nodes: Vec<String>,
    /// The node class every one of them now has.
    pub node_type: String,
    /// The format value written, or `None` when the profile opted out.
    pub format: Option<String>,
}

/// Make every audio save node in `workflow` write the profile's format.
///
/// The test is the format value, not the node class (MCP-SURFACE 16.3).
/// ACE-Step's template ships `SaveAudioMP3`; MiniMax Music 3's ships
/// `SaveAudioAdvanced` already set to `mp3`, so a check that only asks
/// "is this the modern node" passes MiniMax and hands lossy audio to the
/// mastering stage.
///
/// `format` is a `COMFY_DYNAMICCOMBO_V3`: not a slot, unreachable by
/// `set_workflow_slot`, and a positional entry in `widgets_values`
/// (MCP-SURFACE 16.1). The array length varies by format, so it is rebuilt to
/// exactly two entries rather than patched, or a stale `"V0"` survives.
///
/// `filename_prefix` is preserved: it is the part of this node the user
/// legitimately owns, and `107.filename_prefix` remains an ordinary slot.
pub fn ensure_lossless_output(
    workflow: &mut Value,
    output: &OutputSpec,
) -> Result<SaveNodeChange, GraphError> {
    if !output.prefer_lossless {
        // Opting out leaves the graph exactly as the template shipped it.
        // Both shipped profiles set this true; a profile that does not is
        // making a deliberate choice the app should not quietly override.
        return Ok(SaveNodeChange {
            nodes: Vec::new(),
            node_type: output.save_node.clone(),
            format: None,
        });
    }

    let mut changed = Vec::new();

    // Two sequential passes, not one collected list: each nodes array needs
    // its own mutable borrow of the workflow, and holding both at once does
    // not compile.
    if let Some(subgraphs) = workflow
        .pointer_mut("/definitions/subgraphs")
        .and_then(Value::as_array_mut)
    {
        for sub in subgraphs.iter_mut() {
            if let Some(nodes) = sub.get_mut("nodes").and_then(Value::as_array_mut) {
                rewrite_save_nodes(nodes, output, &mut changed)?;
            }
        }
    }
    if let Some(nodes) = workflow.get_mut("nodes").and_then(Value::as_array_mut) {
        rewrite_save_nodes(nodes, output, &mut changed)?;
    }

    if changed.is_empty() {
        return Err(GraphError::NoSaveNode);
    }

    Ok(SaveNodeChange {
        nodes: changed,
        node_type: output.save_node.clone(),
        format: Some(LOSSLESS_FORMAT.to_string()),
    })
}

/// Rewrite every save node in one nodes array.
///
/// Subgraph interiors are searched as well as the top level because MiniMax
/// Music 3 puts most of its graph inside a subgraph (its `UNETLoader` is at
/// `definitions.subgraphs[0].nodes[0]`). Its save node happens to be
/// top-level, but a template that nested one would otherwise be silently left
/// writing MP3 -- and silence is the failure this must not have.
fn rewrite_save_nodes(
    nodes: &mut [Value],
    output: &OutputSpec,
    changed: &mut Vec<String>,
) -> Result<(), GraphError> {
    for node in nodes.iter_mut() {
        let is_save = node
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| SAVE_NODE_TYPES.contains(&t));
        if !is_save {
            continue;
        }

        let prefix = node
            .get("widgets_values")
            .and_then(Value::as_array)
            .and_then(|w| w.first())
            .and_then(Value::as_str)
            .unwrap_or("audio/latentCreate")
            .to_string();
        let id = node
            .get("id")
            .map(|id| id.to_string())
            .ok_or_else(|| GraphError::Malformed {
                detail: "a save node has no id".to_string(),
            })?;

        let map = node.as_object_mut().ok_or_else(|| GraphError::Malformed {
            detail: format!("node {id} is not an object"),
        })?;
        map.insert("type".to_string(), Value::String(output.save_node.clone()));
        map.insert(
            "widgets_values".to_string(),
            Value::Array(vec![
                Value::String(prefix),
                Value::String(LOSSLESS_FORMAT.to_string()),
            ]),
        );
        // Kept in step with `type`: the frontend uses it for search-and-replace
        // and a stale value makes the node look like the class it no longer is.
        if let Some(props) = map.get_mut("properties").and_then(Value::as_object_mut) {
            if props.contains_key("Node name for S&R") {
                props.insert(
                    "Node name for S&R".to_string(),
                    Value::String(output.save_node.clone()),
                );
            }
        }
        changed.push(id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::OutputSpec;
    use serde_json::json;
    use std::path::PathBuf;

    fn ace_output() -> OutputSpec {
        OutputSpec {
            save_node: "SaveAudioAdvanced".to_string(),
            prefer_lossless: true,
        }
    }

    fn load_fixture(name: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/workflows")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        serde_json::from_str(&text).unwrap()
    }

    fn node_by_id<'a>(workflow: &'a Value, id: &str) -> Option<&'a Value> {
        let all = all_nodes(workflow);
        all.into_iter()
            .find(|n| n.get("id").map(|v| v.to_string()) == Some(id.to_string()))
    }

    fn all_nodes(workflow: &Value) -> Vec<&Value> {
        let mut out = Vec::new();
        if let Some(nodes) = workflow.get("nodes").and_then(Value::as_array) {
            out.extend(nodes.iter());
        }
        if let Some(subgraphs) = workflow
            .pointer("/definitions/subgraphs")
            .and_then(Value::as_array)
        {
            for sub in subgraphs {
                if let Some(nodes) = sub.get("nodes").and_then(Value::as_array) {
                    out.extend(nodes.iter());
                }
            }
        }
        out
    }

    /// A deep copy of `workflow` with the node `id` removed from every nodes array.
    ///
    /// The whole document, not just the nodes: `links`, `extra`, `groups`,
    /// `last_node_id` and the subgraph definitions all come along. Comparing
    /// node objects pairwise is not enough -- a version of
    /// `ensure_lossless_output` that deleted the entire `links` array passed a
    /// node-only comparison, and a workflow with no links is a disconnected
    /// graph that T-306 would submit.
    fn without_node(workflow: &Value, id: &str) -> Value {
        let mut copy = workflow.clone();
        let matches = |n: &Value| n.get("id").map(|v| v.to_string()).as_deref() == Some(id);
        if let Some(nodes) = copy.get_mut("nodes").and_then(Value::as_array_mut) {
            nodes.retain(|n| !matches(n));
        }
        if let Some(subgraphs) = copy
            .pointer_mut("/definitions/subgraphs")
            .and_then(Value::as_array_mut)
        {
            for sub in subgraphs.iter_mut() {
                if let Some(nodes) = sub.get_mut("nodes").and_then(Value::as_array_mut) {
                    nodes.retain(|n| !matches(n));
                }
            }
        }
        copy
    }

    /// Everything except the rewritten save node is byte-identical.
    fn assert_rest_of_workflow_unchanged(original: &Value, modified: &Value, save_id: &str) {
        assert_ne!(
            without_node(original, save_id),
            *original,
            "fixture has no node {save_id}; this assertion would be vacuous"
        );
        assert_eq!(
            without_node(original, save_id),
            without_node(modified, save_id),
            "the edit touched something other than node {save_id}"
        );
    }

    #[test]
    fn test_ace_step_save_node_rewrites_to_lossless() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let change = ensure_lossless_output(&mut workflow, &ace_output()).unwrap();

        assert_eq!(change.format, Some("flac".to_string()));
        assert_eq!(change.node_type, "SaveAudioAdvanced");
        assert!(change.nodes.contains(&"107".to_string()));

        let node = node_by_id(&workflow, "107").expect("node 107");
        assert_eq!(
            node.get("type").and_then(Value::as_str),
            Some("SaveAudioAdvanced")
        );
        let widgets = node
            .get("widgets_values")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[0].as_str(), Some("audio/ACE_Step1.5_xl_turbo"));
        assert_eq!(widgets[1].as_str(), Some("flac"));
    }

    #[test]
    fn test_minimax_save_node_rewrites_mp3_to_flac() {
        let mut workflow = load_fixture("minimax_music3_int8.json");

        // The precondition is the whole point of this test: MiniMax ships the
        // modern node class *already set to mp3*, with a third `quality` entry.
        // Asserted so a re-fetched fixture that no longer ships mp3 turns this
        // test red instead of leaving it quietly proving nothing.
        let before = node_by_id(&workflow, "35").expect("node 35");
        assert_eq!(
            before.get("type").and_then(Value::as_str),
            Some("SaveAudioAdvanced")
        );
        assert_eq!(
            before.get("widgets_values").and_then(Value::as_array),
            Some(&vec![
                Value::from("audio/audio_minimax_music3"),
                Value::from("mp3"),
                Value::from("V0"),
            ])
        );

        let change = ensure_lossless_output(&mut workflow, &ace_output()).unwrap();

        assert_eq!(change.format, Some("flac".to_string()));
        assert!(change.nodes.contains(&"35".to_string()));

        let node = node_by_id(&workflow, "35").expect("node 35");
        assert_eq!(
            node.get("type").and_then(Value::as_str),
            Some("SaveAudioAdvanced")
        );
        let widgets = node
            .get("widgets_values")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[0].as_str(), Some("audio/audio_minimax_music3"));
        assert_eq!(widgets[1].as_str(), Some("flac"));
    }

    #[test]
    fn test_node_name_for_sr_follows_type_when_present() {
        let mut workflow = json!({
            "nodes": [{
                "id": 7,
                "type": "SaveAudioMP3",
                "widgets_values": ["audio/test", "V0"],
                "properties": { "Node name for S&R": "SaveAudioMP3" }
            }]
        });
        let _change = ensure_lossless_output(&mut workflow, &ace_output()).unwrap();
        let node = node_by_id(&workflow, "7").unwrap();
        let props = node.get("properties").and_then(Value::as_object).unwrap();
        assert_eq!(
            props.get("Node name for S&R").and_then(Value::as_str),
            Some("SaveAudioAdvanced")
        );
    }

    #[test]
    fn test_missing_node_name_for_sr_is_not_an_error() {
        let mut workflow = json!({
            "nodes": [{
                "id": 8,
                "type": "SaveAudioMP3",
                "widgets_values": ["audio/test", "V0"]
            }]
        });
        let change = ensure_lossless_output(&mut workflow, &ace_output()).unwrap();
        assert_eq!(change.nodes, vec!["8".to_string()]);
    }

    #[test]
    fn test_every_save_node_is_rewritten_not_just_the_first() {
        // Neither shipped template has two save nodes, so nothing else here
        // enforces the "every" in this function's contract: a version that
        // stopped after the first one passed the whole suite. A workflow that
        // writes a preview alongside a master is the shape that breaks, and it
        // breaks by silently leaving the second one on MP3.
        let mut workflow = json!({
            "definitions": { "subgraphs": [{ "nodes": [
                { "id": 9, "type": "SaveAudioOpus", "widgets_values": ["audio/nested", "128k"] }
            ]}]},
            "nodes": [
                { "id": 1, "type": "SaveAudioMP3", "widgets_values": ["audio/preview", "V0"] },
                { "id": 2, "type": "KSampler", "widgets_values": [42] },
                { "id": 3, "type": "SaveAudio", "widgets_values": ["audio/master"] }
            ]
        });
        let change = ensure_lossless_output(&mut workflow, &ace_output()).unwrap();

        assert_eq!(change.nodes.len(), 3, "every save node is reported");
        for id in ["1", "3", "9"] {
            let node = node_by_id(&workflow, id).unwrap_or_else(|| panic!("node {id}"));
            assert_eq!(
                node.get("type").and_then(Value::as_str),
                Some("SaveAudioAdvanced"),
                "node {id} keeps its old class"
            );
            let widgets = node
                .get("widgets_values")
                .and_then(Value::as_array)
                .unwrap();
            assert_eq!(widgets.len(), 2, "node {id} has a stale sub-widget");
            assert_eq!(
                widgets[1].as_str(),
                Some("flac"),
                "node {id} is not lossless"
            );
        }
        // The non-save node in the middle is untouched, including its number.
        let sampler = node_by_id(&workflow, "2").unwrap();
        assert_eq!(
            sampler.get("widgets_values").and_then(Value::as_array),
            Some(&vec![Value::from(42)])
        );
    }

    #[test]
    fn test_no_save_node_returns_error() {
        let mut workflow = json!({ "nodes": [{ "id": 1, "type": "LoadAudio" }] });
        let err = ensure_lossless_output(&mut workflow, &ace_output()).unwrap_err();
        assert_eq!(err, GraphError::NoSaveNode);
    }

    #[test]
    fn test_prefer_lossless_false_leaves_workflow_unchanged() {
        let original = load_fixture("ace_step_1_5_xl_turbo.json");
        let mut workflow = original.clone();
        let output = OutputSpec {
            save_node: "SaveAudioAdvanced".to_string(),
            prefer_lossless: false,
        };
        let change = ensure_lossless_output(&mut workflow, &output).unwrap();
        assert!(change.nodes.is_empty());
        assert_eq!(change.format, None);
        assert_eq!(workflow, original);
    }

    #[test]
    fn test_nested_subgraph_save_node_is_rewritten() {
        let mut workflow = json!({
            "definitions": {
                "subgraphs": [
                    {
                        "nodes": [
                            {
                                "id": 5,
                                "type": "SaveAudioMP3",
                                "widgets_values": ["audio/nested", "V0"]
                            }
                        ]
                    }
                ]
            },
            "nodes": []
        });
        let change = ensure_lossless_output(&mut workflow, &ace_output()).unwrap();
        assert_eq!(change.nodes, vec!["5".to_string()]);
        let widgets = workflow
            .pointer("/definitions/subgraphs/0/nodes/0/widgets_values")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[0].as_str(), Some("audio/nested"));
        assert_eq!(widgets[1].as_str(), Some("flac"));
    }

    #[test]
    fn test_ace_step_rest_of_workflow_unchanged() {
        let original = load_fixture("ace_step_1_5_xl_turbo.json");
        let mut workflow = original.clone();
        ensure_lossless_output(&mut workflow, &ace_output()).unwrap();
        assert_rest_of_workflow_unchanged(&original, &workflow, "107");
    }

    #[test]
    fn test_minimax_rest_of_workflow_unchanged() {
        let original = load_fixture("minimax_music3_int8.json");
        let mut workflow = original.clone();
        ensure_lossless_output(&mut workflow, &ace_output()).unwrap();
        assert_rest_of_workflow_unchanged(&original, &workflow, "35");
    }
}
