//! Which slot receives which semantic input (ARCHITECTURE 5b).
//!
//! An imported graph arrives as 33 slots; asking someone to read all of them
//! and decide which receives lyrics is not an import flow. Every slot already
//! carries its input name, widget type and node class (MCP-SURFACE 29.5), so
//! the candidates can be ranked.
//!
//! **Name matching alone produces an answer the pipeline refuses**, which is
//! the whole reason this module reads the graph and not just the slot list.
//! ACE-Step exposes three slots that look like the seed:
//!
//! | slot | driven by | writing it |
//! |---|---|---|
//! | `3.seed` | `PrimitiveInt` 109 | ignored -- accepted, persisted, never read |
//! | `94.seed` | `PrimitiveInt` 109 | ignored, same reason |
//! | `109.value` | -- | **this is the seed** |
//!
//! `build_and_submit` *refuses to generate* when a resolved address is inert,
//! so offering `3.seed` would produce a profile that cannot run. And the
//! duration role goes the other way: `94.duration` and `98.seconds` are both
//! link-fed and both **land**, because their driver is a `PrimitiveNode` --
//! a frontend-only node whose link is dropped on conversion. `PrimitiveNode`
//! and `PrimitiveInt`: same idea, opposite behaviour, one word apart.
//!
//! [`crate::audit`] already encodes that distinction and is the only correct
//! source for it. Nothing here re-derives it.

use crate::audit::{audit_slots, link_origin};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A semantic input the app knows how to drive.
///
/// ARCHITECTURE 5b's list and no more. `bpm`, `keyscale`, `language` and the
/// rest are real inputs the shipped ACE-Step profile declares, but adding them
/// here widens the name table without testing anything new about the mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Tags,
    Lyrics,
    Negative,
    DurationSeconds,
    Seed,
    Steps,
    Cfg,
}

impl Role {
    /// Every role, in the order an import screen should show them.
    pub const ALL: [Role; 7] = [
        Role::Tags,
        Role::Lyrics,
        Role::Negative,
        Role::DurationSeconds,
        Role::Seed,
        Role::Steps,
        Role::Cfg,
    ];

    /// Input names that mean this role, lowercase.
    fn names(self) -> &'static [&'static str] {
        match self {
            Role::Tags => &["tags", "prompt", "caption", "text", "positive"],
            Role::Lyrics => &["lyrics"],
            Role::Negative => &["negative", "negative_prompt"],
            Role::DurationSeconds => &["duration", "seconds", "length", "max_duration"],
            Role::Seed => &["seed", "noise_seed"],
            Role::Steps => &["steps"],
            Role::Cfg => &["cfg", "cfg_scale", "guidance"],
        }
    }

    /// Whether a widget of this type could carry this role.
    ///
    /// A hard filter, not a ranking signal: a `STRING` named `seed` is not a
    /// seed, and offering it would be noise in front of a decision.
    fn accepts(self, widget_type: &str) -> bool {
        match self {
            Role::Tags | Role::Lyrics | Role::Negative => widget_type == "STRING",
            Role::Seed | Role::Steps => widget_type == "INT",
            Role::DurationSeconds | Role::Cfg => widget_type == "FLOAT" || widget_type == "INT",
        }
    }
}

/// How sure the suggester is, which is the UI's pre-tick rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// The input name and the widget type both fit. The UI pre-selects these.
    Strong,
    /// Reached by following a link rather than by its own name. Offered first,
    /// never pre-selected.
    Possible,
}

/// One slot offered for one role.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub address: String,
    pub node_type: String,
    pub confidence: Confidence,
    /// Why this was offered, in words a person can check against their own
    /// graph.
    ///
    /// Not decoration. The user is being asked to trust a guess about a graph
    /// they built: a row reading "109.value -- drives 3.seed, 94.seed" can be
    /// confirmed at a glance, and one reading "109.value" cannot.
    pub reason: String,
}

/// What one slot needs to look like to be ranked. Mirrors `mcp_bridge::Slot`,
/// restated here so `create-core` does not depend on the bridge for a pure
/// ranking pass.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotInfo {
    pub address: String,
    pub name: String,
    pub widget_type: String,
    pub instance_id: String,
    pub node_type: String,
}

/// One role and everything offered for it.
///
/// A named struct rather than a tuple because this crosses the wire to the
/// import screen: a tuple serializes as a positional array, which is a poor
/// wire type and a worse one to read on the other side.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoleSuggestion {
    pub role: Role,
    pub candidates: Vec<Candidate>,
}

