//! Node registry: `nodes(action="get")` -- the live node schema.
//!
//! Shapes verified live 2026-08-24 -- docs/MCP-SURFACE.md 12.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// One input on a node class.
///
/// `is_link: true` marks a linkable input (a graph edge, e.g. `MODEL`);
/// `is_link: false` marks a widget (a value the user sets). `choices` is
/// non-empty only for `COMBO` inputs -- it is the live enum, and the source
/// for `from_node_choices` and for LoRA enumeration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInput {
    /// Input name, e.g. `lora_name`.
    pub name: String,
    /// ComfyUI input type: `MODEL`, `COMBO`, `FLOAT`, `INT`, `STRING`,
    /// `BOOLEAN`, `CONDITIONING`, `CLIP`, `LATENT`, ...
    #[serde(rename = "type")]
    pub ty: String,
    /// Whether the input must be connected/set.
    #[serde(default)]
    pub required: bool,
    /// True for a graph edge, false for a widget.
    #[serde(default)]
    pub is_link: bool,
    /// `"required"` or `"optional"`.
    #[serde(default)]
    pub section: String,
    /// The live enum for a `COMBO` input; empty otherwise.
    #[serde(default)]
    pub choices: Vec<String>,
    /// Numeric bounds / default. Polymorphic -- see [`NodeOptions`].
    #[serde(default)]
    pub options: NodeOptions,
}

/// A node input's bounds and default.
///
/// Every field is `Option<Value>` because the payload is polymorphic:
/// `default` is a string (`"en"`), a bool (`true`), a number (`0`, `120.0`),
/// or `null`; `min`/`max`/`step` are numbers or `null`. The `INT` seed's
/// `max` is `18446744073709551615` = `u64::MAX`, which does not fit in `i64`
/// -- so these are kept as `Value`, never `f64`/`i64` (the same precision
/// rule that made `Seed` its own profile type).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeOptions {
    #[serde(default)]
    pub min: Option<Value>,
    #[serde(default)]
    pub max: Option<Value>,
    #[serde(default)]
    pub step: Option<Value>,
    #[serde(default)]
    pub default: Option<Value>,
}

/// One output on a node class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeOutput {
    /// Output name, e.g. `MODEL`.
    pub name: String,
    /// ComfyUI output type.
    #[serde(rename = "type")]
    pub ty: String,
}

/// The full schema of one node class, from the live `object_info`.
///
/// This is the authoritative list of what a graph will accept -- the same
/// source `validate_workflow` and `run_workflow`'s pre-validation read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSchema {
    /// Class name, e.g. `LoraLoaderModelOnly`.
    pub id: String,
    /// Class name again (comfy-cli repeats it).
    pub name: String,
    /// Human title, e.g. `Load LoRA`.
    #[serde(default)]
    pub display_name: String,
    /// May be empty.
    #[serde(default)]
    pub description: String,
    /// Category path, e.g. `model/loaders`.
    #[serde(default)]
    pub category: String,
    /// Output connection types, e.g. `["MODEL"]`.
    #[serde(default)]
    pub output_types: Vec<String>,
    /// Whether this node is a graph output.
    #[serde(default)]
    pub output_node: bool,
    /// Whether this node calls a partner API.
    #[serde(default)]
    pub is_api_node: bool,
    /// Whether the class is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// The node pack that provides it (`core` for built-ins).
    #[serde(default)]
    pub pack: String,
    /// Extra labels; `[]` on core nodes.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Whether the class is disabled on cloud.
    #[serde(default)]
    pub cloud_disabled: bool,
    /// The inputs, in order.
    #[serde(default)]
    pub inputs: Vec<NodeInput>,
    /// The outputs.
    #[serde(default)]
    pub outputs: Vec<NodeOutput>,
    /// Whether comfy-cli answered from its cache rather than from ComfyUI.
    ///
    /// **Tri-state on purpose, like `local_check` (MCP-SURFACE 6).**
    /// `Some(true)` is a schema comfy-cli served from its own `object_info`
    /// cache because it could not reach the server; `Some(false)` is a live
    /// read; `None` means the response did not say, which is **not** the same
    /// as fresh and must never be shown as fresh.
    ///
    /// This matters because `nodes(action="get")` **succeeds with ComfyUI
    /// down** -- verified 2026-08-28, the whole 53-entry LoRA list came back
    /// from cache with `stale: true`. A caller that ignores this presents a
    /// cached `lora_name` list as the installed one: LoRAs the user deleted
    /// are still offered, ones they just added are missing, and picking a
    /// deleted one does not fail -- ComfyUI warns on unmatched keys and
    /// completes, writing a track with no LoRA on it (MCP-SURFACE 17.6).
    #[serde(default)]
    pub stale: Option<bool>,
    /// Envelope warnings, e.g. `object_info_stale` with the reason.
    ///
    /// Carried rather than dropped: the warning is the only place the reason
    /// appears ("cannot reach http://127.0.0.1:8188/object_info"), and a UI
    /// that has to explain why a list may be wrong needs it.
    #[serde(default)]
    pub warnings: Vec<NodeWarning>,
}

