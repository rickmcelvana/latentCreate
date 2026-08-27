//! Whether a resolved slot write can actually reach the engine.
//!
//! `set_workflow_slot` reports an address `applied` whenever it can write the
//! widget. Whether the widget is *read* depends on the graph, and the tool
//! never says: in the ACE-Step template `3.seed` and `94.seed` are driven by a
//! link from `PrimitiveInt` 109, so writing them is accepted, persisted, and
//! ignored (MCP-SURFACE 18.1). This module is the standing check.
//!
//! Separate from [`crate::graph`] on purpose: that module *edits* a workflow,
//! this one only asks questions about one.

use serde_json::Value;

/// One resolved slot address whose target input is driven by a link.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkFed {
    /// The address as the profile writes it, e.g. `"3.seed"`.
    pub address: String,
    /// `type` of the node driving the link, when it is in the top-level graph.
    ///
    /// This decides whether the write is inert. A link from a **frontend-only**
    /// node (`PrimitiveNode`) is dropped when the graph is converted for the
    /// engine and the consumer's own widget value is used, so the write lands.
    /// A link from a **real backend node** (`PrimitiveInt`) survives, and the
    /// consumer's widget is ignored -- so the write is inert.
    pub source_type: Option<String>,
}

/// What [`audit_slots`] could and could not determine.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlotAudit {
    /// Addresses whose target input carries a link.
    pub link_fed: Vec<LinkFed>,
    /// Addresses this check could not resolve: subgraph interiors (`37/6.x`),
    /// and addresses naming a node the top-level graph does not have.
    ///
    /// Reported rather than skipped. A guard that quietly ignores what it
    /// cannot see is how an inert write survives review.
    pub unchecked: Vec<String>,
}

/// Node classes that exist only in the ComfyUI frontend.
///
/// Their links are dropped when the UI graph is converted to the API prompt,
/// which is why a write to an input they feed still lands. Verified on the
/// ACE-Step template: node 99 is a `PrimitiveNode` holding 120, it is **absent
/// from the executed prompt**, and its two consumers ran with the values
/// written into their own widgets.
pub const VIRTUAL_NODE_TYPES: [&str; 2] = ["PrimitiveNode", "Reroute"];

impl LinkFed {
    /// Whether writing this address changes nothing at execution time.
    ///
    /// An unknown source type is reported as inert. The link exists; if its
    /// source cannot be identified, the safe reading is that it survives
    /// conversion and overrides the widget.
    pub fn is_inert(&self) -> bool {
        match self.source_type.as_deref() {
            Some(t) => !VIRTUAL_NODE_TYPES.contains(&t),
            None => true,
        }
    }
}

