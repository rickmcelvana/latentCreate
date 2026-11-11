//! Pure workflow graph edits that slot writes cannot express. API format only
//! (ARCHITECTURE 3a, 7).
//!
//! Two edits: making the audio save node lossless, and splicing the LoRA stack
//! into the MODEL chain. Both are plain JSON operations on
//! `{node_id: {class_type, inputs}}`; a link is `[source_id, output_index]`.

use crate::audit::{as_link, node_ids};
use crate::profile::{LoraSupport, OutputSpec};
use serde_json::{json, Value};

/// The audio save nodes ComfyUI ships. `SaveAudio` (FLAC), `SaveAudioMP3` and
/// `SaveAudioOpus` are deprecated; `SaveAudioAdvanced` is current. All four
/// are recognised because a workflow may carry any of them, and a save node
/// this list misses is a graph the app silently leaves writing MP3.
const SAVE_NODE_TYPES: [&str; 4] = [
    "SaveAudio",
    "SaveAudioMP3",
    "SaveAudioOpus",
    "SaveAudioAdvanced",
];

/// The only lossless format `SaveAudioAdvanced` offers: its options are exactly
/// `flac`, `mp3` and `opus`, there is no WAV, and `flac` alone has no sub-input
/// (COMFY-HTTP-SURFACE 4).
pub const LOSSLESS_FORMAT: &str = "flac";

/// One LoRA the user stacked, in the order it is applied.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraChoice {
    /// Exactly as ComfyUI lists it -- backslashes and subdirectories included.
    pub name: String,
    /// The loader's `strength_model`.
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
    /// Not an API-format workflow object.
    #[error("workflow is malformed: {detail}")]
    Malformed {
        /// What was wrong.
        detail: String,
    },
    /// No audio save node anywhere in the graph. An error, never a silent pass:
    /// a workflow the app cannot make lossless is one it must refuse to run.
    #[error("no audio save node found")]
    NoSaveNode,
    /// More LoRAs were stacked than the profile allows.
    #[error("too many LoRAs selected: profile allows {max}, got {got}")]
    TooManyLoras {
        /// The profile's `max_stack`.
        max: u8,
        /// How many were asked for.
        got: usize,
    },
    /// A LoRA strength is outside the profile's declared range.
    #[error("strength for '{lora}' must be between {min} and {max}, got {value}")]
    StrengthOutOfRange {
        /// The LoRA file.
        lora: String,
        /// Lower bound.
        min: f64,
        /// Upper bound.
        max: f64,
        /// What was asked for.
        value: f64,
    },
    /// The profile's `attach_after` node is not in the workflow.
    #[error("attach point '{id}' not found in workflow")]
    NoAttachPoint {
        /// The profile's `attach_after`.
        id: String,
    },
    /// Nothing consumes the anchor's MODEL output, so a LoRA there would not
    /// affect the result.
    #[error("node '{id}' has no MODEL consumer, so a LoRA there would do nothing")]
    NoModelConsumer {
        /// The profile's `attach_after`.
        id: String,
    },
}

/// What `ensure_lossless_output` did, for provenance and the caller's logs.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveNodeChange {
    /// Ids of the nodes rewritten, in node order.
    pub nodes: Vec<String>,
    /// The node class every one of them now has.
    pub node_type: String,
    /// The format value written, or `None` when the profile opted out.
    pub format: Option<String>,
}

