//! The installed-LoRA list, made pickable.
//!
//! `nodes(action="get", name="LoraLoaderModelOnly")` returns `lora_name` as a
//! COMBO whose choices are the installed LoRA paths. That is the authoritative
//! list because it is exactly what the graph will accept -- but raw it is not
//! something a person can choose from. On the reference install 53 choices
//! carry 12 things anyone would pick: 21 are `training_state.pt` files that are
//! not adapters at all, and 20 more are epoch checkpoints of a single training
//! run (MCP-SURFACE 4, 12.2, 16.5).
//!
//! This module is the pure transform from that list to a grouped catalog.
//! Paths are carried **verbatim**, backslashes included, because the path is
//! the identity handed to the loader node ([`crate::graph::splice_loras`]).
//!
//! Three things it deliberately does not do:
//!
//! - **Filter by base model.** Two full video models sit in the same folder
//!   (`minimax_h3_fl2v_turbo_*`). Nothing in a filename separates them from an
//!   audio adapter -- MCP-SURFACE 4 says so plainly -- so they survive into the
//!   catalog. A heuristic guessing here would hide real LoRAs to hide two.
//! - **Rename cosmetically.** Labels are mechanical. Stripping `ACE-Step-v1.5-`
//!   prefixes reads better and is exactly the rule 12.2 says needs the owner
//!   looking at the panel, so it waits for T-309.
//! - **Hold favourites or user renames.** Those are persisted user state keyed
//!   on [`LoraEntry::path`] and layered on top; a pure function over one list
//!   is not where they belong.

use std::collections::BTreeSet;

use serde::Serialize;

/// Extensions a loadable adapter has.
///
/// ComfyUI lists everything in `models/loras` with a recognised torch
/// extension, which is why `training_state.pt` appears beside real adapters.
/// `.bin` is here for PEFT's `adapter_model.bin`; this install has none, but
/// the loader takes them and excluding one would be the same silent loss.
const ADAPTER_EXTENSIONS: [&str; 2] = ["safetensors", "bin"];

/// Path segment prefix marking one saved step of a training run.
const EPOCH_PREFIX: &str = "checkpoint-epoch-";

/// Path segment marking the finished adapter of a training run.
const FINAL_SEGMENT: &str = "final";

/// PEFT's structural directory level. Names no variant, so it is skipped when
/// a label falls back to the directory tree.
const ADAPTER_SEGMENT: &str = "adapter";

/// One selectable adapter.
///
/// `Serialize` because the LoRA panel renders this directly: it is a computed
/// value with one consumer, so a view type in `src-tauri` would be a
/// field-for-field copy free to drift from it -- the same reasoning
/// `profile_inputs` follows for the profile's own shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoraEntry {
    /// The choice verbatim, separators included -- what the loader is given.
    pub path: String,
    /// Mechanically derived display text. See [`label`].
    pub label: String,
    /// `Some(n)` when this is step `n` of a training run.
    pub epoch: Option<u32>,
    /// Whether this is a training run's `final/` adapter.
    pub is_final: bool,
}

/// Adapters sharing a top-level directory.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoraGroup {
    /// The first path segment, in the spelling seen first.
    ///
    /// **Empty** for files loose in the `loras` root. The wording for that case
    /// is the panel's to choose, not this module's.
    pub name: String,
    /// Shown by default: every distinct adapter, plus one representative of
    /// each training run.
    pub primary: Vec<LoraEntry>,
    /// Superseded training steps, newest first, behind an expander.
    pub superseded: Vec<LoraEntry>,
}

/// Why a choice did not become an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    /// Not a loadable adapter -- a training state, an optimiser dump.
    ///
    /// MCP-SURFACE 17.6: picking one of these does **not** fail. The run
    /// completes and the track has no LoRA on it. Excluding them is the only
    /// thing standing between a user and a silent no-op.
    NotAnAdapter,
    /// The same file already seen under a different capitalisation.
    ///
    /// A case-insensitive filesystem enumerates one file under two directory
    /// spellings (MCP-SURFACE 4).
    CaseDuplicate,
}

