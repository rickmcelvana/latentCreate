//! Typed views of ComfyUI's responses (COMFY-HTTP-SURFACE sections 2-7).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `GET /system_stats`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemStats {
    /// Version and platform.
    #[serde(default)]
    pub system: SystemInfo,
    /// GPUs (and CPU) ComfyUI can run on.
    #[serde(default)]
    pub devices: Vec<Device>,
}

/// The `system` block. `argv` is deliberately not modelled: it carries absolute
/// paths into the user's profile and must not reach a sidecar or a log.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemInfo {
    /// `"win32"`, `"linux"`, `"darwin"`.
    #[serde(default)]
    pub os: Option<String>,
    /// `"0.45.2"`.
    #[serde(default)]
    pub comfyui_version: Option<String>,
}

/// One compute device.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Device {
    /// `"cuda:0 NVIDIA GeForce RTX 5060 Ti : cudaMallocAsync"`.
    #[serde(default)]
    pub name: String,
    /// `"cuda"`, `"cpu"`, `"mps"`.
    #[serde(default, rename = "type")]
    pub kind: String,
    /// Total VRAM in bytes.
    #[serde(default)]
    pub vram_total: Option<u64>,
}

impl SystemStats {
    /// Total VRAM of the first device, in bytes. `None` when ComfyUI reports no
    /// device, which the UI shows as unknown rather than zero.
    pub fn vram_bytes(&self) -> Option<u64> {
        self.devices.first().and_then(|d| d.vram_total)
    }
}

/// One input of a node class, from `/object_info/<class>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeInput {
    /// The input name as it appears in a workflow's `inputs`.
    pub name: String,
    /// `"INT"`, `"FLOAT"`, `"STRING"`, `"BOOLEAN"`, `"COMBO"`,
    /// `"COMFY_DYNAMICCOMBO_V3"`, or a link type such as `"MODEL"`.
    pub ty: String,
    /// Declared under `required` rather than `optional`.
    pub required: bool,
    /// The input takes a connection from another node, not a value.
    pub is_link: bool,
    /// The options a combo accepts (for a dynamic combo, its keys).
    pub choices: Vec<String>,
    /// `min`/`max`/`step`/`default`, as ComfyUI declared them.
    pub options: NodeOptions,
}

/// Numeric bounds and the default, kept as raw JSON so a `u64` seed maximum is
/// not squeezed through `f64`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NodeOptions {
    /// Lower bound.
    pub min: Option<Value>,
    /// Upper bound.
    pub max: Option<Value>,
    /// Increment.
    pub step: Option<Value>,
    /// The node's default.
    pub default: Option<Value>,
}

/// A node class's schema.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeSchema {
    /// The class name (`"KSampler"`).
    pub name: String,
    /// Its label in the ComfyUI UI.
    pub display_name: String,
    /// Writes results (a save node).
    pub output_node: bool,
    /// Inputs in ComfyUI's declared order.
    pub inputs: Vec<NodeInput>,
    /// Output types, in slot order.
    pub output_types: Vec<String>,
}

/// Value types that are written, not connected.
const VALUE_TYPES: [&str; 6] = [
    "INT",
    "FLOAT",
    "STRING",
    "BOOLEAN",
    "COMBO",
    "COMFY_DYNAMICCOMBO_V3",
];

