//! The installed-LoRA catalog, as one panel's worth of state.
//!
//! `create_core::loras::catalog` does the hard part -- 53 raw choices become 12
//! pickable entries in 6 groups (T-307). This module reads the loader node's
//! live `lora_name` list, hands it to that function, and pairs the result with
//! the strength range and stack limit the profile declares.

use create_core::loras::{catalog, Excluded, LoraGroup};
use create_core::profile::{ModelProfile, StrengthRange};
use serde::Serialize;
use tauri::State as TauriState;

use crate::{ConfigDir, ProfilesDir};

/// The input a LoRA loader takes its file from.
///
/// Core ComfyUI's `LoraLoaderModelOnly` names it `lora_name` (verified,
/// MCP-SURFACE 4). A profile declares its `loader_node` class but not this,
/// because no shipped or observed loader spells it differently -- so a node
/// that has no such input is reported as [`CatalogState::Unavailable`] naming
/// the class, which is a profile-authoring mistake surfaced rather than an
/// empty picker.
const LORA_NAME_INPUT: &str = "lora_name";

/// Everything the LoRA stack panel needs for one profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoraPanel {
    /// The range **the profile** offers, deliberately narrower than the node's.
    ///
    /// `LoraLoaderModelOnly.strength_model` accepts `-100.0..=100.0` in steps
    /// of `0.01` (read live, 2026-08-28). Only about `0.0..=2.0` is musically
    /// useful, and a slider spanning the node's range would put every usable
    /// value inside one percent of its travel.
    pub strength: StrengthRange,
    /// How many entries the stack may hold, from the profile.
    pub max_stack: u8,
    pub catalog: CatalogState,
}

/// Whether the installed LoRAs could be read, and how well.
///
/// A tagged union rather than an optional list, for the reason
/// [`crate::profile::EnumOptions`] is one: the states read completely
/// differently to a person, and the backend is the half that can tell them
/// apart.
///
/// Note what is **not** in here: "this model has no LoRA support". That is the
/// command returning `None`, and the difference is load-bearing. A panel that
/// disappeared whenever ComfyUI was down would tell the user their model cannot
/// take LoRAs -- the same collapse the param panel already refuses between "off"
/// and "unsupported".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CatalogState {
    Loaded {
        groups: Vec<LoraGroup>,
        excluded: Vec<Excluded>,
        /// Whether comfy-cli answered from its cache rather than ComfyUI.
        ///
        /// Classified here rather than passed up raw, because a live read
        /// carries neither the `stale` flag nor the warning and reading that
        /// absence correctly took observing both shapes (MCP-SURFACE 19.1).
        ///
        /// What it costs the user is narrower than it first looks: a cached
        /// list is a **short** list, missing whatever was installed since
        /// ComfyUI last ran. A path the live server does not know is rejected
        /// by `validate_workflow` as `unknown_enum_value` before any GPU time,
        /// so a LoRA deleted out from under this picker is a failed job rather
        /// than a silent no-op (19.3).
        cached: bool,
    },
    /// The loader node could not be read at all.
    Unavailable { detail: String },
}

/// The LoRA panel for one profile, or `None` when there is no panel to show.
///
/// `None` covers both "no profile answers to this id" and "this profile
/// declares no `loras` block", because both mean *render nothing* -- and an
/// unknown profile is already reported by the param panel directly above this
/// one. Every other state is a visible panel with a sentence in it.
#[tauri::command]
pub async fn lora_panel(
    state: TauriState<'_, crate::jobs::ComfyState>,
    profiles_dir: TauriState<'_, ProfilesDir>,
    config_dir: TauriState<'_, ConfigDir>,
    profile_id: String,
) -> Result<Option<LoraPanel>, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    let Some(loaded) = set.profiles.get(&profile_id) else {
        return Ok(None);
    };
    let Some(loras) = loaded.profile.loras.as_ref() else {
        return Ok(None);
    };

    let read = match state.connected().await {
        None => Err("ComfyUI is not connected.".to_string()),
        Some(comfy) => comfy
            .node_schema(&loras.loader_node)
            .await
            .map_err(|e| e.to_string()),
    };

    Ok(panel_for(
        &loaded.profile,
        catalog_for(&read, &loras.loader_node),
    ))
}

/// Pair one profile's LoRA rules with a catalog read.
///
/// Pure, and separate from the command, because the one thing that can go wrong
/// here is invisible otherwise: the loader node's own `strength_model` runs
/// `-100.0..=100.0`, the profile's range is `0.0..=2.0`, and both are in scope
/// at the call site. Reading the wrong one compiles, renders a slider, and puts
/// every musically useful value inside one percent of its travel.
fn panel_for(profile: &ModelProfile, catalog: CatalogState) -> Option<LoraPanel> {
    let loras = profile.loras.as_ref()?;
    Some(LoraPanel {
        strength: loras.strength.clone(),
        max_stack: loras.max_stack,
        catalog,
    })
}