/// A choice that did not become an entry, and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Excluded {
    pub path: String,
    pub reason: ExclusionReason,
}

/// The installed LoRAs, grouped and filtered for a picker.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct LoraCatalog {
    /// Named groups first, alphabetically; loose files last.
    pub groups: Vec<LoraGroup>,
    /// Every choice that did not become an entry, with its reason.
    ///
    /// Reported rather than dropped: 21 of 53 entries vanishing with no account
    /// of why is how a user concludes the app cannot see their LoRAs.
    pub excluded: Vec<Excluded>,
}

impl LoraCatalog {
    /// Every entry shown without opening an expander, across all groups.
    pub fn primary_count(&self) -> usize {
        self.groups.iter().map(|g| g.primary.len()).sum()
    }
}

/// Turn a raw `lora_name` choice list into a picker-ready catalog.
///
/// Order of operations matters and is not arbitrary:
///
/// 1. **Non-adapters go first**, so a `training_state.pt` is reported as what
///    it is rather than as a duplicate of its case-variant twin.
/// 2. **Case-duplicates next**, before grouping -- fold two spellings after
///    grouping and the merged group holds one file twice.
/// 3. **Group by first path segment**, keyed case-insensitively so the variant
///    directories merge, named by whichever spelling was seen first.
/// 4. **Collapse each training run** to one representative.
pub fn catalog(choices: &[String]) -> LoraCatalog {
    let mut excluded = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut keys: Vec<String> = Vec::new();
    let mut groups: Vec<LoraGroup> = Vec::new();

    for choice in choices {
        if !is_adapter(choice) {
            excluded.push(Excluded {
                path: choice.clone(),
                reason: ExclusionReason::NotAnAdapter,
            });
            continue;
        }
        if !seen.insert(fold(choice)) {
            excluded.push(Excluded {
                path: choice.clone(),
                reason: ExclusionReason::CaseDuplicate,
            });
            continue;
        }

        let segments = split_segments(choice);
        let name = if segments.len() > 1 { segments[0] } else { "" };
        let key = fold(name);
        let index = match keys.iter().position(|k| *k == key) {
            Some(index) => index,
            None => {
                keys.push(key);
                groups.push(LoraGroup {
                    name: name.to_string(),
                    primary: Vec::new(),
                    superseded: Vec::new(),
                });
                groups.len() - 1
            }
        };
        groups[index].primary.push(entry(choice, &segments));
    }

    for group in &mut groups {
        collapse_training_run(group);
        group.primary.sort_by_key(|e| fold(&e.label));
        group.superseded.sort_by_key(|e| std::cmp::Reverse(e.epoch));
    }
    groups.sort_by_key(|g| (g.name.is_empty(), fold(&g.name)));

    LoraCatalog { groups, excluded }
}

/// Move every superseded training step out of `primary`.
///
/// A run's representative is its `final/` adapter when it has one, otherwise
/// its highest-numbered epoch. Everything else moves behind the expander --
/// 20 checkpoints of one run is the single biggest reason the raw list is
/// unusable.
///
/// The epoch is compared as a **number**. Sorted as text,
/// `checkpoint-epoch-90` is the largest of `{15, 30, ... 300}`, so a
/// lexicographic pick returns epoch 90, looks entirely reasonable, and offers
/// the user a two-thirds-trained adapter as their finished one.
///
/// Two separate runs under one top-level directory would be treated as one
/// series and lose the older run's representative. Not seen on any install
/// recorded in MCP-SURFACE, and building for it would mean guessing where a
/// run's root is.
fn collapse_training_run(group: &mut LoraGroup) {
    let Some(newest) = group.primary.iter().filter_map(|e| e.epoch).max() else {
        return;
    };
    let has_final = group.primary.iter().any(|e| e.is_final);
    let (primary, superseded): (Vec<_>, Vec<_>) = std::mem::take(&mut group.primary)
        .into_iter()
        .partition(|e| match e.epoch {
            None => true,
            Some(n) => !has_final && n == newest,
        });
    group.primary = primary;
    group.superseded = superseded;
}