impl NodeSchema {
    /// Parse one class's entry from `/object_info/<class>` (the value under the
    /// class key). Handles both combo shapes (COMFY-HTTP-SURFACE 4).
    pub fn from_object_info(name: &str, entry: &Value) -> Option<Self> {
        let input = entry.get("input")?;
        let order = entry.get("input_order");
        let mut inputs = Vec::new();
        for (section, required) in [("required", true), ("optional", false)] {
            let Some(specs) = input.get(section).and_then(Value::as_object) else {
                continue;
            };
            let names: Vec<String> = order
                .and_then(|o| o.get(section))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_else(|| specs.keys().cloned().collect());
            for input_name in names {
                if let Some(spec) = specs.get(&input_name) {
                    inputs.push(parse_input(&input_name, spec, required));
                }
            }
        }
        Some(NodeSchema {
            name: name.to_string(),
            display_name: entry
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(name)
                .to_string(),
            output_node: entry
                .get("output_node")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            inputs,
            output_types: entry
                .get("output")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// The named input.
    pub fn input(&self, name: &str) -> Option<&NodeInput> {
        self.inputs.iter().find(|i| i.name == name)
    }

    /// The options a combo input accepts.
    pub fn choices_for(&self, name: &str) -> Option<&[String]> {
        self.input(name).map(|i| i.choices.as_slice())
    }
}

fn parse_input(name: &str, spec: &Value, required: bool) -> NodeInput {
    let head = spec.get(0);
    let opts = spec.get(1);
    let strings = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| match x {
                        Value::String(s) => Some(s.clone()),
                        Value::Object(o) => {
                            o.get("key").and_then(Value::as_str).map(str::to_string)
                        }
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let (ty, choices) = match head {
        // Legacy combo: the option list itself sits where the type would.
        Some(Value::Array(_)) => ("COMBO".to_string(), strings(head)),
        Some(Value::String(t)) => {
            let choices = if t == "COMBO" || t == "COMFY_DYNAMICCOMBO_V3" {
                strings(opts.and_then(|o| o.get("options")))
            } else {
                Vec::new()
            };
            (t.clone(), choices)
        }
        _ => ("UNKNOWN".to_string(), Vec::new()),
    };
    let field = |k: &str| opts.and_then(|o| o.get(k)).cloned();
    NodeInput {
        name: name.to_string(),
        is_link: !VALUE_TYPES.contains(&ty.as_str()) && ty != "UNKNOWN",
        ty,
        required,
        choices,
        options: NodeOptions {
            min: field("min"),
            max: field("max"),
            step: field("step"),
            default: field("default"),
        },
    }
}

/// `GET /queue`, reduced to the prompt ids in each state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueueState {
    /// Executing now (at most one on a single-GPU install).
    pub running: Vec<String>,
    /// Waiting, in queue order.
    pub pending: Vec<String>,
}

impl QueueState {
    /// Parse the raw body. Each item is `[number, prompt_id, graph, extra,
    /// outputs]`; only index 1 is read.
    pub fn from_value(value: &Value) -> Option<Self> {
        let ids = |key: &str| -> Option<Vec<String>> {
            Some(
                value
                    .get(key)?
                    .as_array()?
                    .iter()
                    .filter_map(|item| item.get(1).and_then(Value::as_str).map(str::to_string))
                    .collect(),
            )
        };
        Some(QueueState {
            running: ids("queue_running")?,
            pending: ids("queue_pending")?,
        })
    }

    /// Where this prompt is in the queue, if it is there at all.
    pub fn position(&self, prompt_id: &str) -> Option<QueuePosition> {
        if self.running.iter().any(|id| id == prompt_id) {
            Some(QueuePosition::Running)
        } else if self.pending.iter().any(|id| id == prompt_id) {
            Some(QueuePosition::Pending)
        } else {
            None
        }
    }
}

/// Where a prompt sits in `/queue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePosition {
    /// Executing.
    Running,
    /// Waiting behind another job.
    Pending,
}

/// One finished job, from `GET /history/<id>`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HistoryRecord {
    /// How it ended.
    #[serde(default)]
    pub status: HistoryStatus,
    /// Per save node: `{"images": [...]}` or `{"audio": [...]}`.
    #[serde(default)]
    pub outputs: BTreeMap<String, Value>,
}

/// The `status` block of a history record.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct HistoryStatus {
    /// `"success"` or `"error"`.
    #[serde(default)]
    pub status_str: String,
    /// `true` only on success.
    #[serde(default)]
    pub completed: bool,
    /// `[event, payload]` pairs: `execution_start`, `execution_cached`,
    /// `execution_success`, `execution_error`, `execution_interrupted`.
    #[serde(default)]
    pub messages: Vec<(String, Value)>,
}

/// How a finished job ended, read from its history record.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Ran to completion.
    Success,
    /// Stopped by `/interrupt`.
    Interrupted,
    /// A node raised.
    Failed {
        /// The failing node's class, when reported.
        node_type: Option<String>,
        /// ComfyUI's `exception_message`, trimmed. Never the traceback.
        message: Option<String>,
    },
}

impl HistoryRecord {
    /// Classify the record. The message list is authoritative; `status_str` is
    /// the fallback for a record with no recognisable event.
    pub fn outcome(&self) -> Outcome {
        for (event, payload) in &self.status.messages {
            match event.as_str() {
                "execution_interrupted" => return Outcome::Interrupted,
                "execution_error" => {
                    let text = |k: &str| {
                        payload
                            .get(k)
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                    };
                    return Outcome::Failed {
                        node_type: text("node_type"),
                        message: text("exception_message"),
                    };
                }
                _ => {}
            }
        }
        if self.status.status_str == "success" || self.status.completed {
            Outcome::Success
        } else {
            Outcome::Failed {
                node_type: None,
                message: None,
            }
        }
    }

    /// Every saved file, skipping `type: "temp"` previews. Ordered by node id,
    /// then by position in that node's list.
    pub fn output_files(&self) -> Vec<OutputRef> {
        let mut files = Vec::new();
        for (node_id, node) in &self.outputs {
            let Some(map) = node.as_object() else {
                continue;
            };
            for (kind, items) in map {
                let Some(items) = items.as_array() else {
                    continue;
                };
                for item in items {
                    let Some(filename) = item.get("filename").and_then(Value::as_str) else {
                        continue;
                    };
                    let file_type = item.get("type").and_then(Value::as_str).unwrap_or("output");
                    if file_type != "output" {
                        continue;
                    }
                    files.push(OutputRef {
                        node_id: node_id.clone(),
                        kind: kind.clone(),
                        filename: filename.to_string(),
                        subfolder: item
                            .get("subfolder")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        file_type: file_type.to_string(),
                    });
                }
            }
        }
        files
    }
}

