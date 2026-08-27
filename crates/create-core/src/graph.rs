//! Pure workflow graph edits that slots cannot express.
//!
//! The edits are the lossless output swap and the LoRA stack splice. ComfyUI's
//! `format` widget is a dynamic combo that is not surfaced as a slot, so it must be
//! rewritten as a positional entry in the node's `widgets_values` (MCP-SURFACE
//! section 16.1). LoRA loaders must be inserted as nodes and wired into the
//! MODEL chain, because slots cannot add nodes (MCP-SURFACE 4).

use crate::profile::{LoraSupport, OutputSpec};
use serde_json::{json, Value};

/// The audio save nodes ComfyUI ships.
///
/// `SaveAudio` (FLAC), `SaveAudioMP3` and `SaveAudioOpus` are all marked
/// DEPRECATED in the install this was verified against; `SaveAudioAdvanced` is
/// the current one (MCP-SURFACE 5). All four are recognised because a
/// template may ship any of them, and a save node this list misses is a graph
/// the app silently leaves writing MP3.
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

/// One LoRA the user stacked, in the order it is applied.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraChoice {
    /// Exactly as ComfyUI lists it -- backslashes and subdirectories included.
    pub name: String,
    pub strength: f64,
}

/// What `splice_loras` inserted.
#[derive(Debug, Clone, PartialEq)]
pub struct SpliceChange {
    /// Ids of the inserted loader nodes, in apply order. Empty for an empty stack.
    pub nodes: Vec<String>,
}

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
    /// More LoRAs were stacked than the profile allows.
    #[error("too many LoRAs selected: profile allows {max}, got {got}")]
    TooManyLoras { max: u8, got: usize },
    /// A LoRA strength is outside the profile's declared range.
    #[error("strength for '{lora}' must be between {min} and {max}, got {value}")]
    StrengthOutOfRange {
        lora: String,
        min: f64,
        max: f64,
        value: f64,
    },
    /// The profile's `attach_after` node is not in the top-level graph.
    #[error("attach point '{id}' not found in workflow")]
    NoAttachPoint { id: String },
    /// The anchor node has no MODEL output to splice after.
    #[error("node '{id}' has no MODEL output")]
    NoModelOutput { id: String },
    /// The anchor's MODEL output feeds nothing, so a LoRA there would not
    /// affect the result.
    #[error("node '{id}' has no MODEL consumer, so a LoRA there would do nothing")]
    NoModelConsumer { id: String },
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
/// (MCP-SURFACE 16.1). The array length varies by format, so it is
/// rebuilt to exactly two entries rather than patched, or a stale `"V0"`
/// survives.
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

