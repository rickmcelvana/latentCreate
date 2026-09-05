//! Whether a profile's model files are actually present in ComfyUI.
//!
//! Pure set membership: a profile declares the files it needs, ComfyUI lists
//! the files it has, and the comparison is exact string matching per folder.
//! Nothing here does I/O -- `mcp-bridge` builds the [`ModelInventory`], and
//! this decides what it means.
//!
//! **Why not ask ComfyUI whether the workflow can run?** Because
//! `local_check.runnable` answers a different question. MiniMax Music 3 has all
//! three of its files installed and still reports `runnable: false`, because
//! the gallery template pins `minimax_music3_dit_fp16.safetensors` while the
//! int8 DiT is what is on disk -- a mismatch the profile's own `slot_overrides`
//! already corrects (MCP-SURFACE 6). Driving the models step off `runnable`
//! would tell a user with a working install to download 2.3 GiB they have.

use crate::profile::{ModelFileSpec, ModelProfile};
use std::collections::{BTreeMap, BTreeSet};

/// The model files ComfyUI currently has, by folder.
///
/// Built from `search_models(folder=)`, which needs a **running** ComfyUI: it
/// fetches `http://127.0.0.1:8188/models`, it does not read the disk. An
/// inventory that could not be taken is absent, not empty -- see
/// [`ProfileReadiness::Unknown`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelInventory {
    by_folder: BTreeMap<String, BTreeSet<String>>,
}

impl ModelInventory {
    /// Build from `(folder, filenames)` pairs.
    pub fn new(folders: impl IntoIterator<Item = (String, BTreeSet<String>)>) -> Self {
        ModelInventory {
            by_folder: folders.into_iter().collect(),
        }
    }

    /// Whether `file` is present in `folder`.
    ///
    /// A folder that was never listed reads as "not present", which is correct
    /// for a folder ComfyUI does not have -- but the caller must list every
    /// folder its profiles name, or it will report installed files as missing.
    pub fn has(&self, folder: &str, file: &str) -> bool {
        self.by_folder
            .get(folder)
            .is_some_and(|files| files.contains(file))
    }

    /// The folder holding `file`, or `None` when no listed folder has it.
    ///
    /// The inverse of `has`: readiness asks "is this file in *that* folder?"; the
    /// adopt path (T-507b) has a filename and must discover which folder it lives
    /// in. First match wins -- a file of the same name in two folders is not a
    /// case any real model install produces, and either answer is correct for
    /// readiness (both folders have it).
    pub fn folder_of(&self, file: &str) -> Option<&str> {
        self.by_folder
            .iter()
            .find(|(_, files)| files.contains(file))
            .map(|(folder, _)| folder.as_str())
    }

    /// Every folder this inventory covers.
    pub fn folders(&self) -> impl Iterator<Item = &str> {
        self.by_folder.keys().map(String::as_str)
    }
}

/// What the models step should show for one profile.
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileReadiness {
    /// Every declared file is present.
    Ready,
    /// Files are missing, with what to fetch and how big it is.
    Missing {
        files: Vec<ModelFileSpec>,
        /// Total download, or `None` when any missing file has no declared
        /// size. A partial total shown as if complete understates the cost.
        total_bytes: Option<u64>,
    },
    /// The profile declares no files, so nothing can be checked. Distinct from
    /// [`ProfileReadiness::Ready`] on purpose: "nobody wrote the list down" is
    /// not "you have everything".
    Undeclared,
    /// ComfyUI was not running, so no inventory could be taken. Distinct from
    /// [`ProfileReadiness::Missing`]: a wizard that says "not installed" while
    /// ComfyUI is merely stopped sends the user to re-download models they
    /// already have.
    Unknown,
}

impl ProfileReadiness {
    /// Whether this profile can be used right now.
    pub fn is_ready(&self) -> bool {
        matches!(self, ProfileReadiness::Ready)
    }

    /// The files that need fetching, empty for every other state.
    pub fn missing(&self) -> &[ModelFileSpec] {
        match self {
            ProfileReadiness::Missing { files, .. } => files,
            _ => &[],
        }
    }
}

