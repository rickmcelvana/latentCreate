//! Whether a resolved slot write can reach the engine, and the graph reads the
//! role suggester needs. API-format workflows only (ARCHITECTURE 3a).
//!
//! In ComfyUI's API format every input is either a **literal** or a **link**
//! (`["109", 0]`). Frontend-only nodes have already been inlined as literals,
//! and backend primitives such as `PrimitiveInt` are real nodes, so the
//! question "does this write land?" has a complete answer: it lands exactly
//! when the input exists and holds a literal. The comfy-mcp era's trap --
//! `3.seed` accepted, persisted and ignored because `PrimitiveInt` 109 drives
//! it (MCP-SURFACE 18.1) -- is visible here as a link, and refused.
//!
//! Separate from [`crate::graph`] on purpose: that module *edits* a workflow,
//! this one asks questions about one (and performs the plain value writes).

use serde_json::Value;

/// One resolved address whose input another node drives.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkFed {
    /// The address as the profile writes it, e.g. `"3.seed"`.
    pub address: String,
    /// The id of the node supplying the value.
    pub source: String,
    /// That node's class, when the graph has it.
    pub source_class: Option<String>,
}

impl LinkFed {
    /// Whether writing this address would change nothing at execution time.
    /// Always true in API format; kept as a method so callers read as intent.
    pub fn is_inert(&self) -> bool {
        true
    }
}

/// What [`audit_slots`] found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlotAudit {
    /// Addresses whose input is a link: a write there would be ignored.
    pub link_fed: Vec<LinkFed>,
    /// Addresses naming a node or input the workflow does not have, or that
    /// are not `node.input` at all.
    pub missing: Vec<String>,
}

impl SlotAudit {
    /// Every address can be written and will be read.
    pub fn is_clean(&self) -> bool {
        self.link_fed.is_empty() && self.missing.is_empty()
    }
}

/// Split `node.input` at the **first** `.`. Node ids never contain one; input
/// names can (`111.format.quality`), and subgraph nodes carry `parent:child`
/// ids (`76:3.seed`).
pub fn split_address(address: &str) -> Option<(&str, &str)> {
    let (node, input) = address.split_once('.')?;
    if node.is_empty() || input.is_empty() {
        None
    } else {
        Some((node, input))
    }
}

/// A link, if `value` is one: `[source_id, output_index]`.
pub fn as_link(value: &Value) -> Option<(&str, u64)> {
    let pair = value.as_array()?;
    if pair.len() != 2 {
        return None;
    }
    Some((pair[0].as_str()?, pair[1].as_u64()?))
}

/// The class of node `id`.
pub fn class_of<'a>(workflow: &'a Value, id: &str) -> Option<&'a str> {
    workflow.get(id)?.get("class_type")?.as_str()
}

/// Classify every address: writable, link-fed, or missing.
pub fn audit_slots(workflow: &Value, addresses: &[String]) -> SlotAudit {
    let mut audit = SlotAudit::default();
    for address in addresses {
        let Some((node, input)) = split_address(address) else {
            audit.missing.push(address.clone());
            continue;
        };
        let Some(value) = workflow
            .get(node)
            .and_then(|n| n.get("inputs"))
            .and_then(|i| i.get(input))
        else {
            audit.missing.push(address.clone());
            continue;
        };
        if let Some((source, _)) = as_link(value) {
            audit.link_fed.push(LinkFed {
                address: address.clone(),
                source: source.to_string(),
                source_class: class_of(workflow, source).map(str::to_string),
            });
        }
    }
    audit
}

/// Why a value could not be written.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WriteError {
    /// The address names no node or input in this workflow.
    #[error("the workflow has no input at {0}")]
    Missing(String),
    /// The input is a link; writing it would replace a connection with a value.
    #[error("{0} is connected to another node, so a value written there would be ignored")]
    LinkFed(String),
}

/// Write `value` at `address`. Refuses a missing input and a link -- the same
/// rule as [`audit_slots`], enforced at the point of writing too, so no caller
/// can skip the audit and silently rewire a graph.
pub fn write_slot(workflow: &mut Value, address: &str, value: Value) -> Result<(), WriteError> {
    let missing = || WriteError::Missing(address.to_string());
    let (node, input) = split_address(address).ok_or_else(missing)?;
    let slot = workflow
        .get_mut(node)
        .and_then(|n| n.get_mut("inputs"))
        .and_then(|i| i.get_mut(input))
        .ok_or_else(missing)?;
    if as_link(slot).is_some() {
        return Err(WriteError::LinkFed(address.to_string()));
    }
    *slot = value;
    Ok(())
}

/// The id of the node driving `address`, when a link drives it.
///
/// For role suggestion (T-313c): an inert candidate's driver may own the slot
/// that works -- `3.seed` is fed by `PrimitiveInt` 109, whose `109.value` is
/// ACE-Step's real seed.
pub fn link_origin(workflow: &Value, address: &str) -> Option<String> {
    let (node, input) = split_address(address)?;
    let value = workflow.get(node)?.get("inputs")?.get(input)?;
    as_link(value).map(|(source, _)| source.to_string())
}