/// One envelope warning attached to a node schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeWarning {
    /// Machine code, e.g. `object_info_stale`.
    #[serde(default)]
    pub code: String,
    /// Human text, including the reason the live read failed.
    #[serde(default)]
    pub message: String,
}

/// The warning code comfy-cli attaches when it answers from its cache.
pub const OBJECT_INFO_STALE: &str = "object_info_stale";

impl NodeSchema {
    /// Whether this schema came from comfy-cli's cache rather than ComfyUI.
    ///
    /// **Both signals, because the live shape omits `stale` entirely.**
    /// Observed 2026-08-28 by running it both ways: with ComfyUI down the
    /// response carries `stale: true` *and* an [`OBJECT_INFO_STALE`] warning;
    /// with ComfyUI up it carries **neither** -- there is no `stale: false`.
    ///
    /// So absence is now evidence, not an assumption, and the earlier
    /// tri-state reading (`None` is not fresh) has to go: it warned on every
    /// healthy install, and a caution that is always on is one nobody reads by
    /// the time it matters. The warning is still the more reliable of the two
    /// signals -- it carries the reason -- so a cache that stopped setting the
    /// flag would still be caught.
    pub fn is_cached(&self) -> bool {
        self.stale == Some(true) || self.warnings.iter().any(|w| w.code == OBJECT_INFO_STALE)
    }

    /// Why the live read failed, when it did.
    pub fn cache_reason(&self) -> Option<&str> {
        self.warnings
            .iter()
            .find(|w| w.code == OBJECT_INFO_STALE)
            .map(|w| w.message.as_str())
    }
}

impl NodeSchema {
    /// Find one input by name.
    pub fn input(&self, name: &str) -> Option<&NodeInput> {
        self.inputs.iter().find(|i| i.name == name)
    }

    /// The live choices for a named input, or `None` when the input is absent.
    ///
    /// A non-`COMBO` input has empty `choices`, so a caller reading an enum
    /// should also check `is_empty()` before presenting options.
    pub fn choices_for(&self, name: &str) -> Option<&[String]> {
        self.input(name).map(|i| i.choices.as_slice())
    }
}

