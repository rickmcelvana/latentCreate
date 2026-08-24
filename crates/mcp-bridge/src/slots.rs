//! Slots: the parameter mechanism, read and write.
//!
//! Shapes and traps verified live 2026-08-24 -- docs/MCP-SURFACE.md 9.1.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// One agent-tweakable widget on a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    /// Stable address: flat `35.filename_prefix`, or subgraph
    /// `37/6.unet_name`. This is what `set_workflow_slot` takes.
    pub address: String,
    /// Input name -- the part after the last `.`.
    pub name: String,
    /// ComfyUI input type: `STRING`, `INT`, `FLOAT`, `COMBO`, `BOOLEAN`.
    #[serde(rename = "type")]
    pub ty: String,
    /// The value currently baked into the graph.
    pub current_value: Value,
    /// Node instance: `35`, or `37/6` inside a subgraph.
    pub instance_id: String,
    /// Node class, e.g. `UNETLoader`.
    pub node_type: String,
}

/// Every slot a workflow exposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotList {
    /// Workflow the slots were read from.
    #[serde(default)]
    pub workflow: Option<PathBuf>,
    /// comfy-cli's id for the workflow, e.g. `minimax_music3_int8`.
    #[serde(default)]
    pub id: Option<String>,
    /// The slots themselves.
    #[serde(default)]
    pub slots: Vec<Slot>,
}

impl SlotList {
    /// Find one slot by its exact address.
    pub fn get(&self, address: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.address == address)
    }

    /// Addresses in `wanted` that this workflow does not expose.
    ///
    /// A profile naming a slot the template no longer has is the drift T-107
    /// has to report; the gallery is cached with a 24 h TTL and does move.
    pub fn missing<'a>(&self, wanted: &[&'a str]) -> Vec<&'a str> {
        wanted
            .iter()
            .filter(|a| self.get(a).is_none())
            .copied()
            .collect()
    }
}

/// Split a slot address into `(instance_id, input_name)`.
///
/// Splits on the LAST `.`, because subgraph instance ids contain `/` but never
/// `.` -- 24 of the 25 slots in the MiniMax fixture are subgraph-form, so a
/// parser that splits on the first separator mishandles almost all of a real
/// workflow.
pub fn split_address(address: &str) -> Option<(&str, &str)> {
    let idx = address.rfind('.')?;
    let (instance, name) = (&address[..idx], &address[idx + 1..]);
    if instance.is_empty() || name.is_empty() {
        None
    } else {
        Some((instance, name))
    }
}

/// One parameter write.
///
/// Always the structured form. comfy-mcp also accepts `"addr=value"` strings,
/// but parses those as JSON and therefore COERCES -- a lyric or caption that
/// happens to read as `true` or `123` would be silently retyped
/// (docs/MCP-SURFACE.md 9.1).
#[derive(Debug, Clone, Serialize)]
pub struct SlotOverride {
    /// Target address, from [`Slot::address`].
    pub address: String,
    /// Value to write. Type is preserved exactly as given.
    pub value: Value,
}

impl SlotOverride {
    /// Build one override for `address`.
    pub fn new(address: impl Into<String>, value: Value) -> Self {
        Self {
            address: address.into(),
            value,
        }
    }
}

/// Result of a successful parameter write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotWrite {
    /// Addresses comfy-mcp confirms it applied.
    #[serde(default)]
    pub applied: Vec<String>,
    /// Non-fatal notes. Third-party content.
    #[serde(default)]
    pub warnings: Vec<Value>,
    /// File that was written. Absent when the call did not persist -- which
    /// for this wrapper means something is wrong, since it always sends
    /// `stdout: false`.
    #[serde(default)]
    pub wrote: Option<PathBuf>,
}

impl LocalComfy {
    /// Read every slot a workflow exposes.
    pub async fn list_slots(&self, workflow: &Path) -> Result<SlotList, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        self.call("list_workflow_slots", args).await
    }

    /// Write parameter values into a workflow file, in one atomic call.
    ///
    /// Sends `stdout: false`, without which comfy-mcp **returns** the modified
    /// workflow instead of saving it, reporting the addresses it applied while
    /// changing nothing on disk (docs/MCP-SURFACE.md 9.1).
    ///
    /// A bad address fails the whole batch and writes nothing, so the caller
    /// may send a complete parameter set and needs no partial-write recovery.
    pub async fn set_slots(
        &self,
        workflow: &Path,
        overrides: &[SlotOverride],
    ) -> Result<SlotWrite, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        args.insert(
            "overrides".into(),
            serde_json::to_value(overrides).map_err(|e| ComfyError::Payload {
                tool: "set_workflow_slot".to_string(),
                detail: e.to_string(),
            })?,
        );
        args.insert("stdout".into(), Value::Bool(false));

        let write: SlotWrite = self.call("set_workflow_slot", args).await?;
        confirm_persisted(&write, overrides)?;
        Ok(write)
    }
}