/// The input names that `instance`'s outputs drive, one hop.
///
/// Role suggestion needs it because an image graph's two `CLIPTextEncode`
/// nodes are indistinguishable by name -- both expose `text` -- and the only
/// thing separating prompt from negative is which sampler input each lands on.
/// One hop on purpose: a deeper walk would follow ACE-Step's encoder through
/// `ConditioningZeroOut` and report that it drives the negative side too.
pub fn output_targets(workflow: &Value, instance: &str) -> Vec<String> {
    let Some(nodes) = workflow.as_object() else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for node in nodes.values() {
        let Some(inputs) = node.get("inputs").and_then(Value::as_object) else {
            continue;
        };
        for (name, value) in inputs {
            if as_link(value).is_some_and(|(source, _)| source == instance) {
                targets.push(name.clone());
            }
        }
    }
    targets.sort();
    targets
}

/// Every node id in a stable order: numeric ids numerically, then the rest.
pub fn node_ids(workflow: &Value) -> Vec<String> {
    let mut ids: Vec<String> = workflow
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    ids.sort_by_key(|id| {
        let head = id.split(':').next().unwrap_or(id);
        (head.parse::<u64>().unwrap_or(u64::MAX), id.clone())
    });
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shipped(id: &str) -> Value {
        let path = format!(
            "{}/../../profiles/workflows/{id}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("workflow")).expect("JSON")
    }

    #[test]
    fn test_split_address_splits_at_the_first_dot() {
        assert_eq!(
            split_address("111.format.quality"),
            Some(("111", "format.quality"))
        );
        assert_eq!(split_address("76:3.seed"), Some(("76:3", "seed")));
        assert_eq!(split_address("seed"), None);
        assert_eq!(split_address(".seed"), None);
    }

    /// Protects: the MCP-SURFACE 18.1 trap, in API format. ACE-Step's `3.seed`
    /// and `94.seed` are links from `PrimitiveInt` 109 and are reported as such;
    /// `109.value` is the writable seed.
    #[test]
    fn test_audit_reports_the_ace_step_seeds_as_link_fed() {
        let wf = shipped("ace-step-1.5-turbo");
        let audit = audit_slots(
            &wf,
            &[
                "3.seed".into(),
                "94.seed".into(),
                "109.value".into(),
                "94.tags".into(),
            ],
        );
        let fed: Vec<_> = audit.link_fed.iter().map(|f| f.address.as_str()).collect();
        assert_eq!(fed, ["3.seed", "94.seed"]);
        assert_eq!(audit.link_fed[0].source, "109");
        assert_eq!(
            audit.link_fed[0].source_class.as_deref(),
            Some("PrimitiveInt")
        );
        assert!(audit.missing.is_empty());
    }

    #[test]
    fn test_audit_reports_missing_nodes_and_inputs() {
        let wf = shipped("flux-1-schnell-fp8");
        let audit = audit_slots(&wf, &["999.text".into(), "6.nope".into(), "garbage".into()]);
        assert_eq!(audit.missing, ["999.text", "6.nope", "garbage"]);
        assert!(!audit.is_clean());
    }

    #[test]
    fn test_write_slot_sets_a_literal_and_refuses_a_link() {
        let mut wf = shipped("ace-step-1.5-turbo");
        write_slot(&mut wf, "109.value", json!(42)).expect("literal");
        assert_eq!(wf["109"]["inputs"]["value"], json!(42));
        assert_eq!(
            write_slot(&mut wf, "3.seed", json!(1)),
            Err(WriteError::LinkFed("3.seed".into()))
        );
        assert_eq!(
            wf["3"]["inputs"]["seed"],
            json!(["109", 0]),
            "link untouched"
        );
        assert_eq!(
            write_slot(&mut wf, "3.nope", json!(1)),
            Err(WriteError::Missing("3.nope".into()))
        );
    }

    #[test]
    fn test_link_origin_and_output_targets_walk_one_hop() {
        let wf = shipped("flux-1-schnell-fp8");
        assert_eq!(link_origin(&wf, "31.positive").as_deref(), Some("6"));
        assert_eq!(link_origin(&wf, "31.seed"), None, "a literal has no origin");
        assert_eq!(output_targets(&wf, "6"), ["positive"]);
        assert_eq!(output_targets(&wf, "33"), ["negative"]);
    }

    #[test]
    fn test_node_ids_sort_numerically_with_subgraph_ids_after_their_parent() {
        let wf = json!({"10": {}, "9": {}, "76:3": {}, "76:12": {}, "60": {}});
        assert_eq!(node_ids(&wf), ["9", "10", "60", "76:12", "76:3"]);
    }
}
