# T-106: node registry — `nodes(action="get")` for enum choices and LoRA enumeration
**Depends:** T-105b | **Crate/dir:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to create:** `crates/mcp-bridge/src/nodes.rs`

**Files to modify:** `crates/mcp-bridge/src/lib.rs`

> The last unwrapped tool on the comfy-mcp surface. `nodes(action="get")` reads one node
> class's schema from the live `object_info` — the source for `from_node_choices` enums
> (keyscale/language/timesignature) and for LoRA enumeration (`lora_name.choices`).

## Goal
A `NodeSchema` type modelling the full `nodes(action="get")` response, a `node_schema(class)`
wrapper on `LocalComfy`, and a `choices_for(name)` helper that reads a COMBO input's live
`choices`. This is the primitive the param panel (T-107+) and the LoRA picker (Phase 3) both
build on.

## Verified, not recalled
Shape captured live 2026-08-24 against the running server — recorded in **docs/MCP-SURFACE.md
§12**. The reference code compiles, is `cargo fmt`- and `clippy -D warnings`-clean, and all 79
scratch tests pass (5 new).

Three facts the types encode:
- ⚠ **`options` is polymorphic.** `default` is a string (`"en"`), a bool (`true`), a number
  (`0`, `120.0`), or `null`; `min`/`max`/`step` are numbers or `null`. The `INT` seed's `max`
  is `18446744073709551615` = `u64::MAX`, which does **not** fit in `i64` — so `NodeOptions`
  fields are `Option<Value>`, never `f64`/`i64` (the same precision rule that made `Seed` its
  own profile type).
- **`choices` is non-empty only for `COMBO` inputs** — it is the live enum. `is_link: true`
  marks a graph edge (e.g. `MODEL`), `is_link: false` a widget.
- **Unknown class** fails with `[node_not_found]` — the usual `Ok(is_error: true)` shape, so
  the wrapper surfaces it as `ComfyError::Tool` with that code, not a decode error.

## Reference code

### `crates/mcp-bridge/src/nodes.rs` — full file
```rust
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
        assert_eq!(
            log[0]["arguments"]["name"],
            json!("LoraLoaderModelOnly")
        );
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
            Some(&["a\\adapter_model.safetensors".to_string(),
                   "b\\adapter_model.safetensors".to_string()][..])
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
```

### `crates/mcp-bridge/src/lib.rs`
Add the module (alphabetical, between `models` and `preflight`) and the re-export:
```rust
mod nodes;
```
```rust
pub use nodes::{NodeInput, NodeOptions, NodeOutput, NodeSchema};
```

## Tests
Five new tests in `nodes.rs`. Per test, the invariant:

- `test_node_schema_sends_action_get_and_name` — **protects:** the argument set — `action="get"`
  and `name` go out verbatim. A misspelled argument is rejected outright (MCP-SURFACE §8.7).
- `test_node_schema_decodes_the_full_shape` — **protects:** the full decode — metadata, inputs
  with `type`/`choices`/`options`, and outputs, against the real `LoraLoaderModelOnly` shape.
- `test_choices_for_reads_a_combo_input` — **protects:** the enum/LoRA primitive — `choices_for`
  reads a COMBO's choices, returns empty for a non-COMBO input, and `None` for an absent input.
- `test_options_default_is_polymorphic` — **protects:** the polymorphic `options` — a `u64::MAX`
  seed max and a string/bool default decode without truncation or error.
- `test_unknown_node_is_a_tool_error` — **protects:** the `is_error` trap — an unknown class
  surfaces as `ComfyError::Tool` with code `node_not_found`, not a decode error.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All five named tests present and passing; the pre-existing 74 tests still pass
- [ ] No test spawns a process, opens a socket, or reaches the network
- [ ] No changes outside the two listed files
- [ ] No new dependencies

## Out of scope
The LoRA list **filtering/grouping** (drop `training_state.pt` and non-adapters, group by
directory, collapse epoch series, dedupe case variants) — that is Phase 3's LoRA stack panel
(ROADMAP Phase 3, ARCHITECTURE §5a). This task delivers the **raw** list. `nodes(action="search"/
"list"/"upstream"/"downstream"/"path"/"types"/"categories")` — only `get` is needed now. Any
frontend or Tauri command. The `from_node_choices` *consumer* (T-107's profile loader).

## Notes for the executor
- `NodeOptions` derives `Default` — required because `NodeInput.options` is `#[serde(default)]`.
- The `u64::MAX` literal in the test needs the `u64` suffix (`18446744073709551615u64`) or it
  fails to compile as an overflowing `i32` literal.
- `choices_for` returns `Option<&[String]>`; a non-COMBO input yields `Some(&[])`, an absent
  input `None` — the caller distinguishes "no such input" from "input with no choices".
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
`error.rs`, `local.rs` and `mock.rs` are `--read`: the module does `impl LocalComfy` and returns
`ComfyError`, and the tests use `client_and_log`/`Reply`.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/error.rs --read crates/mcp-bridge/src/local.rs --read crates/mcp-bridge/src/mock.rs --file crates/mcp-bridge/src/nodes.rs --file crates/mcp-bridge/src/lib.rs
```