/// Build one entry. `segments` is `path` already split.
fn entry(path: &str, segments: &[&str]) -> LoraEntry {
    // Everything between the group directory and the file itself. A loose file
    // has no interior, and `1..0` is a panic rather than an empty range.
    let interior: &[&str] = match segments.len() {
        0..=2 => &[],
        n => &segments[1..n - 1],
    };
    LoraEntry {
        path: path.to_string(),
        label: label(segments),
        epoch: interior.iter().find_map(|s| epoch_number(s)),
        is_final: interior
            .iter()
            .any(|s| s.eq_ignore_ascii_case(FINAL_SEGMENT)),
    }
}

/// Display text for one entry.
///
/// The file stem with a trailing `adapter_model` removed; when that leaves
/// nothing -- which is the common case, since PEFT names every adapter
/// identically -- the deepest directory that names something, skipping the
/// structural `adapter/` level; and failing that the group's own name.
fn label(segments: &[&str]) -> String {
    let file = segments.last().copied().unwrap_or_default();
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    let trimmed = stem
        .trim_end_matches("adapter_model")
        .trim_end_matches(['_', '-']);
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    segments[..segments.len().saturating_sub(1)]
        .iter()
        .rev()
        .find(|s| !s.eq_ignore_ascii_case(ADAPTER_SEGMENT))
        .copied()
        .unwrap_or(file)
        .to_string()
}

/// Whether a choice names a file the loader can consume.
fn is_adapter(path: &str) -> bool {
    let file = split_segments(path).last().copied().unwrap_or_default();
    match file.rsplit_once('.') {
        Some((_, ext)) => ADAPTER_EXTENSIONS
            .iter()
            .any(|a| ext.eq_ignore_ascii_case(a)),
        None => false,
    }
}

/// The epoch a `checkpoint-epoch-N` segment names.
fn epoch_number(segment: &str) -> Option<u32> {
    segment.strip_prefix(EPOCH_PREFIX)?.parse().ok()
}

/// Split on either separator. The list uses backslashes on Windows; a Linux
/// install produces the same paths with forward slashes.
fn split_segments(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|s| !s.is_empty()).collect()
}