/// Reject a write that did not actually land.
///
/// Two ways it can fail quietly: no `wrote` path at all (the call did not
/// persist), or an address that never appears in `applied`. comfy-mcp reports
/// the latter with an empty `warnings`, so silence is not confirmation.
fn confirm_persisted(write: &SlotWrite, overrides: &[SlotOverride]) -> Result<(), ComfyError> {
    if write.wrote.is_none() {
        return Err(ComfyError::Tool {
            tool: "set_workflow_slot".to_string(),
            code: Some("not_persisted".to_string()),
            message: "the workflow was not written to disk".to_string(),
        });
    }
    let unapplied: Vec<&str> = overrides
        .iter()
        .map(|o| o.address.as_str())
        .filter(|a| !write.applied.iter().any(|done| done == a))
        .collect();
    if !unapplied.is_empty() {
        return Err(ComfyError::Tool {
            tool: "set_workflow_slot".to_string(),
            code: Some("not_applied".to_string()),
            message: format!("these addresses were not applied: {}", unapplied.join(", ")),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;
    use crate::slots::{split_address, SlotOverride};

    /// Protects: the shape the whole parameter panel reads.
    #[tokio::test]
    async fn test_slots_decode_from_the_captured_fixture() {
        let fixture = include_str!("../../../testdata/mcp/list_workflow_slots.minimax.json");
        let value: Value = serde_json::from_str(fixture).expect("fixture is valid JSON");
        let (client, _recorded) = client_and_log(vec![Reply::Json(value)]).await;

        let list = client
            .list_slots(std::path::Path::new("wf.json"))
            .await
            .expect("fixture decodes");
        assert_eq!(list.slots.len(), 25);

        let unet = list.get("37/6.unet_name").expect("unet slot present");
        assert_eq!(unet.ty, "COMBO");
        assert_eq!(
            unet.current_value,
            json!("minimax_music3_dit_int8_convrot.safetensors")
        );

        let prefix = list.get("35.filename_prefix").expect("prefix slot present");
        assert_eq!(prefix.ty, "STRING");
    }

    /// Protects: the case 24 of the 25 real slots are in.
    #[test]
    fn test_split_address_handles_both_forms() {
        assert_eq!(
            split_address("35.filename_prefix"),
            Some(("35", "filename_prefix"))
        );
        assert_eq!(split_address("37/6.unet_name"), Some(("37/6", "unet_name")));
        assert_eq!(split_address("nodot"), None);
    }

    /// Protects: T-107's drift check.
    #[tokio::test]
    async fn test_slot_list_reports_missing_addresses() {
        let fixture = include_str!("../../../testdata/mcp/list_workflow_slots.minimax.json");
        let value: Value = serde_json::from_str(fixture).expect("fixture is valid JSON");
        let (client, _recorded) = client_and_log(vec![Reply::Json(value)]).await;

        let list = client
            .list_slots(std::path::Path::new("wf.json"))
            .await
            .expect("fixture decodes");
        let missing = list.missing(&["37/6.unet_name", "94.tags"]);
        assert_eq!(missing, vec!["94.tags"]);
    }

    /// Protects: the whole write path.
    #[tokio::test]
    async fn test_set_slots_sends_stdout_false() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "applied": ["37/13.caption"],
            "warnings": [],
            "wrote": "wf.json"
        }))])
        .await;

        let _ = client
            .set_slots(
                std::path::Path::new("wf.json"),
                &[SlotOverride::new("37/13.caption", json!("x"))],
            )
            .await
            .expect("write succeeds");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["name"], json!("set_workflow_slot"));
        assert_eq!(log[0]["arguments"]["stdout"], json!(false));
    }

    /// Protects: user text against silent retyping.
    #[tokio::test]
    async fn test_set_slots_sends_structured_overrides_preserving_type() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "applied": ["37/13.caption"],
            "warnings": [],
            "wrote": "wf.json"
        }))])
        .await;

        let _ = client
            .set_slots(
                std::path::Path::new("wf.json"),
                &[SlotOverride::new("37/13.caption", json!("true"))],
            )
            .await
            .expect("write succeeds");

        let log = recorded.lock().expect("recorded calls");
        let first = &log[0]["arguments"]["overrides"][0];
        assert_eq!(first["address"], json!("37/13.caption"));
        assert_eq!(first["value"], json!("true"));
        assert!(first["value"].is_string());
    }

    /// Protects: trap 1 at runtime, not just at the call site.
    #[tokio::test]
    async fn test_set_slots_rejects_a_reply_that_did_not_persist() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "applied": ["37/13.caption"],
            "warnings": []
        }))])
        .await;

        let err = client
            .set_slots(
                std::path::Path::new("wf.json"),
                &[SlotOverride::new("37/13.caption", json!("x"))],
            )
            .await
            .expect_err("must reject a non-persisting reply");

        match err {
            crate::ComfyError::Tool { tool, code, .. } => {
                assert_eq!(tool, "set_workflow_slot");
                assert_eq!(code.as_deref(), Some("not_persisted"));
            }
            other => panic!("expected ComfyError::Tool, got {:?}", other),
        }
    }

    /// Protects: "silence is not confirmation".
    #[tokio::test]
    async fn test_set_slots_rejects_an_unapplied_address() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "applied": [],
            "warnings": [],
            "wrote": "wf.json"
        }))])
        .await;

        let err = client
            .set_slots(
                std::path::Path::new("wf.json"),
                &[SlotOverride::new("37/13.caption", json!("x"))],
            )
            .await
            .expect_err("must reject an unapplied address");

        match err {
            crate::ComfyError::Tool { tool, code, .. } => {
                assert_eq!(tool, "set_workflow_slot");
                assert_eq!(code.as_deref(), Some("not_applied"));
            }
            other => panic!("expected ComfyError::Tool, got {:?}", other),
        }
    }

    /// Protects: argument naming on a surface that rejects a misspelling outright.
    #[tokio::test]
    async fn test_list_slots_sends_workflow_path() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "workflow": "wf.json",
            "id": "x",
            "count": 0,
            "slots": []
        }))])
        .await;

        let _ = client
            .list_slots(std::path::Path::new("wf.json"))
            .await
            .expect("list succeeds");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["name"], json!("list_workflow_slots"));
        assert_eq!(log[0]["arguments"]["workflow_path"], json!("wf.json"));
    }
}