/// Insert the LoRA stack into the MODEL chain, after the profile's attach point.
///
/// Splicing is top-level only. If `attach_after` names a node that exists only
/// inside `definitions.subgraphs`, this returns `NoAttachPoint` rather than
/// attempting it, because subgraph interiors have their own id and link space
/// and getting that wrong is another silent no-op.
pub fn splice_loras(
    workflow: &mut Value,
    loras: &LoraSupport,
    stack: &[LoraChoice],
) -> Result<SpliceChange, GraphError> {
    if stack.is_empty() {
        return Ok(SpliceChange { nodes: Vec::new() });
    }
    if stack.len() > loras.max_stack as usize {
        return Err(GraphError::TooManyLoras {
            max: loras.max_stack,
            got: stack.len(),
        });
    }
    for choice in stack {
        if choice.strength < loras.strength.min || choice.strength > loras.strength.max {
            return Err(GraphError::StrengthOutOfRange {
                lora: choice.name.clone(),
                min: loras.strength.min,
                max: loras.strength.max,
                value: choice.strength,
            });
        }
    }

    let attach = &loras.attach_after;
    let (src_slot, consumers, anchor_pos, anchor_order) = read_anchor(workflow, attach)?;

    let mut next_node = next_id(workflow, "last_node_id", max_node_id(workflow));
    let mut next_link = next_id(workflow, "last_link_id", max_link_id(workflow));

    let loader_ids: Vec<i64> = (0..stack.len()).map(|i| next_node + i as i64).collect();
    next_node += stack.len() as i64;

    let feed: Vec<i64> = (0..stack.len()).map(|i| next_link + i as i64).collect();
    next_link += stack.len() as i64;

    let mut new_links: Vec<Value> = Vec::new();
    new_links.push(Value::Array(vec![
        Value::from(feed[0]),
        Value::from(attach.parse::<i64>().unwrap_or_default()),
        Value::from(src_slot),
        Value::from(loader_ids[0]),
        Value::from(0),
        Value::from("MODEL"),
    ]));
    for i in 1..stack.len() {
        new_links.push(Value::Array(vec![
            Value::from(feed[i]),
            Value::from(loader_ids[i - 1]),
            Value::from(0),
            Value::from(loader_ids[i]),
            Value::from(0),
            Value::from("MODEL"),
        ]));
    }

    let last_loader = *loader_ids.last().expect("stack is non-empty");
    let mut made: Vec<Value> = Vec::new();
    for (i, choice) in stack.iter().enumerate() {
        let outgoing: Vec<Value> = if i + 1 < stack.len() {
            vec![Value::from(feed[i + 1])]
        } else {
            consumers.iter().map(|id| Value::from(*id)).collect()
        };
        made.push(json!({
            "id": loader_ids[i],
            "type": loras.loader_node,
            "pos": [anchor_pos.0, anchor_pos.1 + 160.0 * (i as f64 + 1.0)],
            "size": [330, 82],
            "flags": {},
            "order": anchor_order,
            "mode": 0,
            "inputs": [{ "name": "model", "type": "MODEL", "link": feed[i] }],
            "outputs": [{ "name": "MODEL", "type": "MODEL", "links": outgoing }],
            "properties": { "Node name for S&R": loras.loader_node },
            "widgets_values": [choice.name, choice.strength],
        }));
    }

    let links = workflow
        .get_mut("links")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| GraphError::Malformed {
            detail: "workflow has no links array".to_string(),
        })?;
    for link in links.iter_mut() {
        let id = link.get(0).and_then(Value::as_i64);
        if id.is_some_and(|id| consumers.contains(&id)) {
            let entry = link.as_array_mut().expect("link is an array");
            entry[1] = Value::from(last_loader);
            entry[2] = Value::from(0);
        }
    }
    links.extend(new_links);

    let all = workflow
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .expect("checked in read_anchor");
    all.extend(made);
    for node in all.iter_mut() {
        if node.get("id").map(|v| v.to_string()).as_deref() == Some(attach.as_str()) {
            let out = node
                .pointer_mut(&format!("/outputs/{src_slot}/links"))
                .expect("checked in read_anchor");
            *out = Value::Array(vec![Value::from(feed[0])]);
        }
    }

    if let Some(map) = workflow.as_object_mut() {
        map.insert("last_node_id".to_string(), Value::from(next_node - 1));
        map.insert("last_link_id".to_string(), Value::from(next_link - 1));
    }

    Ok(SpliceChange {
        nodes: loader_ids.iter().map(|id| id.to_string()).collect(),
    })
}

type Anchor = (usize, Vec<i64>, (f64, f64), i64);

fn read_anchor(workflow: &Value, attach: &str) -> Result<Anchor, GraphError> {
    let nodes = workflow
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GraphError::Malformed {
            detail: "workflow has no top-level nodes array".to_string(),
        })?;
    let node = nodes
        .iter()
        .find(|n| n.get("id").map(|v| v.to_string()).as_deref() == Some(attach))
        .ok_or_else(|| GraphError::NoAttachPoint {
            id: attach.to_string(),
        })?;

    let outputs = node.get("outputs").and_then(Value::as_array);
    let (slot, output) = outputs
        .into_iter()
        .flatten()
        .enumerate()
        .find(|(_, o)| o.get("type").and_then(Value::as_str) == Some("MODEL"))
        .ok_or_else(|| GraphError::NoModelOutput {
            id: attach.to_string(),
        })?;

    let consumers: Vec<i64> = output
        .get("links")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    if consumers.is_empty() {
        return Err(GraphError::NoModelConsumer {
            id: attach.to_string(),
        });
    }

    let pos = node.get("pos").and_then(Value::as_array);
    let x = pos
        .and_then(|p| p.first())
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let y = pos
        .and_then(|p| p.get(1))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let order = node.get("order").and_then(Value::as_i64).unwrap_or(0);

    Ok((slot, consumers, (x, y), order))
}