/// One output file ComfyUI wrote, addressable through `/view`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputRef {
    /// The save node that wrote it.
    pub node_id: String,
    /// `"images"` or `"audio"` (the key it was listed under).
    pub kind: String,
    /// `"ace_00001.flac"`.
    pub filename: String,
    /// Relative to ComfyUI's output directory; may be empty.
    pub subfolder: String,
    /// Always `"output"` here; temp previews are filtered out.
    pub file_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/../../testdata/comfy-http/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("JSON")
    }

    fn record(name: &str) -> HistoryRecord {
        let value = fixture(name);
        let (_, rec) = value
            .as_object()
            .expect("object")
            .iter()
            .next()
            .expect("one record");
        serde_json::from_value(rec.clone()).expect("history record")
    }

    #[test]
    fn test_system_stats_reads_version_and_vram() {
        let stats: SystemStats =
            serde_json::from_value(fixture("system_stats.json")).expect("decodes");
        assert_eq!(stats.system.comfyui_version.as_deref(), Some("0.45.2"));
        assert_eq!(stats.vram_bytes(), Some(17_074_749_440));
    }

    /// Protects: both combo shapes yield choices, a link input is marked as one,
    /// and a u64 seed maximum survives as an exact JSON number.
    #[test]
    fn test_node_schema_parses_both_combo_shapes_and_links() {
        let info = fixture("object_info_subset.json");
        let ks = NodeSchema::from_object_info("KSampler", &info["KSampler"]).expect("parses");
        assert_eq!(ks.inputs[0].name, "model");
        assert!(ks.input("model").expect("model").is_link);
        let seed = ks.input("seed").expect("seed");
        assert!(!seed.is_link);
        assert_eq!(
            seed.options.max,
            Some(serde_json::json!(18_446_744_073_709_551_615u64))
        );
        assert!(ks
            .choices_for("sampler_name")
            .expect("combo")
            .contains(&"euler".to_string()));

        let save = NodeSchema::from_object_info("SaveAudioAdvanced", &info["SaveAudioAdvanced"])
            .expect("parses");
        assert_eq!(
            save.choices_for("format").expect("dyn combo"),
            ["flac", "mp3", "opus"]
        );
        assert!(save.output_node);
    }

    #[test]
    fn test_queue_state_reads_prompt_ids() {
        let q = QueueState::from_value(&fixture("queue_running.json")).expect("parses");
        assert_eq!(q.running.len(), 1);
        assert_eq!(
            q.position(&q.running[0].clone()),
            Some(QueuePosition::Running)
        );
        assert_eq!(q.position("nope"), None);
    }

    #[test]
    fn test_history_success_lists_its_image() {
        let rec = record("history_success.json");
        assert_eq!(rec.outcome(), Outcome::Success);
        let files = rec.output_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, "images");
        assert_eq!(files[0].subfolder, "latentcreate_probe");
    }

    #[test]
    fn test_history_audio_lists_its_flac() {
        let files = record("history_audio_success.json").output_files();
        assert_eq!(files[0].kind, "audio");
        assert!(files[0].filename.ends_with(".flac"));
    }

    /// Protects: a node failure names the node class and ComfyUI's message --
    /// and never the traceback.
    #[test]
    fn test_history_node_error_names_node_and_message() {
        match record("history_node_error.json").outcome() {
            Outcome::Failed { node_type, message } => {
                assert_eq!(node_type.as_deref(), Some("LoraLoaderModelOnly"));
                let message = message.expect("message");
                assert!(
                    message.starts_with("The safetensors header is incomplete"),
                    "{message}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// Protects: an interrupt reports `status_str: "error"`, and must still read
    /// as a cancel, not a failure.
    #[test]
    fn test_history_interrupt_is_not_a_failure() {
        let rec = record("history_interrupted.json");
        assert_eq!(rec.status.status_str, "error");
        assert_eq!(rec.outcome(), Outcome::Interrupted);
    }

    #[test]
    fn test_output_files_skip_temp_previews() {
        let rec: HistoryRecord = serde_json::from_value(serde_json::json!({
            "status": {"status_str": "success", "completed": true, "messages": []},
            "outputs": {"9": {"images": [
                {"filename": "a.png", "subfolder": "", "type": "temp"},
                {"filename": "b.png", "subfolder": "", "type": "output"}
            ]}}
        }))
        .expect("decodes");
        let names: Vec<_> = rec.output_files().into_iter().map(|f| f.filename).collect();
        assert_eq!(names, ["b.png"]);
    }
}