/// Rank each role's candidates over `slots`, using `workflow` to discard the
/// ones the engine would ignore.
///
/// Roles with nothing plausible are **absent** rather than present and empty:
/// ACE-Step has no negative prompt at all, and a low-confidence guess at
/// `94.tags` would be worse than silence.
pub fn suggest_roles(workflow: &Value, slots: &[SlotInfo]) -> Vec<RoleSuggestion> {
    let addresses: Vec<String> = slots.iter().map(|s| s.address.clone()).collect();
    let audit = audit_slots(workflow, &addresses);
    let inert: Vec<&str> = audit
        .link_fed
        .iter()
        .filter(|f| f.is_inert())
        .map(|f| f.address.as_str())
        .collect();

    let mut out = Vec::new();
    for role in Role::ALL {
        let mut candidates = Vec::new();
        // Which inert slots this role wanted, so the hop can say what it drives.
        let mut blocked: Vec<&SlotInfo> = Vec::new();

        for slot in slots {
            if !role.accepts(&slot.widget_type) {
                continue;
            }
            if !role
                .names()
                .contains(&slot.name.to_ascii_lowercase().as_str())
            {
                continue;
            }
            if inert.contains(&slot.address.as_str()) {
                blocked.push(slot);
                continue;
            }
            candidates.push(Candidate {
                address: slot.address.clone(),
                node_type: slot.node_type.clone(),
                confidence: Confidence::Strong,
                reason: format!("{} on {}", slot.name, slot.node_type),
            });
        }

        // The hop: an inert candidate's driver may own the slot that works.
        for slot in &blocked {
            let Some(origin) = link_origin(workflow, &slot.address) else {
                continue;
            };
            let Some(driver) = slots
                .iter()
                .find(|s| s.instance_id == origin && role.accepts(&s.widget_type))
            else {
                continue;
            };
            if candidates.iter().any(|c| c.address == driver.address) {
                continue;
            }
            let drives: Vec<&str> = blocked
                .iter()
                .filter(|b| link_origin(workflow, &b.address).as_deref() == Some(origin.as_str()))
                .map(|b| b.address.as_str())
                .collect();
            candidates.push(Candidate {
                address: driver.address.clone(),
                node_type: driver.node_type.clone(),
                confidence: Confidence::Possible,
                reason: format!("drives {}", drives.join(", ")),
            });
        }

        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by(|a, b| match (a.confidence, b.confidence) {
            (Confidence::Strong, Confidence::Possible) => std::cmp::Ordering::Less,
            (Confidence::Possible, Confidence::Strong) => std::cmp::Ordering::Greater,
            _ => a.address.cmp(&b.address),
        });
        out.push(RoleSuggestion { role, candidates });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow(name: &str) -> Value {
        read_json(&format!("testdata/workflows/{name}"))
    }

    fn slots(name: &str) -> Vec<SlotInfo> {
        let payload = read_json(&format!("testdata/mcp/{name}"));
        payload
            .get("slots")
            .and_then(Value::as_array)
            .expect("slots")
            .iter()
            .map(|s| SlotInfo {
                address: field(s, "address"),
                name: field(s, "name"),
                widget_type: field(s, "type"),
                instance_id: field(s, "instance_id"),
                node_type: field(s, "node_type"),
            })
            .collect()
    }

    fn field(slot: &Value, key: &str) -> String {
        slot.get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("slot has no {key}"))
            .to_string()
    }

    fn read_json(rel: &str) -> Value {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        serde_json::from_str(&text).expect("fixture decodes")
    }

    fn ace() -> (Value, Vec<SlotInfo>) {
        (
            workflow("ace_step_1_5_xl_turbo.json"),
            slots("list_workflow_slots.ace-step.json"),
        )
    }

    fn for_role(suggestions: &[RoleSuggestion], role: Role) -> Vec<Candidate> {
        suggestions
            .iter()
            .find(|s| s.role == role)
            .map(|s| s.candidates.clone())
            .unwrap_or_default()
    }

    /// Protects: the suggester never offers a mapping `build_and_submit`
    /// would refuse.
    ///
    /// The headline case. `3.seed` and `94.seed` are both named `seed`, both
    /// `INT`, and both **inert** -- driven by `PrimitiveInt` 109, so the write
    /// is accepted, persisted and never read (MCP-SURFACE 18.1). A name-based
    /// suggester offers one of them and produces a profile that cannot
    /// generate. The seed lives on the primitive's own widget.
    #[test]
    fn test_ace_step_seed_resolves_to_the_primitive_not_the_sampler() {
        let (graph, slots) = ace();
        let seeds = for_role(&suggest_roles(&graph, &slots), Role::Seed);
        let addresses: Vec<&str> = seeds.iter().map(|c| c.address.as_str()).collect();

        assert!(!addresses.contains(&"3.seed"), "{addresses:?}");
        assert!(!addresses.contains(&"94.seed"), "{addresses:?}");
        assert!(addresses.contains(&"109.value"), "{addresses:?}");

        let primitive = seeds
            .iter()
            .find(|c| c.address == "109.value")
            .expect("the primitive is offered");
        assert_eq!(primitive.node_type, "PrimitiveInt");
        assert!(primitive.reason.contains("3.seed"), "{}", primitive.reason);
        assert!(primitive.reason.contains("94.seed"), "{}", primitive.reason);

        // **`Possible`, never `Strong`, and the confidence is the UI's
        // pre-tick rule** -- so this assertion is the difference between
        // offering the hop and silently accepting it on the user's behalf.
        // Nothing about `109.value` says "seed": its name is `value` and its
        // class is a primitive. It is right because of the graph's shape,
        // which is a reason to put it top of the list with an explanation, not
        // a reason to tick it.
        assert_eq!(primitive.confidence, Confidence::Possible);
    }

    /// Protects: `PrimitiveNode` is not `PrimitiveInt`.
    ///
    /// Both duration slots are link-fed like the seeds, and both **land**,
    /// because their driver is a frontend-only node whose link is dropped when
    /// the graph is converted. Dropping every link-fed slot would lose the
    /// duration role entirely -- this test and the seed one fail together if
    /// the inert rule is re-derived here instead of delegated to `audit`.
    #[test]
    fn test_ace_step_duration_offers_both_slots_strongly() {
        let (graph, slots) = ace();
        let durations = for_role(&suggest_roles(&graph, &slots), Role::DurationSeconds);
        let strong: Vec<&str> = durations
            .iter()
            .filter(|c| c.confidence == Confidence::Strong)
            .map(|c| c.address.as_str())
            .collect();

        assert!(strong.contains(&"94.duration"), "{strong:?}");
        assert!(strong.contains(&"98.seconds"), "{strong:?}");
    }

    /// Protects: the two text roles land on the encoder, strongly.
    #[test]
    fn test_ace_step_tags_and_lyrics_land_on_the_encoder() {
        let (graph, slots) = ace();
        let suggestions = suggest_roles(&graph, &slots);

        let tags = for_role(&suggestions, Role::Tags);
        assert!(tags
            .iter()
            .any(|c| c.address == "94.tags" && c.confidence == Confidence::Strong));

        let lyrics = for_role(&suggestions, Role::Lyrics);
        assert_eq!(lyrics.len(), 1);
        assert_eq!(lyrics[0].address, "94.lyrics");
        assert_eq!(lyrics[0].confidence, Confidence::Strong);
    }

    /// Protects: a graph whose slots are all subgraph interiors still maps.
    ///
    /// MiniMax addresses every input through `37/13.*`, and a subgraph interior
    /// resolves to `Boundary` -- not inert -- so none of them may be dropped.
    /// The address parser has broken on this shape before (MCP-SURFACE 18.5).
    #[test]
    fn test_minimax_maps_its_roles_through_subgraph_addresses() {
        let graph = workflow("minimax_music3_int8.json");
        let slots = slots("list_workflow_slots.minimax.json");
        let suggestions = suggest_roles(&graph, &slots);

        let caption = for_role(&suggestions, Role::Tags);
        assert!(
            caption.iter().any(|c| c.address == "37/13.caption"),
            "{caption:?}"
        );

        let lyrics = for_role(&suggestions, Role::Lyrics);
        assert!(
            lyrics.iter().any(|c| c.address == "37/13.lyrics"),
            "{lyrics:?}"
        );

        let duration = for_role(&suggestions, Role::DurationSeconds);
        assert!(
            duration.iter().any(|c| c.address == "37/13.max_duration"),
            "{duration:?}"
        );

        let seed = for_role(&suggestions, Role::Seed);
        assert!(!seed.is_empty(), "MiniMax's seed must be offered");
    }

    /// Protects: silence beats a guess.
    ///
    /// ACE-Step has no negative prompt -- the shipped profile records it as
    /// `Unsupported`, verified rather than assumed (MCP-SURFACE 3). A
    /// suggester that reached for `94.tags` to fill the row would be inventing
    /// a capability the model does not have.
    #[test]
    fn test_a_role_with_nothing_plausible_is_absent_rather_than_guessed() {
        let (graph, slots) = ace();
        let negatives = for_role(&suggest_roles(&graph, &slots), Role::Negative);
        assert!(negatives.is_empty(), "{negatives:?}");
    }

    /// Protects: the widget type is a hard filter, not a ranking signal.
    #[test]
    fn test_a_slot_of_the_wrong_type_is_never_offered() {
        let (graph, mut slots) = ace();
        slots.push(SlotInfo {
            address: "999.seed".to_string(),
            name: "seed".to_string(),
            widget_type: "STRING".to_string(),
            instance_id: "999".to_string(),
            node_type: "SomeTextNode".to_string(),
        });

        let seeds = for_role(&suggest_roles(&graph, &slots), Role::Seed);
        assert!(
            !seeds.iter().any(|c| c.address == "999.seed"),
            "a STRING is not a seed: {seeds:?}"
        );
    }

    /// Protects: `Strong` sorts above `Possible`, so the UI's pre-tick rule
    /// and the reading order agree.
    #[test]
    fn test_strong_candidates_come_first() {
        let (graph, slots) = ace();
        for RoleSuggestion { candidates, .. } in suggest_roles(&graph, &slots) {
            let first_possible = candidates
                .iter()
                .position(|c| c.confidence == Confidence::Possible);
            let last_strong = candidates
                .iter()
                .rposition(|c| c.confidence == Confidence::Strong);
            if let (Some(p), Some(s)) = (first_possible, last_strong) {
                assert!(s < p, "a Possible sorted above a Strong: {candidates:?}");
            }
        }
    }
}