impl LocalComfy {
    /// Read one node class's schema from the live registry.
    pub async fn node_schema(&self, class: &str) -> Result<NodeSchema, ComfyError> {
        let mut args = Map::new();
        args.insert("action".into(), Value::String("get".into()));
        args.insert("name".into(), Value::String(class.to_string()));
        self.call("nodes", args).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;
    use crate::nodes::NodeSchema;

    /// Protects: a cached answer is recognisable as one.
    ///
    /// `nodes(action="get")` **succeeds with ComfyUI down** -- comfy-cli serves
    /// its own `object_info` cache and flags it. Verified 2026-08-28 against
    /// this exact payload, which is the fixture T-307 was built on: the entire
    /// 53-entry LoRA list came back with `stale: true` and an
    /// `object_info_stale` warning naming the connection that failed.
    ///
    /// Dropping those two fields is what makes the failure silent. A picker
    /// filled from a cache offers LoRAs the user has deleted and hides the one
    /// they just trained, and choosing a deleted one does not error -- ComfyUI
    /// warns about unmatched keys and finishes, writing a track with no LoRA
    /// and nothing anywhere saying so (MCP-SURFACE 17.6).
    #[tokio::test]
    async fn test_a_cached_schema_says_it_is_cached() {
        let captured: serde_json::Value = serde_json::from_str(include_str!(
            "../../../testdata/mcp/nodes.LoraLoaderModelOnly.json"
        ))
        .expect("the captured node schema decodes");

        let (client, _recorded) = client_and_log(vec![Reply::Json(captured)]).await;
        let schema: NodeSchema = client
            .node_schema("LoraLoaderModelOnly")
            .await
            .expect("schema");

        assert_eq!(schema.stale, Some(true));
        assert!(schema.is_cached());
        assert!(schema.cache_reason().is_some());
        assert_eq!(schema.warnings.len(), 1);
        assert_eq!(schema.warnings[0].code, "object_info_stale");
        assert!(schema.warnings[0].message.contains("cannot reach"));
        assert_eq!(
            schema.choices_for("lora_name").map(<[String]>::len),
            Some(53),
            "the cache still answers in full, which is exactly why it is easy to trust"
        );
    }

    /// Protects: a live read is recognised as live.
    ///
    /// **This is the shape a running ComfyUI returns** -- no `stale` key and no
    /// warning, confirmed 2026-08-28 by running the panel both ways. There is
    /// no `stale: false`, so a reading that only trusted an explicit `false`
    /// warned on every healthy install; the earlier version of this test
    /// asserted exactly that behaviour and was wrong about the world, not
    /// about the code.
    #[tokio::test]
    async fn test_a_live_read_carries_neither_signal_and_is_not_cached() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "id": "LoraLoaderModelOnly", "name": "LoraLoaderModelOnly"
        }))])
        .await;

        let schema: NodeSchema = client
            .node_schema("LoraLoaderModelOnly")
            .await
            .expect("schema");

        assert_eq!(schema.stale, None);
        assert!(schema.warnings.is_empty());
        assert!(!schema.is_cached());
        assert_eq!(schema.cache_reason(), None);
    }

    /// Protects: the flag alone is enough.
    ///
    /// The symmetric case, and the one that was missing: both signals arrive
    /// together on every response observed so far, so a reading that consulted
    /// only the warning passed the whole suite. Either signal on its own means
    /// cached, and neither means live.
    #[tokio::test]
    async fn test_the_stale_flag_alone_marks_a_cached_schema() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "id": "X", "name": "X", "stale": true
        }))])
        .await;

        let schema: NodeSchema = client.node_schema("X").await.expect("schema");

        assert!(schema.warnings.is_empty());
        assert!(schema.is_cached());
        assert_eq!(schema.cache_reason(), None);
    }

    /// Protects: the warning alone is enough.
    ///
    /// The two signals arrive together today. The warning carries the reason
    /// and is the harder of the two to drop by accident, so a cache that
    /// stopped setting the flag must still be caught.
    #[tokio::test]
    async fn test_the_stale_warning_alone_marks_a_cached_schema() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "id": "X", "name": "X",
            "warnings": [{ "code": "object_info_stale", "message": "served from cache" }]
        }))])
        .await;

        let schema: NodeSchema = client.node_schema("X").await.expect("schema");

        assert_eq!(schema.stale, None);
        assert!(schema.is_cached());
    }

    /// Protects: the argument set -- `action="get"` and `name` go out verbatim.
    /// comfy-mcp rejects a misspelled argument outright (MCP-SURFACE 8.7).
    #[tokio::test]
    async fn test_node_schema_sends_action_get_and_name() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "id": "LoraLoaderModelOnly", "name": "LoraLoaderModelOnly"
        }))])
        .await;

        let _: NodeSchema = client
            .node_schema("LoraLoaderModelOnly")
            .await
            .expect("schema");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("nodes"));
        assert_eq!(log[0]["arguments"]["action"], json!("get"));
        assert_eq!(log[0]["arguments"]["name"], json!("LoraLoaderModelOnly"));
    }

    /// Protects: the full decode -- metadata, inputs with `type`/`choices`/
    /// `options`, and outputs. Uses the real `LoraLoaderModelOnly` shape.
    #[tokio::test]
    async fn test_node_schema_decodes_the_full_shape() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "id": "LoraLoaderModelOnly",
            "name": "LoraLoaderModelOnly",
            "display_name": "Load LoRA",
            "description": "This LoRAs loader is used to modify the diffusion model",
            "category": "model/loaders",
            "output_types": ["MODEL"],
            "output_node": false,
            "is_api_node": false,
            "deprecated": false,
            "pack": "core",
            "labels": [],
            "cloud_disabled": false,
            "inputs": [
                { "name": "model", "type": "MODEL", "required": true, "is_link": true,
                  "section": "required", "choices": [],
                  "options": { "min": null, "max": null, "step": null, "default": null } },
                { "name": "lora_name", "type": "COMBO", "required": true, "is_link": false,
                  "section": "required",
                  "choices": [
                    "ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors",
                    "ACE-Step-v1.5-chinese-new-year-LoRA\\adapter_model.safetensors"
                  ],
                  "options": { "min": null, "max": null, "step": null, "default": null } },
                { "name": "strength_model", "type": "FLOAT", "required": true, "is_link": false,
                  "section": "required", "choices": [],
                  "options": { "min": -100.0, "max": 100.0, "step": 0.01, "default": 1.0 } }
            ],
            "outputs": [ { "name": "MODEL", "type": "MODEL" } ]
        }))])
        .await;

        let schema: NodeSchema = client
            .node_schema("LoraLoaderModelOnly")
            .await
            .expect("schema");
        assert_eq!(schema.id, "LoraLoaderModelOnly");
        assert_eq!(schema.display_name, "Load LoRA");
        assert_eq!(schema.pack, "core");
        assert_eq!(schema.output_types, vec!["MODEL"]);
        assert_eq!(schema.inputs.len(), 3);
        assert_eq!(schema.outputs.len(), 1);
        assert_eq!(schema.outputs[0].ty, "MODEL");

        let lora = schema.input("lora_name").expect("lora_name input");
        assert_eq!(lora.ty, "COMBO");
        assert!(!lora.is_link);
        assert_eq!(lora.choices.len(), 2);
        assert!(lora.choices[0].contains('\\'));

        let strength = schema.input("strength_model").expect("strength input");
        assert_eq!(strength.ty, "FLOAT");
        assert_eq!(strength.options.default, Some(json!(1.0)));
    }

    /// Protects: the enum/LoRA primitive -- `choices_for` reads a COMBO's
    /// choices and returns `None` for an absent input.
    #[tokio::test]
    async fn test_choices_for_reads_a_combo_input() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "id": "LoraLoaderModelOnly", "name": "LoraLoaderModelOnly",
            "inputs": [
                { "name": "model", "type": "MODEL", "choices": [] },
                { "name": "lora_name", "type": "COMBO",
                  "choices": ["a\\adapter_model.safetensors", "b\\adapter_model.safetensors"] }
            ]
        }))])
        .await;

        let schema: NodeSchema = client
            .node_schema("LoraLoaderModelOnly")
            .await
            .expect("schema");
        assert_eq!(
            schema.choices_for("lora_name"),
            Some(
                &[
                    "a\\adapter_model.safetensors".to_string(),
                    "b\\adapter_model.safetensors".to_string()
                ][..]
            )
        );
        assert_eq!(schema.choices_for("model"), Some(&[][..]));
        assert_eq!(schema.choices_for("nope"), None);
    }

    /// Protects: the polymorphic `options` -- a `u64::MAX` seed max and a
    /// string/bool default must decode without truncation or error.
    #[tokio::test]
    async fn test_options_default_is_polymorphic() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "id": "TextEncodeAceStepAudio1.5", "name": "TextEncodeAceStepAudio1.5",
            "inputs": [
                { "name": "seed", "type": "INT",
                  "options": { "min": 0, "max": 18446744073709551615u64, "step": null, "default": 0 } },
                { "name": "language", "type": "COMBO", "choices": ["en", "zh"],
                  "options": { "min": null, "max": null, "step": null, "default": "en" } },
                { "name": "generate_audio_codes", "type": "BOOLEAN",
                  "options": { "min": null, "max": null, "step": null, "default": true } }
            ]
        }))])
        .await;

        let schema: NodeSchema = client
            .node_schema("TextEncodeAceStepAudio1.5")
            .await
            .expect("schema");

        let seed = schema.input("seed").expect("seed input");
        assert_eq!(seed.options.max, Some(json!(18446744073709551615u64)));

        let language = schema.input("language").expect("language input");
        assert_eq!(language.options.default, Some(json!("en")));

        let codes = schema.input("generate_audio_codes").expect("codes input");
        assert_eq!(codes.options.default, Some(json!(true)));
    }

    /// Protects: an unknown class surfaces as `ComfyError::Tool` with the
    /// `node_not_found` code, not a decode error -- the `is_error` trap.
    #[tokio::test]
    async fn test_unknown_node_is_a_tool_error() {
        let (client, _recorded) = client_and_log(vec![Reply::ToolError(
            "comfy nodes show DefinitelyNotARealNode failed [node_not_found]: \
             Node class 'DefinitelyNotARealNode' not found in the loaded environment."
                .into(),
        )])
        .await;

        let err = client
            .node_schema("DefinitelyNotARealNode")
            .await
            .expect_err("unknown class must be a tool error");

        match err {
            crate::ComfyError::Tool { tool, code, .. } => {
                assert_eq!(tool, "nodes");
                assert_eq!(code.as_deref(), Some("node_not_found"));
            }
            other => panic!("expected ComfyError::Tool, got {:?}", other),
        }
    }
}
