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
//!
//! **Two graphs, two link shapes.** The top-level `links` are six-element
//! arrays (`[id, origin_id, origin_slot, target_id, target_slot, type]`); a
//! subgraph's interior `links` are objects with those names as keys
//! (MCP-SURFACE 18.5). Reading an interior link positionally finds nothing and
//! returns "unknown source", which this module treats as inert -- so the naive
//! extension does not fail loudly, it refuses every MiniMax generation.

use serde_json::Value;

/// What drives a link, and therefore whether the write it overrides is inert.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkSource {
    /// A real backend node. Its link survives conversion to the API prompt and
    /// the consumer's own widget is ignored -- so the write is inert.
    Backend(String),
    /// A frontend-only node ([`VIRTUAL_NODE_TYPES`]). The link is dropped when
    /// the graph is converted and the consumer's widget is used, so the write
    /// lands.
    Virtual(String),
    /// The subgraph's own input boundary -- a promoted widget, not a driving
    /// edge, so the write lands.
    ///
    /// Not an inference. Five MiniMax addresses are fed this way
    /// (`37/6.unet_name`, `37/13.caption`, `37/13.lyrics`,
    /// `37/13.max_duration`, `37/38.seed`) and the first live run confirmed all
    /// five applied, read back from `GET /history` (MCP-SURFACE 18.5).
    Boundary,
    /// The link exists but its origin could not be identified.
    Unknown,
}

impl LinkSource {
    /// Classify a link origin by the node type it names.
    ///
    /// `None` -- an origin id no node answers to -- is [`LinkSource::Unknown`],
    /// and therefore inert: the link exists, and if its source cannot be
    /// identified the safe reading is that it survives conversion.
    fn of_node_type(node_type: Option<&str>) -> Self {
        match node_type {
            Some(t) if VIRTUAL_NODE_TYPES.contains(&t) => LinkSource::Virtual(t.to_string()),
            Some(t) => LinkSource::Backend(t.to_string()),
            None => LinkSource::Unknown,
        }
    }
}

/// One resolved slot address whose target input is driven by a link.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkFed {
    /// The address as the profile writes it, e.g. `"3.seed"`.
    pub address: String,
    /// What drives it.
    pub source: LinkSource,
}

/// What [`audit_slots`] could and could not determine.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlotAudit {
    /// Addresses whose target input carries a link.
    pub link_fed: Vec<LinkFed>,
    /// Addresses this check could not resolve: an address naming a node no
    /// graph has, a subgraph definition that is missing, and nesting deeper
    /// than one level.
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
    pub fn is_inert(&self) -> bool {
        matches!(self.source, LinkSource::Backend(_) | LinkSource::Unknown)
    }
}

/// What one address turned out to be.
enum Resolution {
    /// Could not be resolved; report it rather than assume it is fine.
    Unchecked,
    /// Resolved, and the input carries no link -- the write lands.
    NoLink,
    /// Resolved, and the input is driven by this.
    Fed(LinkSource),
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
        let resolution = if instance.contains('/') {
            resolve_in_subgraph(workflow, nodes, instance, field)
        } else {
            resolve_top_level(workflow, nodes, instance, field)
        };
        match resolution {
            Resolution::Unchecked => audit.unchecked.push(address.clone()),
            Resolution::NoLink => {}
            Resolution::Fed(source) => audit.link_fed.push(LinkFed {
                address: address.clone(),
                source,
            }),
        }
    }
    audit
}

/// Resolve `"94.duration"` against the top-level graph.
fn resolve_top_level(workflow: &Value, nodes: &[Value], instance: &str, field: &str) -> Resolution {
    let Some(node) = node_with_id(nodes, instance) else {
        return Resolution::Unchecked;
    };
    let Some(link) = link_on(node, field) else {
        return Resolution::NoLink;
    };
    Resolution::Fed(LinkSource::of_node_type(
        top_level_origin_type(workflow, nodes, link).as_deref(),
    ))
}

