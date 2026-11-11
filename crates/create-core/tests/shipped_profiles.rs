//! Every shipped profile against the workflow it ships with (ARCHITECTURE 5).
//!
//! The comfy-mcp era checked a profile's addresses against a template fetched
//! from a live server, so a wrong address surfaced only on a real run. Now the
//! workflow ships beside the profile, and this file is the whole check: every
//! address must name an input that **exists and holds a literal**, every audio
//! profile must be makeable lossless, and every LoRA-capable profile must take
//! a stack that stays on the MODEL path.

use std::path::{Path, PathBuf};

use create_core::audit::{as_link, audit_slots};
use create_core::graph::{ensure_lossless_output, splice_loras, LoraChoice};
use create_core::profile::{ModelKind, ModelProfile};
use serde_json::Value;

fn profiles_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles")
}

fn shipped() -> Vec<(ModelProfile, Value)> {
    let dir = profiles_dir();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("profiles dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("profile");
        let profile: ModelProfile = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()));
        let rel = profile
            .comfy
            .workflow
            .clone()
            .unwrap_or_else(|| panic!("{} declares no workflow", profile.id));
        let wf_path = dir.join(&rel);
        let wf: Value = serde_json::from_str(
            &std::fs::read_to_string(&wf_path)
                .unwrap_or_else(|e| panic!("{}: {e}", wf_path.display())),
        )
        .expect("workflow JSON");
        out.push((profile, wf));
    }
    out.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    out
}

#[test]
fn test_nine_profiles_ship_each_with_its_workflow() {
    let ids: Vec<String> = shipped().into_iter().map(|(p, _)| p.id).collect();
    assert_eq!(ids.len(), 9, "{ids:?}");
}

/// Protects: no shipped profile names an address its workflow lacks or one a
/// node drives -- the write that would be accepted and ignored (MCP-SURFACE 18.1).
#[test]
fn test_every_shipped_address_is_a_literal_in_its_workflow() {
    for (profile, wf) in shipped() {
        let addresses: Vec<String> = profile.slot_addresses().into_iter().map(|a| a.0).collect();
        assert!(!addresses.is_empty(), "{} names no addresses", profile.id);
        let audit = audit_slots(&wf, &addresses);
        assert!(audit.is_clean(), "{}: {audit:?}", profile.id);
    }
}

#[test]
fn test_every_audio_profile_can_be_made_lossless() {
    for (profile, mut wf) in shipped() {
        if profile.kind != ModelKind::Music {
            continue;
        }
        let change = ensure_lossless_output(&mut wf, &profile.comfy.output)
            .unwrap_or_else(|e| panic!("{}: {e}", profile.id));
        assert_eq!(change.format.as_deref(), Some("flac"), "{}", profile.id);
        for id in change.nodes {
            let inputs = wf[&id]["inputs"].as_object().expect("inputs");
            assert!(
                !inputs.keys().any(|k| k.starts_with("format.")),
                "{}",
                profile.id
            );
        }
    }
}

/// Protects: a two-LoRA stack spliced into every LoRA-capable profile is on the
/// MODEL path -- walking `model` links back from each former consumer reaches
/// both loaders and then the anchor.
#[test]
fn test_every_lora_profile_splices_a_reachable_stack() {
    let stack = [
        LoraChoice {
            name: "a.safetensors".into(),
            strength: 1.0,
        },
        LoraChoice {
            name: "b.safetensors".into(),
            strength: 0.5,
        },
    ];
    for (profile, mut wf) in shipped() {
        let Some(loras) = profile.loras.as_ref() else {
            continue;
        };
        let change =
            splice_loras(&mut wf, loras, &stack).unwrap_or_else(|e| panic!("{}: {e}", profile.id));
        let last = change.nodes.last().expect("loaders").clone();
        let consumers: Vec<String> = wf
            .as_object()
            .expect("object")
            .iter()
            .filter(|(_, n)| {
                n["inputs"]
                    .as_object()
                    .is_some_and(|i| i.values().any(|v| as_link(v) == Some((last.as_str(), 0))))
            })
            .map(|(id, _)| id.clone())
            .collect();
        assert!(
            !consumers.is_empty(),
            "{}: the stack feeds nothing",
            profile.id
        );
        let mut at = last;
        let mut seen = vec![at.clone()];
        while let Some((src, _)) = wf[&at]["inputs"].get("model").and_then(as_link) {
            at = src.to_string();
            seen.push(at.clone());
        }
        assert_eq!(
            seen.last(),
            Some(&loras.attach_after),
            "{}: {seen:?}",
            profile.id
        );
        assert_eq!(seen.len(), 3, "{}: {seen:?}", profile.id);
    }
}