impl ModelProfile {
    /// Check this profile's declared files against what ComfyUI has.
    ///
    /// Pass `None` when ComfyUI is not running: that is [`ProfileReadiness::Unknown`],
    /// not "nothing is installed".
    pub fn readiness(&self, inventory: Option<&ModelInventory>) -> ProfileReadiness {
        let Some(inventory) = inventory else {
            return ProfileReadiness::Unknown;
        };
        if self.comfy.models.is_empty() {
            return ProfileReadiness::Undeclared;
        }
        let files: Vec<ModelFileSpec> = self
            .comfy
            .models
            .iter()
            .filter(|spec| !inventory.has(&spec.folder, &spec.file))
            .cloned()
            .collect();
        if files.is_empty() {
            return ProfileReadiness::Ready;
        }
        let total_bytes = files
            .iter()
            .map(|spec| spec.size_bytes)
            .try_fold(0u64, |sum, size| Some(sum + size?));
        ProfileReadiness::Missing { files, total_bytes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACE_STEP: &str = include_str!("../../../profiles/ace-step-1.5-turbo.json");
    const MINIMAX: &str = include_str!("../../../profiles/minimax-music-3.json");

    fn profile(json: &str) -> ModelProfile {
        serde_json::from_str(json).expect("profile decodes")
    }

    /// The three folders and files captured live on 2026-08-25.
    fn installed(pairs: &[(&str, &[&str])]) -> ModelInventory {
        ModelInventory::new(pairs.iter().map(|(folder, files)| {
            (
                (*folder).to_string(),
                files.iter().map(|f| (*f).to_string()).collect(),
            )
        }))
    }

    /// Protects: the profile's declared files are matched verbatim against the
    /// folder listing. This is the whole mechanism -- if the names or folders
    /// in the shipped profile drift from what ComfyUI reports, a user with a
    /// complete install is told to download it again.
    #[test]
    fn test_a_complete_install_reads_as_ready() {
        let minimax = profile(MINIMAX);
        let inventory = installed(&[
            (
                "diffusion_models",
                &["minimax_music3_dit_int8_convrot.safetensors"],
            ),
            (
                "text_encoders",
                &["minimax_music3_text_encoder_pruned_int8_convrot.safetensors"],
            ),
            ("vae", &["minimax_music3_dav.safetensors"]),
        ]);
        assert_eq!(minimax.readiness(Some(&inventory)), ProfileReadiness::Ready);
    }

    /// Protects: the MiniMax lesson, which is the reason this module exists
    /// rather than reading `local_check.runnable`. The exact inventory above
    /// makes `local_check` report `runnable: false` -- the template pins the
    /// fp16 DiT and the int8 is installed -- while the profile's own
    /// `slot_overrides` already corrects it. Ready is the honest answer.
    #[test]
    fn test_a_slot_override_mismatch_is_not_a_missing_model() {
        let minimax = profile(MINIMAX);
        let pinned = minimax
            .comfy
            .slot_overrides
            .values()
            .next()
            .expect("minimax pins its DiT variant");
        let installed_dit = "minimax_music3_dit_int8_convrot.safetensors";
        assert!(format!("{pinned:?}").contains(installed_dit));

        let inventory = installed(&[
            ("diffusion_models", &[installed_dit]),
            (
                "text_encoders",
                &["minimax_music3_text_encoder_pruned_int8_convrot.safetensors"],
            ),
            ("vae", &["minimax_music3_dav.safetensors"]),
        ]);
        assert!(minimax.readiness(Some(&inventory)).is_ready());
    }

    /// Protects: the missing set names every file and totals the download.
    /// Captured live -- ACE-Step is the app's default model and was *not*
    /// installed on the verification machine, across three folders.
    #[test]
    fn test_missing_files_are_named_and_totalled() {
        let ace = profile(ACE_STEP);
        let inventory = installed(&[
            (
                "diffusion_models",
                &["minimax_music3_dit_int8_convrot.safetensors"],
            ),
            ("text_encoders", &[]),
            ("vae", &["minimax_music3_dav.safetensors"]),
        ]);
        match ace.readiness(Some(&inventory)) {
            ProfileReadiness::Missing { files, total_bytes } => {
                assert_eq!(files.len(), 4, "all four ACE-Step files are absent");
                assert_eq!(total_bytes, Some(19_882_894_104));
                assert!(files.iter().all(|f| f.source_url.is_some()));
            }
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    /// Protects: a partial size total is never shown as if it were complete.
    /// One file without a declared size makes the whole total unknown, because
    /// "3.1 GiB" against an actual 18.5 GiB is worse than saying nothing.
    #[test]
    fn test_an_undeclared_size_makes_the_total_unknown() {
        let mut ace = profile(ACE_STEP);
        ace.comfy.models[0].size_bytes = None;
        match ace.readiness(Some(&ModelInventory::default())) {
            ProfileReadiness::Missing { total_bytes, .. } => assert_eq!(total_bytes, None),
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    /// Protects: the two absences that must not read as answers. No inventory
    /// means ComfyUI is stopped, and no declared files means nobody wrote the
    /// list down -- neither is "ready", and neither is "you must download".
    #[test]
    fn test_absent_inventory_and_absent_declaration_are_both_unknown_not_ready() {
        let ace = profile(ACE_STEP);
        assert_eq!(ace.readiness(None), ProfileReadiness::Unknown);
        assert!(!ace.readiness(None).is_ready());
        assert!(ace.readiness(None).missing().is_empty());

        let mut undeclared = profile(ACE_STEP);
        undeclared.comfy.models.clear();
        let readiness = undeclared.readiness(Some(&ModelInventory::default()));
        assert_eq!(readiness, ProfileReadiness::Undeclared);
        assert!(!readiness.is_ready());
    }

    /// Protects: a folder the inventory never listed is not silently "present".
    /// `search_models` is per-folder, so a caller that forgets a folder must
    /// see missing files, not a false Ready.
    #[test]
    fn test_an_unlisted_folder_is_not_treated_as_present() {
        let ace = profile(ACE_STEP);
        let partial = installed(&[(
            "diffusion_models",
            &["acestep_v1.5_xl_turbo_bf16.safetensors"],
        )]);
        let missing = ace.readiness(Some(&partial));
        assert_eq!(missing.missing().len(), 3, "vae and both text encoders");
        assert!(missing
            .missing()
            .iter()
            .all(|f| f.folder != "diffusion_models"));
    }

    /// Protects: a filename resolves to the folder that holds it, and an
    /// unknown filename resolves to nothing. T-507b uses this to discover
    /// which folder an adopted graph's COMBO value lives in.
    #[test]
    fn test_folder_of_resolves_to_the_holding_folder() {
        let inventory = installed(&[
            ("diffusion_models", &["klein.safetensors"]),
            ("vae", &["ae.safetensors"]),
        ]);
        assert_eq!(
            inventory.folder_of("klein.safetensors"),
            Some("diffusion_models")
        );
        assert_eq!(inventory.folder_of("ae.safetensors"), Some("vae"));
        assert_eq!(inventory.folder_of("euler"), None);
    }
}