/// Resolve `"37/13.seed"`: one level down, into `definitions.subgraphs`.
///
/// The hop is `37` -> its `type`, which is a subgraph definition uuid rather
/// than a node class -> the entry in `definitions.subgraphs` with that `id` ->
/// interior node `13`. The interior is a separate id space, which is why every
/// step is checked rather than assumed.
///
/// **One level only.** A nested `A/B/C` is reported unchecked rather than
/// truncated to `A/B`, which would answer a question about a different node.
fn resolve_in_subgraph(
    workflow: &Value,
    nodes: &[Value],
    instance: &str,
    field: &str,
) -> Resolution {
    let Some((outer, inner)) = instance.split_once('/') else {
        return Resolution::Unchecked;
    };
    if inner.contains('/') {
        return Resolution::Unchecked;
    }

    let Some(definition) = node_with_id(nodes, outer)
        .and_then(|host| host.get("type"))
        .and_then(Value::as_str)
    else {
        return Resolution::Unchecked;
    };
    let Some(subgraph) = workflow
        .pointer("/definitions/subgraphs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|s| s.get("id").and_then(Value::as_str) == Some(definition))
    else {
        return Resolution::Unchecked;
    };
    let Some(interior) = subgraph.get("nodes").and_then(Value::as_array) else {
        return Resolution::Unchecked;
    };
    let Some(node) = node_with_id(interior, inner) else {
        return Resolution::Unchecked;
    };
    let Some(link) = link_on(node, field) else {
        return Resolution::NoLink;
    };
    Resolution::Fed(subgraph_origin(subgraph, interior, link))
}

/// The node in `nodes` whose `id` renders as `id`.
fn node_with_id<'a>(nodes: &'a [Value], id: &str) -> Option<&'a Value> {
    nodes
        .iter()
        .find(|n| n.get("id").map(|v| v.to_string()).as_deref() == Some(id))
}