/// Report which of `addresses` name an input that a link drives.
///
/// **Why this exists.** `set_workflow_slot` reports an address as `applied`
/// whether or not the value can reach the engine. In the ACE-Step template
/// `3.seed` and `94.seed` are both fed from `PrimitiveInt` 109, so writing them
/// is accepted, persisted, and ignored -- every track would render with node
/// 109's seed no matter what the user chose. Nothing in the MCP surface says
/// so: `list_workflow_slots` lists both addresses with a current value and no
/// hint that they are driven.
pub fn audit_slots(workflow: &Value, addresses: &[String]) -> SlotAudit {
    let mut audit = SlotAudit::default();
    let Some(nodes) = workflow.get("nodes").and_then(Value::as_array) else {
        audit.unchecked = addresses.to_vec();
        return audit;
    };

    for address in addresses {
        let Some((instance, field)) = split_address(address) else {
            audit.unchecked.push(address.clone());
            continue;
        };
        if instance.contains('/') {
            // Subgraph interior. Resolving it means walking from the instance
            // node to its definition, which is a different id space (T-305b
            // declined the same thing for the splice).
            audit.unchecked.push(address.clone());
            continue;
        }
        let Some(node) = nodes
            .iter()
            .find(|n| n.get("id").map(|v| v.to_string()).as_deref() == Some(instance))
        else {
            audit.unchecked.push(address.clone());
            continue;
        };
        let link = node
            .get("inputs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|i| i.get("name").and_then(Value::as_str) == Some(field))
            .and_then(|i| i.get("link"))
            .and_then(Value::as_i64);
        let Some(link) = link else {
            continue;
        };
        audit.link_fed.push(LinkFed {
            address: address.clone(),
            source_type: source_type_of(workflow, nodes, link),
        });
    }
    audit
}

/// The `type` of the node that link `id` comes from.
fn source_type_of(workflow: &Value, nodes: &[Value], id: i64) -> Option<String> {
    let src = workflow
        .get("links")
        .and_then(Value::as_array)?
        .iter()
        .find(|l| l.get(0).and_then(Value::as_i64) == Some(id))
        .and_then(|l| l.get(1))
        .and_then(Value::as_i64)?;
    nodes
        .iter()
        .find(|n| n.get("id").and_then(Value::as_i64) == Some(src))
        .and_then(|n| n.get("type"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Split `"3.seed"` into `("3", "seed")`, or `"37/6.unet_name"` into
/// `("37/6", "unet_name")`. `None` when there is no field part.
fn split_address(address: &str) -> Option<(&str, &str)> {
    let (instance, field) = address.rsplit_once('.')?;
    if instance.is_empty() || field.is_empty() {
        return None;
    }
    Some((instance, field))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::{GenerationSpec, InputValue};
    use crate::profile::ModelProfile;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// The real captured template, read from disk at test time.
    fn fixture(name: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/workflows")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        serde_json::from_str(&text).unwrap()
    }

    /// The shipped ACE-Step profile, compiled in the way `profile.rs`'s tests do.
    fn ace() -> ModelProfile {
        serde_json::from_str(include_str!("../../../profiles/ace-step-1.5-turbo.json")).unwrap()
    }

    /// A spec that sets every input the shipped ACE-Step profile declares.
    fn full_ace_spec() -> GenerationSpec {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "tags".to_string(),
            InputValue::Text("synthwave".to_string()),
        );
        inputs.insert(
            "lyrics".to_string(),
            InputValue::Text("[Verse]\nline\n[Chorus]\nline".to_string()),
        );
        inputs.insert("duration_s".to_string(), InputValue::Float(120.0));
        inputs.insert("seed".to_string(), InputValue::Seed(42));
        inputs.insert("bpm".to_string(), InputValue::Int(120));
        inputs.insert(
            "keyscale".to_string(),
            InputValue::Enum("E minor".to_string()),
        );
        inputs.insert(
            "timesignature".to_string(),
            InputValue::Enum("4".to_string()),
        );
        inputs.insert("language".to_string(), InputValue::Enum("en".to_string()));
        inputs.insert("steps".to_string(), InputValue::Int(8));
        inputs.insert("shift".to_string(), InputValue::Float(3.0));
        inputs.insert("planner.cfg_scale".to_string(), InputValue::Float(2.0));
        inputs.insert("planner.temperature".to_string(), InputValue::Float(0.85));
        inputs.insert("planner.top_p".to_string(), InputValue::Float(0.9));
        inputs.insert("planner.top_k".to_string(), InputValue::Int(0));
        inputs.insert("planner.min_p".to_string(), InputValue::Float(0.0));
        GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs,
            loras: vec![],
            lyrics: None,
        }
    }

    #[test]
    fn test_no_inert_slots_in_shipped_ace_step_profile() {
        let profile = ace();
        let spec = full_ace_spec();
        let resolved = profile.resolve_slots(&spec).unwrap();
        let addresses: Vec<String> = resolved.keys().map(|a| a.0.clone()).collect();
        let workflow = fixture("ace_step_1_5_xl_turbo.json");
        let audit = audit_slots(&workflow, &addresses);

        // Vacuity guards. Without them this test -- the one the task exists
        // for -- passes on an audit that reports nothing at all, which a
        // mutation confirmed.
        assert!(
            !addresses.is_empty(),
            "the spec resolved to no addresses at all"
        );
        assert!(
            audit.unchecked.is_empty(),
            "every ACE-Step address is top-level and should be checkable: {:?}",
            audit.unchecked
        );
        assert!(
            !audit.link_fed.is_empty(),
            "duration is link-fed in this template, so the audit must see it"
        );

        let inert: Vec<&LinkFed> = audit.link_fed.iter().filter(|l| l.is_inert()).collect();
        assert!(
            inert.is_empty(),
            "expected no inert slot writes, found: {inert:?}"
        );
    }

    #[test]
    fn test_duration_slots_are_link_fed_from_virtual_node() {
        let workflow = fixture("ace_step_1_5_xl_turbo.json");
        let audit = audit_slots(
            &workflow,
            &["94.duration".to_string(), "98.seconds".to_string()],
        );
        assert_eq!(audit.link_fed.len(), 2);
        assert!(audit
            .link_fed
            .iter()
            .all(|l| l.source_type == Some("PrimitiveNode".to_string())));
        assert!(!audit.link_fed[0].is_inert());
        assert!(!audit.link_fed[1].is_inert());
    }

    #[test]
    fn test_address_without_link_is_not_reported() {
        let workflow = fixture("ace_step_1_5_xl_turbo.json");
        let audit = audit_slots(&workflow, &["94.tags".to_string()]);
        assert!(audit.link_fed.is_empty());
        assert!(audit.unchecked.is_empty());
    }

    #[test]
    fn test_subgraph_address_is_unchecked() {
        let workflow = fixture("minimax_music3_int8.json");
        let audit = audit_slots(&workflow, &["37/6.unet_name".to_string()]);
        assert_eq!(audit.unchecked, vec!["37/6.unet_name"]);
        assert!(audit.link_fed.is_empty());
    }

    #[test]
    fn test_missing_node_is_unchecked() {
        let workflow = fixture("ace_step_1_5_xl_turbo.json");
        let audit = audit_slots(&workflow, &["999.missing".to_string()]);
        assert_eq!(audit.unchecked, vec!["999.missing"]);
        assert!(audit.link_fed.is_empty());
    }

    #[test]
    fn test_link_with_missing_source_is_inert() {
        let mut workflow = fixture("ace_step_1_5_xl_turbo.json");
        let nodes = workflow
            .get_mut("nodes")
            .and_then(Value::as_array_mut)
            .unwrap();
        // Find node 3 and give it a link from a non-existent source id.
        let node = nodes
            .iter_mut()
            .find(|n| n.get("id").and_then(Value::as_i64) == Some(3))
            .unwrap();
        let inputs = node
            .get_mut("inputs")
            .and_then(Value::as_array_mut)
            .unwrap();
        let seed_input = inputs
            .iter_mut()
            .find(|i| i.get("name").and_then(Value::as_str) == Some("seed"))
            .unwrap();
        seed_input
            .as_object_mut()
            .unwrap()
            .insert("link".to_string(), Value::from(999_999));
        let audit = audit_slots(&workflow, &["3.seed".to_string()]);
        assert_eq!(audit.link_fed.len(), 1);
        assert_eq!(audit.link_fed[0].source_type, None);
        assert!(audit.link_fed[0].is_inert());
    }
}