/// The comparison key for "the same thing, spelled differently": lowercased,
/// with separators normalised so one file is one key on either platform.
fn fold(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::path::PathBuf;

    fn read_fixture(name: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/mcp")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        serde_json::from_str(&text).unwrap()
    }

    fn choices(value: &Value) -> Vec<String> {
        value
            .as_array()
            .expect("a choices array")
            .iter()
            .map(|v| v.as_str().expect("a string choice").to_string())
            .collect()
    }

    /// The real list, read the way the app will: the `lora_name` COMBO's
    /// choices from a verbatim `nodes(action="get")` capture.
    fn installed() -> Vec<String> {
        let node = read_fixture("nodes.LoraLoaderModelOnly.json");
        let input = node
            .get("inputs")
            .and_then(Value::as_array)
            .expect("inputs")
            .iter()
            .find(|i| i.get("name").and_then(Value::as_str) == Some("lora_name"))
            .expect("the lora_name input");
        choices(input.get("choices").expect("choices"))
    }

    /// The **synthetic** case-variant list. Constructed, not observed -- the
    /// fixture says so in its own `_why` field, and MCP-SURFACE 16.5 explains
    /// why no capture of it exists.
    fn case_variants() -> Vec<String> {
        let fixture = read_fixture("lora_choices.case-variant.synthetic.json");
        assert_eq!(
            fixture.get("_synthetic").and_then(Value::as_bool),
            Some(true),
            "this fixture must keep announcing that it is constructed"
        );
        choices(fixture.get("choices").expect("choices"))
    }

    /// Protects: the frontend's fixture is this function's own output.
    ///
    /// `app/src/state/loras.test.ts` cannot call Rust, so it reads a committed
    /// JSON catalog. Hand-writing that file is the trap this repo has hit
    /// twice: a fixture written to agree with the code agrees with the code.
    /// It is generated from the real 53-entry capture instead -- and nothing
    /// generates one language's fixture from the other's output, so without
    /// this test the two drift silently, which is the same gap
    /// `profile_inputs`' wire-shape tests exist to close.
    ///
    /// Underscore-prefixed keys are the fixture's own provenance and are not
    /// part of the shape.
    #[test]
    fn test_the_committed_catalog_fixture_matches_the_catalog() {
        let mut fixture = read_fixture("lora_catalog.ace-step.json");
        let object = fixture.as_object_mut().expect("a JSON object");
        object.retain(|key, _| !key.starts_with('_'));

        assert_eq!(
            fixture,
            serde_json::to_value(catalog(&installed())).unwrap(),
            "regenerate testdata/mcp/lora_catalog.ace-step.json from catalog()"
        );
    }

    /// Protects: the headline. 53 raw choices become 12 pickable entries.
    #[test]
    fn test_the_real_list_becomes_twelve_pickable_entries() {
        let installed = installed();
        assert_eq!(installed.len(), 53, "the captured list is 53 entries");

        let catalog = catalog(&installed);

        assert_eq!(catalog.primary_count(), 12);
        assert_eq!(catalog.excluded.len(), 21);
        assert_eq!(catalog.groups.len(), 6);
    }

    /// Protects: nothing is dropped without a reason, and the reason for all
    /// 21 is that they are not adapters (MCP-SURFACE 17.6 -- picking one does
    /// not fail, it silently applies no LoRA).
    #[test]
    fn test_every_excluded_entry_is_a_training_state_with_a_reason() {
        let catalog = catalog(&installed());

        assert!(catalog
            .excluded
            .iter()
            .all(|e| e.reason == ExclusionReason::NotAnAdapter));
        assert!(
            catalog.excluded.iter().all(|e| e.path.ends_with(".pt")),
            "only the torch training states were excluded"
        );
    }

    /// Protects: a run's `final/` adapter is the one offered, and all 20 epoch
    /// checkpoints -- including the highest -- go behind the expander.
    #[test]
    fn test_final_supersedes_every_epoch() {
        let catalog = catalog(&installed());
        let run = catalog
            .groups
            .iter()
            .find(|g| g.name == "loragoth")
            .expect("the training run's group");

        assert_eq!(run.primary.len(), 1);
        assert_eq!(run.primary[0].label, "final");
        assert!(run.primary[0].is_final);
        assert_eq!(run.superseded.len(), 20);
        assert_eq!(
            run.superseded[0].epoch,
            Some(300),
            "the expander lists newest first"
        );
    }

    /// Protects: the epoch is compared as a number, not as text.
    ///
    /// The real list has a `final/`, which makes every epoch superseded and
    /// hides this entirely -- so the run is replayed here without it, which is
    /// a training run still in progress. Lexicographically
    /// `checkpoint-epoch-90` is the largest of `{15, 30, ... 300}`, so the
    /// wrong implementation offers epoch 90 as the finished adapter and looks
    /// completely plausible doing it.
    #[test]
    fn test_the_newest_epoch_is_not_the_lexicographic_one() {
        let in_progress: Vec<String> = installed()
            .into_iter()
            .filter(|c| !fold(c).contains("/final/"))
            .collect();

        let catalog = catalog(&in_progress);
        let run = catalog
            .groups
            .iter()
            .find(|g| g.name == "loragoth")
            .expect("the training run's group");

        assert_eq!(run.primary.len(), 1);
        assert_eq!(run.primary[0].epoch, Some(300));
        assert_eq!(run.superseded.len(), 19);
    }

    /// Protects: one file under two directory spellings is one entry.
    ///
    /// Stands on the synthetic fixture, and only that -- the install that
    /// produced a case-variant pair was described in MCP-SURFACE 4 and never
    /// captured (16.5). The rule stays because the filesystem does.
    #[test]
    fn test_case_variant_directories_collapse_to_one_group() {
        let catalog = catalog(&case_variants());

        assert_eq!(catalog.groups.len(), 1);
        assert_eq!(
            catalog.groups[0].name, "LoRAgoth",
            "the group keeps the spelling seen first"
        );
        assert_eq!(catalog.groups[0].primary.len(), 1);
        assert_eq!(catalog.groups[0].superseded.len(), 1);
        assert_eq!(
            catalog
                .excluded
                .iter()
                .filter(|e| e.reason == ExclusionReason::CaseDuplicate)
                .count(),
            2
        );
    }

    /// Protects: a non-adapter is reported as a non-adapter even when it is
    /// also a case-duplicate. The more informative reason wins.
    #[test]
    fn test_a_duplicate_non_adapter_is_reported_as_a_non_adapter() {
        let catalog = catalog(&case_variants());

        assert_eq!(
            catalog
                .excluded
                .iter()
                .filter(|e| e.reason == ExclusionReason::NotAnAdapter)
                .count(),
            2
        );
    }

    /// Protects: the two misfiled video models stay visible.
    ///
    /// This asserts a documented limitation rather than a feature. Nothing in
    /// `minimax_h3_fl2v_turbo_4step_v1.0_768p_comfyui_bf16.safetensors` marks
    /// it as belonging to another model (MCP-SURFACE 4), so filtering it would
    /// mean guessing -- and a guess that hides two files also hides someone's
    /// real LoRA. If this test is ever changed, the rule changed with it.
    #[test]
    fn test_the_misfiled_video_models_are_not_filtered_out() {
        let catalog = catalog(&installed());
        let loose = catalog.groups.last().expect("at least one group");

        assert_eq!(loose.name, "", "loose files sort last, under no group name");
        assert_eq!(loose.primary.len(), 2);
        assert!(loose
            .primary
            .iter()
            .all(|e| e.label.starts_with("minimax_h3")));
    }

    /// Protects: PEFT names every adapter `adapter_model.safetensors`, so the
    /// label has to come from what varies -- the prefix, or the directory.
    #[test]
    fn test_labels_come_from_what_actually_varies() {
        let catalog = catalog(&installed());
        let five = catalog
            .groups
            .iter()
            .find(|g| g.name.contains("raspy-vocal"))
            .expect("the five-adapter directory");

        let labels: Vec<&str> = five.primary.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "ACE-Step-v1.5-raspy-vocal-and-instrumental-5-LoRAs",
                "instrumental",
                "male_vocals",
                "voc_06_inst_14",
                "voc_14_inst_06",
            ],
            "the bare adapter_model falls back to its directory; the rest keep their prefix"
        );
    }

    /// Protects: the path is the identity, carried through untouched.
    ///
    /// `splice_loras` writes this string into the loader's `widgets_values`,
    /// so a catalog that tidied separators would produce a graph naming a file
    /// that does not exist -- and ComfyUI warns and continues (17.6).
    #[test]
    fn test_the_path_is_carried_verbatim() {
        let catalog = catalog(&installed());
        let dream = catalog
            .groups
            .iter()
            .find(|g| g.name.contains("ambient_dream1"))
            .expect("the ambient_dream1 group");

        assert_eq!(
            dream.primary[0].path,
            "ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors"
        );
    }

    /// Protects: the catalog is a function of the set of choices, not of the
    /// order they arrive in.
    ///
    /// Every ordering rule in this module is satisfied by accident on the
    /// captured list, because ComfyUI hands back `choices` already sorted --
    /// the ACE-Step directories, then `loragoth`, then the loose files, and
    /// within each directory the bare `adapter_model` before the prefixed
    /// ones. Delete the group sort and the primary sort outright and every
    /// other test here still passes. Reversing the same real list is the
    /// cheapest input whose order is not already the answer.
    #[test]
    fn test_the_catalog_does_not_depend_on_arrival_order() {
        let forward = catalog(&installed());
        let mut backwards = installed();
        backwards.reverse();

        assert_eq!(catalog(&backwards).groups, forward.groups);
    }

    /// Protects: an install with no LoRAs is an empty catalog, not a panic.
    #[test]
    fn test_an_empty_list_is_an_empty_catalog() {
        assert_eq!(catalog(&[]), LoraCatalog::default());
    }
}