/// The id of the link driving `field` on `node`, if one does.
fn link_on(node: &Value, field: &str) -> Option<i64> {
    node.get("inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|i| i.get("name").and_then(Value::as_str) == Some(field))
        .and_then(|i| i.get("link"))
        .and_then(Value::as_i64)
}

/// The `type` of the node that top-level link `id` comes from.
///
/// Top-level links are positional six-element arrays: element 0 is the link id,
/// element 1 the origin node id.
fn top_level_origin_type(workflow: &Value, nodes: &[Value], id: i64) -> Option<String> {
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

/// What drives interior link `id` inside `subgraph`.
///
/// Interior links are **objects** keyed `origin_id`/`target_id`, not the
/// positional arrays the top level uses (MCP-SURFACE 18.5).
///
/// An origin of the subgraph's own `inputNode` is the promoted-widget boundary
/// rather than a node. Its id is read from the file -- it is `-10` in the
/// MiniMax capture, with `outputNode` at `-20` and every interior node
/// positive, but the file states it, so this uses what it states.
fn subgraph_origin(subgraph: &Value, interior: &[Value], id: i64) -> LinkSource {
    let Some(origin) = subgraph
        .get("links")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|l| l.get("id").and_then(Value::as_i64) == Some(id))
        .and_then(|l| l.get("origin_id"))
        .and_then(Value::as_i64)
    else {
        return LinkSource::Unknown;
    };

    if subgraph.pointer("/inputNode/id").and_then(Value::as_i64) == Some(origin) {
        return LinkSource::Boundary;
    }

    LinkSource::of_node_type(
        interior
            .iter()
            .find(|n| n.get("id").and_then(Value::as_i64) == Some(origin))
            .and_then(|n| n.get("type"))
            .and_then(Value::as_str),
    )
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

    /// The shipped MiniMax profile.
    fn minimax() -> ModelProfile {
        serde_json::from_str(include_str!("../../../profiles/minimax-music-3.json")).unwrap()
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

    /// A spec that sets every input the shipped MiniMax profile declares.
    fn full_minimax_spec() -> GenerationSpec {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "caption".to_string(),
            InputValue::Text("Genre: synthwave. Mood: dreamy".to_string()),
        );
        inputs.insert(
            "lyrics".to_string(),
            InputValue::Text("[intro]\n[verse]\nline".to_string()),
        );
        inputs.insert("duration_s".to_string(), InputValue::Float(120.0));
        inputs.insert("seed".to_string(), InputValue::Seed(8578771011914929));
        GenerationSpec {
            profile_id: "minimax-music-3".to_string(),
            inputs,
            loras: vec![],
            lyrics: None,
        }
    }

    /// Every address MCP-SURFACE 18.5 read back from `GET /history` on the
    /// first live MiniMax run, with the verdict that run recorded.
    ///
    /// **Ground truth, not a restatement of the code.** The executed prompt is
    /// the subgraph *flattened*; this module reads the file *un-flattened*, so
    /// what it computes is a model of the flattener. This table is the only
    /// thing that can say the model is still right.
    const LIVE_18_5: [(&str, bool); 8] = [
        // Fed from the subgraph's inputNode boundary -- all five applied.
        ("37/6.unet_name", false),
        ("37/13.caption", false),
        ("37/13.lyrics", false),
        ("37/13.max_duration", false),
        ("37/38.seed", false),
        // Fed from real interior backend nodes -- all three inert.
        ("37/13.seed", true),
        ("37/9.seed", true),
        ("37/15.seconds", true),
    ];

    #[test]
    fn test_the_audit_reproduces_the_live_history_read() {
        let workflow = fixture("minimax_music3_int8.json");
        let addresses: Vec<String> = LIVE_18_5.iter().map(|(a, _)| a.to_string()).collect();
        let audit = audit_slots(&workflow, &addresses);

        assert!(
            audit.unchecked.is_empty(),
            "every address in the 18.5 table is one level of subgraph deep and must resolve: {:?}",
            audit.unchecked
        );
        assert_eq!(
            audit.link_fed.len(),
            LIVE_18_5.len(),
            "all eight are link-fed in the fixture"
        );

        for (address, inert) in LIVE_18_5 {
            let fed = audit
                .link_fed
                .iter()
                .find(|f| f.address == address)
                .unwrap_or_else(|| panic!("{address} was not reported at all"));
            assert_eq!(
                fed.is_inert(),
                inert,
                "{address}: the live run said inert={inert}, the audit says {:?}",
                fed.source
            );
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

    /// The mirror of the ACE-Step guard, which MiniMax has never had -- and
    /// could not have had, because until the audit could read a subgraph every
    /// one of its addresses came back `unchecked` and the guard was vacuous.
    ///
    /// It is also what stops the three dropped addresses coming back: they
    /// resolve to `SeedNode` and the text encoder, and `generate.rs` refuses a
    /// job with any inert address at all.
    #[test]
    fn test_no_inert_slots_in_shipped_minimax_profile() {
        let profile = minimax();
        let spec = full_minimax_spec();
        let resolved = profile.resolve_slots(&spec).unwrap();
        let addresses: Vec<String> = resolved.keys().map(|a| a.0.clone()).collect();
        let workflow = fixture("minimax_music3_int8.json");
        let audit = audit_slots(&workflow, &addresses);

        assert!(
            !addresses.is_empty(),
            "the spec resolved to no addresses at all"
        );
        assert!(
            audit.unchecked.is_empty(),
            "MiniMax's addresses are all one level deep and must resolve: {:?}",
            audit.unchecked
        );
        assert!(
            !audit.link_fed.is_empty(),
            "every MiniMax address is link-fed in this template, so the audit must see them"
        );

        let inert: Vec<&LinkFed> = audit.link_fed.iter().filter(|l| l.is_inert()).collect();
        assert!(
            inert.is_empty(),
            "expected no inert slot writes, found: {inert:?}"
        );
    }

    /// The user-visible point of the whole task: MiniMax generates with no
    /// "could not be checked" warning, because there is nothing left unchecked.
    #[test]
    fn test_a_minimax_generation_reports_no_unchecked_addresses() {
        let profile = minimax();
        let resolved = profile.resolve_slots(&full_minimax_spec()).unwrap();
        let addresses: Vec<String> = resolved.keys().map(|a| a.0.clone()).collect();
        let audit = audit_slots(&fixture("minimax_music3_int8.json"), &addresses);
        assert_eq!(audit.unchecked, Vec::<String>::new());
    }

    #[test]
    fn test_a_boundary_fed_subgraph_input_is_not_inert() {
        let workflow = fixture("minimax_music3_int8.json");
        let audit = audit_slots(&workflow, &["37/38.seed".to_string()]);
        assert_eq!(audit.link_fed.len(), 1);
        assert_eq!(audit.link_fed[0].source, LinkSource::Boundary);
        assert!(!audit.link_fed[0].is_inert());
    }

    #[test]
    fn test_a_backend_fed_subgraph_input_is_inert() {
        let workflow = fixture("minimax_music3_int8.json");
        let audit = audit_slots(
            &workflow,
            &["37/13.seed".to_string(), "37/15.seconds".to_string()],
        );
        assert_eq!(
            audit.link_fed[0].source,
            LinkSource::Backend("SeedNode".to_string())
        );
        assert_eq!(
            audit.link_fed[1].source,
            LinkSource::Backend("MiniMaxMusic3TextEncode".to_string())
        );
        assert!(audit.link_fed.iter().all(LinkFed::is_inert));
    }

    /// Interior links are objects; the top-level reader is positional. Reading
    /// one with the other finds nothing and yields `Unknown`, which is inert --
    /// so this failure mode is silent, and refuses every MiniMax generation.
    #[test]
    fn test_interior_links_are_read_as_objects_not_arrays() {
        let workflow = fixture("minimax_music3_int8.json");
        let subgraph = &workflow["definitions"]["subgraphs"][0];
        assert!(
            subgraph["links"][0].is_object(),
            "the fixture's interior links must be objects, or this test proves nothing"
        );
        assert!(
            workflow["links"][0].is_array(),
            "the fixture's top-level links must be arrays, or this test proves nothing"
        );

        let audit = audit_slots(&workflow, &["37/13.caption".to_string()]);
        assert_ne!(
            audit.link_fed[0].source,
            LinkSource::Unknown,
            "a positional read of an object link returns Unknown"
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
            .all(|l| l.source == LinkSource::Virtual("PrimitiveNode".to_string())));
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

    /// One level is resolved; deeper nesting is reported rather than truncated
    /// to `37/13`, which would answer a question about a different node.
    #[test]
    fn test_a_nested_subgraph_address_is_unchecked() {
        let workflow = fixture("minimax_music3_int8.json");
        let audit = audit_slots(&workflow, &["37/13/2.seed".to_string()]);
        assert_eq!(audit.unchecked, vec!["37/13/2.seed"]);
        assert!(audit.link_fed.is_empty());
    }

    #[test]
    fn test_an_interior_node_the_subgraph_lacks_is_unchecked() {
        let workflow = fixture("minimax_music3_int8.json");
        let audit = audit_slots(&workflow, &["37/999.seed".to_string()]);
        assert_eq!(audit.unchecked, vec!["37/999.seed"]);
        assert!(audit.link_fed.is_empty());
    }

    /// A workflow with no `definitions` -- which is every non-subgraph template,
    /// ACE-Step included -- cannot resolve a subgraph address, and says so.
    #[test]
    fn test_a_subgraph_address_without_definitions_is_unchecked() {
        let workflow = fixture("ace_step_1_5_xl_turbo.json");
        assert!(workflow
            .get("definitions")
            .and_then(|d| d.as_object())
            .is_none());
        let audit = audit_slots(&workflow, &["94/1.seed".to_string()]);
        assert_eq!(audit.unchecked, vec!["94/1.seed"]);
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
        assert_eq!(audit.link_fed[0].source, LinkSource::Unknown);
        assert!(audit.link_fed[0].is_inert());
    }

    /// The boundary id is read from the file, not assumed to be `-10`.
    #[test]
    fn test_the_boundary_id_comes_from_the_file() {
        let mut workflow = fixture("minimax_music3_int8.json");
        let subgraph = workflow
            .pointer_mut("/definitions/subgraphs/0")
            .unwrap()
            .as_object_mut()
            .unwrap();
        // Move the boundary, and every link that came from it with it.
        subgraph.insert("inputNode".to_string(), serde_json::json!({ "id": -77 }));
        for link in subgraph
            .get_mut("links")
            .and_then(Value::as_array_mut)
            .unwrap()
        {
            if link.get("origin_id").and_then(Value::as_i64) == Some(-10) {
                link.as_object_mut()
                    .unwrap()
                    .insert("origin_id".to_string(), Value::from(-77));
            }
        }
        let audit = audit_slots(&workflow, &["37/38.seed".to_string()]);
        assert_eq!(audit.link_fed[0].source, LinkSource::Boundary);
    }
}