/// One past the document's high-water mark for `key`.
fn next_id(workflow: &Value, key: &str, present: i64) -> i64 {
    let declared = workflow.get(key).and_then(Value::as_i64).unwrap_or(0);
    declared.max(present) + 1
}

fn max_node_id(workflow: &Value) -> i64 {
    let mut max = 0;
    let mut arrays: Vec<&Value> = Vec::new();
    if let Some(nodes) = workflow.get("nodes") {
        arrays.push(nodes);
    }
    if let Some(subs) = workflow
        .pointer("/definitions/subgraphs")
        .and_then(Value::as_array)
    {
        for sub in subs {
            if let Some(nodes) = sub.get("nodes") {
                arrays.push(nodes);
            }
        }
    }
    for array in arrays {
        for node in array.as_array().into_iter().flatten() {
            max = max.max(node.get("id").and_then(Value::as_i64).unwrap_or(0));
        }
    }
    max
}

fn max_link_id(workflow: &Value) -> i64 {
    workflow
        .get("links")
        .and_then(Value::as_array)
        .map(|links| {
            links
                .iter()
                .filter_map(|l| l.get(0).and_then(Value::as_i64))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{ModelProfile, OutputSpec};
    use serde_json::json;
    use std::path::PathBuf;

    fn ace_output() -> OutputSpec {
        OutputSpec {
            save_node: "SaveAudioAdvanced".to_string(),
            prefer_lossless: true,
        }
    }

    fn ace_loras() -> LoraSupport {
        let profile: ModelProfile =
            serde_json::from_str(include_str!("../../../profiles/ace-step-1.5-turbo.json"))
                .unwrap();
        profile.loras.expect("ACE-Step profile has loras")
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

    fn link_by_id(workflow: &Value, id: i64) -> Option<&Value> {
        workflow
            .get("links")
            .and_then(Value::as_array)?
            .iter()
            .find(|l| l.get(0).and_then(Value::as_i64) == Some(id))
    }

    /// Assert the MODEL chain runs through `expected` in order, in **all three**
    /// places a workflow records an edge.
    ///
    /// The `links` array alone is not enough, and that is not a hypothetical:
    /// a version of `splice_loras` that left every loader's `inputs[0].link`
    /// null passed all 118 tests with only the `links` check, and the live
    /// validator rejected it with `required_input_missing` (MCP-SURFACE 17).
    /// **The UI-to-API converter builds the graph from `inputs[].link`** --
    /// verified by running a workflow whose anchor `outputs[].links` was stale
    /// and getting an identical executed prompt -- so that is the load-bearing
    /// field and the one a test must check.
    fn assert_model_chain(workflow: &Value, expected: &[i64]) {
        assert!(
            !expected.is_empty(),
            "chain must have at least a start node"
        );
        let links = workflow
            .get("links")
            .and_then(Value::as_array)
            .expect("links array");
        for window in expected.windows(2) {
            let src = window[0];
            let dst = window[1];
            let matches: Vec<&Value> = links
                .iter()
                .filter(|l| {
                    let entry = l.as_array().expect("link is an array");
                    entry.len() == 6
                        && entry[1].as_i64() == Some(src)
                        && entry[3].as_i64() == Some(dst)
                        && entry[5].as_str() == Some("MODEL")
                })
                .collect();
            assert_eq!(
                matches.len(),
                1,
                "expected exactly one MODEL link from {src} to {dst}, found {matches:?}"
            );
            let link_id = matches[0].as_array().expect("link is an array")[0]
                .as_i64()
                .expect("link id");

            // Load-bearing: the destination claims this link as an input.
            let dst_node = node_by_id(workflow, &dst.to_string())
                .unwrap_or_else(|| panic!("node {dst} is in the chain but not in the graph"));
            let claimed = dst_node
                .get("inputs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|i| i.get("link").and_then(Value::as_i64) == Some(link_id));
            assert!(
                claimed,
                "node {dst} has no input carrying link {link_id}; the links array says                  {src} feeds it, but the engine reads inputs[].link and would reject this"
            );

            // Editor-facing: the source lists this link among its outputs. A
            // stale list here still executes correctly (verified live), but it
            // renders the graph wrong if the user opens it in ComfyUI.
            let src_node = node_by_id(workflow, &src.to_string())
                .unwrap_or_else(|| panic!("node {src} is in the chain but not in the graph"));
            let listed = src_node
                .get("outputs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|o| {
                    o.get("links")
                        .and_then(Value::as_array)
                        .is_some_and(|l| l.iter().any(|v| v.as_i64() == Some(link_id)))
                });
            assert!(
                listed,
                "node {src} does not list link {link_id} in its outputs; the graph would                  render wrong in the ComfyUI editor"
            );
        }
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

    fn without_nodes(workflow: &Value, ids: &[&str]) -> Value {
        let mut copy = workflow.clone();
        let matches = |n: &Value| {
            n.get("id")
                .map(|v| v.to_string())
                .as_deref()
                .map(|id| ids.contains(&id))
                .unwrap_or(false)
        };
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

    fn without_link_ids(workflow: &Value, link_ids: &[i64]) -> Value {
        let mut copy = workflow.clone();
        if let Some(links) = copy.get_mut("links").and_then(Value::as_array_mut) {
            links.retain(|l| {
                !l.get(0)
                    .and_then(Value::as_i64)
                    .map(|id| link_ids.contains(&id))
                    .unwrap_or(false)
            });
        }
        copy
    }

    fn without_high_water_marks(workflow: &Value) -> Value {
        let mut copy = workflow.clone();
        if let Some(map) = copy.as_object_mut() {
            map.remove("last_node_id");
            map.remove("last_link_id");
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

    /// Everything outside the anchor, the inserted loaders and the consumer
    /// links is unchanged. The high-water marks are allowed to move.
    fn assert_splice_rest_unchanged(
        original: &Value,
        modified: &Value,
        anchor_id: &str,
        loader_ids: &[&str],
        consumer_link_ids: &[i64],
        new_link_ids: &[i64],
    ) {
        let mut expected = without_nodes(original, &[anchor_id]);
        expected = without_link_ids(&expected, consumer_link_ids);
        expected = without_high_water_marks(&expected);

        let mut actual = without_nodes(modified, &[anchor_id]);
        actual = without_nodes(&actual, loader_ids);
        let mut removed_links = consumer_link_ids.to_vec();
        removed_links.extend(new_link_ids);
        actual = without_link_ids(&actual, &removed_links);
        actual = without_high_water_marks(&actual);

        assert_eq!(
            expected, actual,
            "splice touched something outside the anchor, loaders, and consumer links"
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

    #[test]
    fn test_two_lora_stack_chains_through_loaders_in_order() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let loras = ace_loras();
        let stack = vec![
            LoraChoice {
                name: "ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors".to_string(),
                strength: 0.75,
            },
            LoraChoice {
                name: "ACE-Step-v1.5-raspy-vocal-and-instrumental-5-LoRAs\\male_vocals_adapter_model.safetensors".to_string(),
                strength: 1.25,
            },
        ];
        let change = splice_loras(&mut workflow, &loras, &stack).unwrap();

        assert_eq!(change.nodes, vec!["111".to_string(), "112".to_string()]);
        assert_model_chain(&workflow, &[104, 111, 112, 78, 3]);

        // The loaders carry the stack in order, with backslashes preserved.
        let first = node_by_id(&workflow, "111").unwrap();
        let first_widgets = first
            .get("widgets_values")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(first_widgets.len(), 2);
        assert_eq!(
            first_widgets[0].as_str(),
            Some("ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors")
        );
        assert_eq!(first_widgets[1].as_f64(), Some(0.75));

        let second = node_by_id(&workflow, "112").unwrap();
        let second_widgets = second
            .get("widgets_values")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(second_widgets.len(), 2);
        assert_eq!(
            second_widgets[0].as_str(),
            Some("ACE-Step-v1.5-raspy-vocal-and-instrumental-5-LoRAs\\male_vocals_adapter_model.safetensors")
        );
        assert_eq!(second_widgets[1].as_f64(), Some(1.25));
    }

    #[test]
    fn test_consumer_node_and_link_keep_their_ids() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let loras = ace_loras();
        let stack = vec![
            LoraChoice {
                name: "lora1.safetensors".to_string(),
                strength: 1.0,
            },
            LoraChoice {
                name: "lora2.safetensors".to_string(),
                strength: 1.0,
            },
        ];
        splice_loras(&mut workflow, &loras, &stack).unwrap();

        // Node 78 is not edited: its input still points at link 260.
        let consumer = node_by_id(&workflow, "78").unwrap();
        let inputs = consumer.get("inputs").and_then(Value::as_array).unwrap();
        assert_eq!(inputs[0].get("link").and_then(Value::as_i64), Some(260));

        // Link 260 keeps its id and destination; only its source moves.
        let link = link_by_id(&workflow, 260).unwrap();
        let entry = link.as_array().unwrap();
        assert_eq!(entry[0].as_i64(), Some(260));
        assert_eq!(entry[1].as_i64(), Some(112));
        assert_eq!(entry[2].as_i64(), Some(0));
        assert_eq!(entry[3].as_i64(), Some(78));
        assert_eq!(entry[4].as_i64(), Some(0));
        assert_eq!(entry[5].as_str(), Some("MODEL"));
    }

    #[test]
    fn test_high_water_marks_bump_for_two_loras() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let loras = ace_loras();
        let stack = vec![
            LoraChoice {
                name: "lora1.safetensors".to_string(),
                strength: 1.0,
            },
            LoraChoice {
                name: "lora2.safetensors".to_string(),
                strength: 1.0,
            },
        ];
        splice_loras(&mut workflow, &loras, &stack).unwrap();

        assert_eq!(
            workflow.get("last_node_id").and_then(Value::as_i64),
            Some(112)
        );
        assert_eq!(
            workflow.get("last_link_id").and_then(Value::as_i64),
            Some(267)
        );

        // No id collision: the new ids did not exist before splicing.
        // (The fixture declares 110 and 265; 111/112 and 266/267 are fresh.)
    }

    #[test]
    fn test_one_lora_stack_chains_anchor_loader_consumer() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let loras = ace_loras();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 1.0,
        }];
        let change = splice_loras(&mut workflow, &loras, &stack).unwrap();

        assert_eq!(change.nodes, vec!["111".to_string()]);
        assert_model_chain(&workflow, &[104, 111, 78, 3]);
        assert_eq!(
            workflow.get("last_node_id").and_then(Value::as_i64),
            Some(111)
        );
        assert_eq!(
            workflow.get("last_link_id").and_then(Value::as_i64),
            Some(266)
        );
    }

    #[test]
    fn test_empty_stack_is_a_no_op() {
        let original = load_fixture("ace_step_1_5_xl_turbo.json");
        let mut workflow = original.clone();
        let loras = ace_loras();
        let change = splice_loras(&mut workflow, &loras, &[]).unwrap();

        assert!(change.nodes.is_empty());
        assert_eq!(
            workflow, original,
            "empty stack must not touch the workflow"
        );
    }

    #[test]
    fn test_fan_out_resources_all_consumer_links() {
        let mut workflow = json!({
            "last_node_id": 10,
            "last_link_id": 100,
            "nodes": [
                {
                    "id": 1,
                    "type": "UNETLoader",
                    "pos": [100.0, 100.0],
                    "order": 0,
                    "outputs": [{ "name": "MODEL", "type": "MODEL", "links": [101, 102] }]
                },
                {
                    "id": 2,
                    "type": "KSampler",
                    "inputs": [{ "name": "model", "type": "MODEL", "link": 101 }],
                    "outputs": []
                },
                {
                    "id": 3,
                    "type": "KSampler",
                    "inputs": [{ "name": "model", "type": "MODEL", "link": 102 }],
                    "outputs": []
                }
            ],
            "links": [
                [101, 1, 0, 2, 0, "MODEL"],
                [102, 1, 0, 3, 0, "MODEL"]
            ]
        });
        let mut loras = ace_loras();
        loras.attach_after = "1".to_string();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 1.0,
        }];
        let change = splice_loras(&mut workflow, &loras, &stack).unwrap();

        assert_eq!(change.nodes, vec!["11".to_string()]);
        // Both consumer links now source from the loader, keeping their ids.
        let link101 = link_by_id(&workflow, 101).unwrap().as_array().unwrap();
        assert_eq!(link101[1].as_i64(), Some(11));
        assert_eq!(link101[3].as_i64(), Some(2));
        let link102 = link_by_id(&workflow, 102).unwrap().as_array().unwrap();
        assert_eq!(link102[1].as_i64(), Some(11));
        assert_eq!(link102[3].as_i64(), Some(3));

        // The consumers themselves are untouched.
        let node2 = node_by_id(&workflow, "2").unwrap();
        assert_eq!(
            node2.get("inputs").and_then(Value::as_array).unwrap()[0]
                .get("link")
                .and_then(Value::as_i64),
            Some(101)
        );
        let node3 = node_by_id(&workflow, "3").unwrap();
        assert_eq!(
            node3.get("inputs").and_then(Value::as_array).unwrap()[0]
                .get("link")
                .and_then(Value::as_i64),
            Some(102)
        );
    }

    #[test]
    fn test_too_many_loras_errors() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let loras = ace_loras();
        let stack: Vec<LoraChoice> = (0..5)
            .map(|i| LoraChoice {
                name: format!("lora{i}.safetensors"),
                strength: 1.0,
            })
            .collect();
        let err = splice_loras(&mut workflow, &loras, &stack).unwrap_err();
        assert!(matches!(err, GraphError::TooManyLoras { max: 4, got: 5 }));
    }

    #[test]
    fn test_strength_out_of_range_errors() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let loras = ace_loras();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 2.5,
        }];
        let err = splice_loras(&mut workflow, &loras, &stack).unwrap_err();
        assert!(matches!(
            err,
            GraphError::StrengthOutOfRange {
                lora,
                min: 0.0,
                max: 2.0,
                value: 2.5
            } if lora == "lora1.safetensors"
        ));
    }

    #[test]
    fn test_no_attach_point_errors() {
        let mut workflow = load_fixture("ace_step_1_5_xl_turbo.json");
        let mut loras = ace_loras();
        loras.attach_after = "999".to_string();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 1.0,
        }];
        let err = splice_loras(&mut workflow, &loras, &stack).unwrap_err();
        assert!(matches!(err, GraphError::NoAttachPoint { id } if id == "999"));
    }

    #[test]
    fn test_no_model_output_errors() {
        let mut workflow = json!({
            "last_node_id": 1,
            "last_link_id": 1,
            "nodes": [
                {
                    "id": 1,
                    "type": "KSampler",
                    "pos": [0.0, 0.0],
                    "order": 0,
                    "outputs": [{ "name": "LATENT", "type": "LATENT", "links": [] }]
                }
            ],
            "links": []
        });
        let mut loras = ace_loras();
        loras.attach_after = "1".to_string();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 1.0,
        }];
        let err = splice_loras(&mut workflow, &loras, &stack).unwrap_err();
        assert!(matches!(err, GraphError::NoModelOutput { id } if id == "1"));
    }

    #[test]
    fn test_no_model_consumer_errors() {
        let mut workflow = json!({
            "last_node_id": 1,
            "last_link_id": 1,
            "nodes": [
                {
                    "id": 1,
                    "type": "UNETLoader",
                    "pos": [0.0, 0.0],
                    "order": 0,
                    "outputs": [{ "name": "MODEL", "type": "MODEL", "links": [] }]
                }
            ],
            "links": []
        });
        let mut loras = ace_loras();
        loras.attach_after = "1".to_string();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 1.0,
        }];
        let err = splice_loras(&mut workflow, &loras, &stack).unwrap_err();
        assert!(matches!(err, GraphError::NoModelConsumer { id } if id == "1"));
    }

    #[test]
    fn test_subgraph_only_attach_point_errors() {
        let mut workflow = json!({
            "last_node_id": 10,
            "last_link_id": 10,
            "nodes": [],
            "definitions": {
                "subgraphs": [
                    {
                        "nodes": [
                            {
                                "id": 99,
                                "type": "UNETLoader",
                                "pos": [0.0, 0.0],
                                "order": 0,
                                "outputs": [{ "name": "MODEL", "type": "MODEL", "links": [1] }]
                            }
                        ]
                    }
                ]
            },
            "links": [[1, 99, 0, 100, 0, "MODEL"]]
        });
        let mut loras = ace_loras();
        loras.attach_after = "99".to_string();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 1.0,
        }];
        let err = splice_loras(&mut workflow, &loras, &stack).unwrap_err();
        assert!(matches!(err, GraphError::NoAttachPoint { id } if id == "99"));
    }

    #[test]
    fn test_splice_uses_actual_high_water_mark_not_declared_one() {
        // last_node_id / last_link_id are stale (3), but a node and link with
        // id 4 already exist. A version that allocated from the declared value
        // alone would collide with the existing node 4 and link 4.
        let mut workflow = json!({
            "last_node_id": 3,
            "last_link_id": 3,
            "nodes": [
                {
                    "id": 1,
                    "type": "UNETLoader",
                    "pos": [0.0, 0.0],
                    "order": 0,
                    "outputs": [{ "name": "MODEL", "type": "MODEL", "links": [4] }]
                },
                {
                    "id": 4,
                    "type": "KSampler",
                    "inputs": [{ "name": "model", "type": "MODEL", "link": 4 }],
                    "outputs": []
                }
            ],
            "links": [[4, 1, 0, 4, 0, "MODEL"]]
        });
        let mut loras = ace_loras();
        loras.attach_after = "1".to_string();
        let stack = vec![LoraChoice {
            name: "lora1.safetensors".to_string(),
            strength: 1.0,
        }];
        let change = splice_loras(&mut workflow, &loras, &stack).unwrap();

        // Must allocate from the real max (4), not the stale declared value (3).
        assert_eq!(change.nodes, vec!["5".to_string()]);
        assert!(node_by_id(&workflow, "5").is_some());
        assert_model_chain(&workflow, &[1, 5, 4]);

        // The original node 4 is still the consumer, not overwritten.
        let consumers: Vec<&Value> = all_nodes(&workflow)
            .into_iter()
            .filter(|n| n.get("id").and_then(Value::as_i64) == Some(4))
            .collect();
        assert_eq!(
            consumers.len(),
            1,
            "node 4 must not be duplicated/overwritten"
        );
    }

    #[test]
    fn test_ace_step_rest_unchanged_after_splice() {
        let original = load_fixture("ace_step_1_5_xl_turbo.json");
        let mut workflow = original.clone();
        let loras = ace_loras();
        let stack = vec![
            LoraChoice {
                name: "lora1.safetensors".to_string(),
                strength: 1.0,
            },
            LoraChoice {
                name: "lora2.safetensors".to_string(),
                strength: 1.0,
            },
        ];
        splice_loras(&mut workflow, &loras, &stack).unwrap();

        assert_splice_rest_unchanged(
            &original,
            &workflow,
            "104",
            &["111", "112"],
            &[260],
            &[266, 267],
        );
    }
}