/// Project one schema read into the panel's catalog state.
///
/// Pure, and separate from the call, so every branch is testable without a
/// backend -- the same split `profile::options_for` uses.
fn catalog_for(read: &Result<mcp_bridge::NodeSchema, String>, class: &str) -> CatalogState {
    let schema = match read {
        Err(detail) => {
            return CatalogState::Unavailable {
                detail: detail.clone(),
            }
        }
        Ok(schema) => schema,
    };
    let Some(choices) = schema.choices_for(LORA_NAME_INPUT) else {
        return CatalogState::Unavailable {
            detail: format!("{class} has no input named {LORA_NAME_INPUT}."),
        };
    };

    let catalog = catalog(choices);
    CatalogState::Loaded {
        groups: catalog.groups,
        excluded: catalog.excluded,
        cached: schema.is_cached(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_core::loras::ExclusionReason;

    const ACE: &str = include_str!("../../profiles/ace-step-1.5-turbo.json");
    const MINIMAX: &str = include_str!("../../profiles/minimax-music-3.json");

    /// The captured `LoraLoaderModelOnly` read, which is a **cached** response:
    /// it was taken with ComfyUI down and carries both staleness signals.
    fn captured() -> mcp_bridge::NodeSchema {
        serde_json::from_str(include_str!(
            "../../testdata/mcp/nodes.LoraLoaderModelOnly.json"
        ))
        .expect("the captured schema decodes")
    }

    fn profile(json: &str) -> ModelProfile {
        serde_json::from_str(json).expect("profile decodes")
    }

    /// Protects: the real list arrives as a picker, not as 53 raw strings.
    #[test]
    fn test_the_captured_list_becomes_the_catalog() {
        match catalog_for(&Ok(captured()), "LoraLoaderModelOnly") {
            CatalogState::Loaded {
                groups, excluded, ..
            } => {
                assert_eq!(groups.len(), 6);
                assert_eq!(groups.iter().map(|g| g.primary.len()).sum::<usize>(), 12);
                assert_eq!(excluded.len(), 21);
                assert!(excluded
                    .iter()
                    .all(|e| e.reason == ExclusionReason::NotAnAdapter));
            }
            other => panic!("expected a loaded catalog, got {other:?}"),
        }
    }

    /// Protects: a cached list reaches the panel flagged as cached.
    ///
    /// `nodes(action="get")` succeeds with ComfyUI down and the response is
    /// indistinguishable from a live one but for two fields (MCP-SURFACE 19.1).
    /// If the flag stops here, a picker missing the LoRA the user finished
    /// training an hour ago looks exactly like a complete one.
    #[test]
    fn test_a_cached_read_is_flagged_as_cached() {
        match catalog_for(&Ok(captured()), "LoraLoaderModelOnly") {
            CatalogState::Loaded { cached, .. } => assert!(cached),
            other => panic!("expected a loaded catalog, got {other:?}"),
        }
    }

    /// Protects: a live read is **not** warned about.
    ///
    /// The other half of the same rule, and the half that was got wrong once
    /// already: a live response carries no `stale` key and no warning, so
    /// reading absence as "did not say, therefore suspect" warns on every
    /// healthy install. The fixture is the cached one with both signals
    /// stripped, which is exactly the shape observed live on 2026-08-28.
    #[test]
    fn test_a_live_read_is_not_flagged() {
        let mut live = captured();
        live.stale = None;
        live.warnings.clear();

        match catalog_for(&Ok(live), "LoraLoaderModelOnly") {
            CatalogState::Loaded { cached, groups, .. } => {
                assert!(!cached);
                assert_eq!(groups.len(), 6, "the same list, minus the warning");
            }
            other => panic!("expected a loaded catalog, got {other:?}"),
        }
    }

    /// Protects: a loader node with no `lora_name` says so instead of showing
    /// an empty picker. Reachable by a user profile naming the wrong class.
    #[test]
    fn test_a_loader_without_the_input_is_unavailable_not_empty() {
        let mut odd = captured();
        odd.inputs.retain(|i| i.name != LORA_NAME_INPUT);

        match catalog_for(&Ok(odd), "SomeCustomLoader") {
            CatalogState::Unavailable { detail } => {
                assert!(detail.contains("SomeCustomLoader"), "{detail}");
                assert!(detail.contains("lora_name"), "{detail}");
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    /// Protects: a failed read is a visible panel with a reason, not a loaded
    /// one with nothing in it.
    #[test]
    fn test_a_failed_read_carries_its_reason() {
        match catalog_for(
            &Err("ComfyUI is not connected.".into()),
            "LoraLoaderModelOnly",
        ) {
            CatalogState::Unavailable { detail } => assert_eq!(detail, "ComfyUI is not connected."),
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    /// Protects: the strength range comes from the profile, never the node.
    ///
    /// The captured schema in this very test declares `strength_model` as
    /// `-100.0..=100.0` step `0.01`. Reading the node instead of the profile
    /// would give a slider whose entire useful travel is one percent of its
    /// width, and the test would still pass if it only checked that *a* range
    /// arrived -- so it pins the numbers.
    #[test]
    fn test_the_strength_range_is_the_profiles_not_the_nodes() {
        let node = captured();
        let strength = node.input("strength_model").expect("the node's own input");
        let bound = |v: &Option<serde_json::Value>| v.as_ref().and_then(serde_json::Value::as_f64);
        assert_eq!(bound(&strength.options.min), Some(-100.0));
        assert_eq!(bound(&strength.options.max), Some(100.0));

        let built = panel_for(&profile(ACE), catalog_for(&Ok(node), "LoraLoaderModelOnly"))
            .expect("ACE-Step declares a loras block");

        assert_eq!(built.strength.min, 0.0);
        assert_eq!(built.strength.max, 2.0);
        assert_eq!(built.strength.step, Some(0.05));
        assert_eq!(built.max_stack, 4);
    }

    /// Protects: no `loras` block means no panel, decided before ComfyUI is
    /// consulted -- which is why the panel can be hidden without a round trip.
    #[test]
    fn test_a_profile_without_loras_builds_no_panel() {
        let catalog = catalog_for(&Ok(captured()), "LoraLoaderModelOnly");

        assert!(panel_for(&profile(MINIMAX), catalog.clone()).is_none());
        assert!(panel_for(&profile(ACE), catalog).is_some());
    }
}