/// Make every audio save node write the profile's lossless format.
///
/// **The test is the format value, not the node class** (MCP-SURFACE 16.3):
/// MiniMax's template ships `SaveAudioAdvanced` already set to `mp3`, so a check
/// that only asks "is this the modern node" ships MP3. In API format the
/// dynamic combo's sub-inputs are dotted sibling keys (`"format.quality"`), so
/// the edit is: retype to the profile's save node, set `format`, and remove
/// every `format.*` key and the deprecated nodes' `quality` -- a stale `V0`
/// beside `flac` is an input the node does not declare. `filename_prefix` and
/// the `audio` link are kept.
pub fn ensure_lossless_output(
    workflow: &mut Value,
    output: &OutputSpec,
) -> Result<SaveNodeChange, GraphError> {
    if !output.prefer_lossless {
        return Ok(SaveNodeChange {
            nodes: Vec::new(),
            node_type: output.save_node.clone(),
            format: None,
        });
    }
    let ids = node_ids(workflow);
    let map = workflow
        .as_object_mut()
        .ok_or_else(|| GraphError::Malformed {
            detail: "not a JSON object".into(),
        })?;
    let mut changed = Vec::new();
    for id in ids {
        let Some(node) = map.get_mut(&id) else {
            continue;
        };
        let is_save = node
            .get("class_type")
            .and_then(Value::as_str)
            .is_some_and(|t| SAVE_NODE_TYPES.contains(&t));
        if !is_save {
            continue;
        }
        let node = node.as_object_mut().ok_or_else(|| GraphError::Malformed {
            detail: format!("node {id} is not an object"),
        })?;
        node.insert("class_type".into(), Value::String(output.save_node.clone()));
        if let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) {
            inputs.retain(|k, _| k != "quality" && !k.starts_with("format."));
            inputs.insert("format".into(), Value::String(LOSSLESS_FORMAT.into()));
        } else {
            node.insert("inputs".into(), json!({"format": LOSSLESS_FORMAT}));
        }
        changed.push(id);
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

/// Insert the LoRA stack into the MODEL chain after the profile's attach point.
///
/// The anchor's MODEL is its output **0** -- true of every loader class a
/// profile attaches after (`UNETLoader`, `CheckpointLoaderSimple`, and the LoRA
/// loaders themselves). Every input linked to `[anchor, 0]` is rewired to the
/// last loader, and the loaders chain `anchor -> L1 -> ... -> Ln`. A chain that
/// feeds nothing still validates and runs with no LoRA applied (MCP-SURFACE
/// 17.1), so this refuses an anchor with no consumer, and the tests walk the
/// result from the sampler back to the anchor.
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
    let anchor = loras.attach_after.as_str();
    let map = workflow
        .as_object_mut()
        .ok_or_else(|| GraphError::Malformed {
            detail: "not a JSON object".into(),
        })?;
    if !map.contains_key(anchor) {
        return Err(GraphError::NoAttachPoint {
            id: anchor.to_string(),
        });
    }

    // Every (node, input) currently fed by the anchor's MODEL output.
    let mut consumers: Vec<(String, String)> = Vec::new();
    for (id, node) in map.iter() {
        let Some(inputs) = node.get("inputs").and_then(Value::as_object) else {
            continue;
        };
        for (name, value) in inputs {
            if as_link(value) == Some((anchor, 0)) {
                consumers.push((id.clone(), name.clone()));
            }
        }
    }
    if consumers.is_empty() {
        return Err(GraphError::NoModelConsumer {
            id: anchor.to_string(),
        });
    }

    // Fresh numeric ids above every top-level and subgraph id in use.
    let next = map
        .keys()
        .filter_map(|k| k.split(':').next().and_then(|h| h.parse::<u64>().ok()))
        .max()
        .unwrap_or(0)
        + 1;
    let mut previous = anchor.to_string();
    let mut made = Vec::new();
    for (offset, choice) in (0u64..).zip(stack) {
        let id = (next + offset).to_string();
        map.insert(
            id.clone(),
            json!({
                "class_type": loras.loader_node,
                "inputs": {
                    "model": [previous, 0],
                    "lora_name": choice.name,
                    "strength_model": choice.strength,
                },
                "_meta": {"title": "LoRA (latentCreate)"},
            }),
        );
        previous = id.clone();
        made.push(id);
    }
    for (id, input) in consumers {
        if let Some(slot) = map
            .get_mut(&id)
            .and_then(|n| n.get_mut("inputs"))
            .and_then(|i| i.get_mut(&input))
        {
            *slot = json!([previous, 0]);
        }
    }
    Ok(SpliceChange { nodes: made })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::StrengthRange;

    fn shipped(id: &str) -> Value {
        let path = format!(
            "{}/../../profiles/workflows/{id}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("workflow")).expect("JSON")
    }

    fn lossless() -> OutputSpec {
        OutputSpec {
            save_node: "SaveAudioAdvanced".into(),
            prefer_lossless: true,
        }
    }

    fn ace_loras() -> LoraSupport {
        LoraSupport {
            loader_node: "LoraLoaderModelOnly".into(),
            attach_after: "104".into(),
            folder: "loras".into(),
            strength: StrengthRange {
                min: 0.0,
                max: 2.0,
                default: 1.0,
                step: Some(0.05),
            },
            max_stack: 4,
        }
    }

    fn pick(name: &str, strength: f64) -> LoraChoice {
        LoraChoice {
            name: name.into(),
            strength,
        }
    }

    /// Protects: MiniMax ships `mp3` + `format.quality: V0`; the edit leaves
    /// `flac` and no stale sub-input, and keeps the prefix and the audio link.
    #[test]
    fn test_lossless_rewrites_an_mp3_save_node_to_flac() {
        let mut wf = shipped("minimax-music-3");
        let change = ensure_lossless_output(&mut wf, &lossless()).expect("edited");
        assert_eq!(change.nodes, ["35"]);
        let inputs = &wf["35"]["inputs"];
        assert_eq!(inputs["format"], "flac");
        assert!(inputs.get("format.quality").is_none());
        assert_eq!(inputs["filename_prefix"], "audio/audio_minimax_music3");
        assert!(as_link(&inputs["audio"]).is_some());
    }

    #[test]
    fn test_lossless_retypes_a_deprecated_mp3_node() {
        let mut wf = json!({
            "5": {"class_type": "SaveAudioMP3",
                  "inputs": {"audio": ["4", 0], "filename_prefix": "x", "quality": "V0"}}
        });
        ensure_lossless_output(&mut wf, &lossless()).expect("edited");
        assert_eq!(wf["5"]["class_type"], "SaveAudioAdvanced");
        assert_eq!(
            wf["5"]["inputs"],
            json!({"audio": ["4", 0], "filename_prefix": "x", "format": "flac"})
        );
    }

    #[test]
    fn test_lossless_refuses_a_graph_with_no_audio_save_node() {
        let mut wf = shipped("flux-1-schnell-fp8");
        assert_eq!(
            ensure_lossless_output(&mut wf, &lossless()),
            Err(GraphError::NoSaveNode)
        );
    }

    #[test]
    fn test_lossless_opt_out_leaves_the_graph_alone() {
        let mut wf = shipped("minimax-music-3");
        let before = wf.clone();
        let out = OutputSpec {
            save_node: "SaveAudioAdvanced".into(),
            prefer_lossless: false,
        };
        assert_eq!(
            ensure_lossless_output(&mut wf, &out).expect("ok").format,
            None
        );
        assert_eq!(wf, before);
    }

    /// Walk MODEL links back from `node` until a node with no `model` input.
    fn model_chain(wf: &Value, node: &str) -> Vec<String> {
        let mut chain = vec![node.to_string()];
        let mut at = node.to_string();
        while let Some((src, _)) = wf[&at]["inputs"].get("model").and_then(as_link) {
            chain.push(src.to_string());
            at = src.to_string();
        }
        chain
    }

    /// Protects: the spliced stack is **reachable** -- walking MODEL links back
    /// from what the anchor used to feed passes through every loader in apply
    /// order and ends at the anchor (MCP-SURFACE 17.1: an unreachable chain
    /// validates and runs with no LoRA).
    #[test]
    fn test_splice_puts_every_loader_on_the_model_path_in_order() {
        let mut wf = shipped("ace-step-1.5-turbo");
        let change = splice_loras(
            &mut wf,
            &ace_loras(),
            &[pick("a.safetensors", 1.0), pick("b.safetensors", 0.5)],
        )
        .expect("spliced");
        assert_eq!(change.nodes, ["112", "113"]);
        // 78 (ModelSamplingAuraFlow) consumed 104 before the splice.
        assert_eq!(model_chain(&wf, "78"), ["78", "113", "112", "104"]);
        assert_eq!(wf["112"]["inputs"]["lora_name"], "a.safetensors");
        assert_eq!(wf["113"]["inputs"]["strength_model"], 0.5);
        assert_eq!(wf["113"]["class_type"], "LoraLoaderModelOnly");
    }

    #[test]
    fn test_splice_of_an_empty_stack_changes_nothing() {
        let mut wf = shipped("ace-step-1.5-turbo");
        let before = wf.clone();
        assert!(splice_loras(&mut wf, &ace_loras(), &[])
            .expect("ok")
            .nodes
            .is_empty());
        assert_eq!(wf, before);
    }

    #[test]
    fn test_splice_refuses_limits_and_missing_anchors() {
        let mut wf = shipped("ace-step-1.5-turbo");
        let five: Vec<_> = (0..5).map(|i| pick(&format!("{i}"), 1.0)).collect();
        assert!(matches!(
            splice_loras(&mut wf, &ace_loras(), &five),
            Err(GraphError::TooManyLoras { max: 4, got: 5 })
        ));
        assert!(matches!(
            splice_loras(&mut wf, &ace_loras(), &[pick("x", 3.0)]),
            Err(GraphError::StrengthOutOfRange { .. })
        ));
        let mut missing = ace_loras();
        missing.attach_after = "999".into();
        assert!(matches!(
            splice_loras(&mut wf, &missing, &[pick("x", 1.0)]),
            Err(GraphError::NoAttachPoint { .. })
        ));
    }

    /// Protects: an anchor whose MODEL feeds nothing is refused rather than
    /// given a dangling chain that runs with no LoRA applied.
    #[test]
    fn test_splice_refuses_an_anchor_nothing_consumes() {
        let mut wf = json!({"104": {"class_type": "UNETLoader", "inputs": {"unet_name": "x"}}});
        assert_eq!(
            splice_loras(&mut wf, &ace_loras(), &[pick("x", 1.0)]),
            Err(GraphError::NoModelConsumer { id: "104".into() })
        );
    }

    /// Protects: new ids clear subgraph-flattened ids too (`76:73`), so a
    /// splice into Qwen-Image's graph could never overwrite an existing node.
    #[test]
    fn test_splice_mints_ids_above_subgraph_ids() {
        let mut wf = shipped("qwen-image");
        let mut loras = ace_loras();
        loras.attach_after = "76:37".into();
        let change = splice_loras(&mut wf, &loras, &[pick("x", 1.0)]).expect("spliced");
        assert_eq!(change.nodes, ["77"]);
    }
}
