# T-111a: profiles declare their model files, and readiness is decided from them
**Depends:** T-107a | **Crate/dir:** `crates/create-core`, `profiles/`
**Files to create/modify:**
- `crates/create-core/src/readiness.rs` (create)
- `crates/create-core/src/profile.rs` (modify: add `ModelFileSpec`, add one field to `ComfySpec`)
- `crates/create-core/src/lib.rs` (modify: one doc line, one `mod`, one re-export)
- `profiles/ace-step-1.5-turbo.json`, `profiles/minimax-music-3.json` (modify: add `comfy.models`)

## Goal
Decide whether a profile's models are actually installed. Read
[docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md) **section 14** first -- every rule below is a
captured fact, not a design preference.

## Spec
Exactly the reference implementation below.

**Why the profile declares its files rather than asking ComfyUI.** No comfy-mcp tool answers
"which model files does this workflow need". `workflow_deps` maps node classes to node
*packs*; `node_dependencies` checks a pack's *Python* requirements. The only signal is
`local_check`'s prose errors -- `"node 104: 'acestep_v1.5_xl_turbo_bf16.safetensors' not in 2
known options for unet_name"` -- and deciding whether to start an 18.5 GiB download by parsing
English is not acceptable (MCP-SURFACE 14.2).

**Four rules that are correctness, not modelling taste:**

- **`local_check.runnable` is the wrong question.** MiniMax Music 3 has all three files
  installed and still reports `runnable: false`, because the gallery template pins the fp16
  DiT while the int8 is on disk -- a mismatch the profile's own `slot_overrides` already
  corrects. Driving readiness off `runnable` tells a user with a working install to download
  2.3 GiB they have (MCP-SURFACE 14.4). This is why `readiness.rs` exists at all.
- **No inventory is `Unknown`, not `Missing`.** `search_models` needs a *running* ComfyUI --
  it fetches `http://127.0.0.1:8188/models`, it does not read the disk. A stopped server must
  never render as "not installed" (MCP-SURFACE 14.1).
- **No declared files is `Undeclared`, not `Ready`.** "Nobody wrote the list down" is not
  "you have everything".
- **One unknown size makes the whole total unknown.** A partial sum shown as if complete
  understates a download that is already 18.5 GiB.

**Folders are matched exactly, per folder.** A folder the inventory never listed reads as
absent. That direction is deliberate: it can say "missing" for something present, which the
user sees and corrects, where the reverse calls a broken install ready.

## The file lists are captured, not guessed
Both were read from the Hugging Face repo listings on 2026-08-25 and cross-checked against the
slot names in each template's `local_check` errors (MCP-SURFACE 14.5). **Do not adjust the
filenames, folders or sizes** -- the tests assert the exact byte totals, and ACE-Step puts
nothing in `checkpoints`, which is the mistake worth not making.

## Reference implementation

### `crates/create-core/src/readiness.rs` (create)
```rust
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
}
```

### `crates/create-core/src/profile.rs`, `lib.rs`, and the two profiles (modify)
Apply exactly this diff:
```diff
diff --git a/crates/create-core/src/lib.rs b/crates/create-core/src/lib.rs
index 2f2b332..7ab8a99 100644
--- a/crates/create-core/src/lib.rs
+++ b/crates/create-core/src/lib.rs
@@ -12,11 +12,13 @@ pub mod generation;
 pub mod profile;
 pub mod project;
 pub mod provenance;
+pub mod readiness;
 
 pub use generation::*;
 pub use profile::*;
 pub use project::*;
 pub use provenance::*;
+pub use readiness::*;
 
 #[cfg(test)]
 mod tests {
diff --git a/crates/create-core/src/profile.rs b/crates/create-core/src/profile.rs
index 99968b9..6ad7c73 100644
--- a/crates/create-core/src/profile.rs
+++ b/crates/create-core/src/profile.rs
@@ -153,6 +153,36 @@ fn default_true() -> bool {
     true
 }
 
+/// One model file this profile needs present in ComfyUI.
+///
+/// **Declared, not derived.** comfy-mcp has no tool that answers "which model
+/// files does this workflow need": `workflow_deps` maps node classes to node
+/// *packs*, and `node_dependencies` checks a pack's *Python* requirements
+/// against the venv. The only signal is `local_check`'s prose errors --
+/// `"node 104: 'acestep_v1.5_xl_turbo_bf16.safetensors' not in 2 known options
+/// for unet_name"` -- and parsing English to decide whether to start a
+/// multi-gigabyte download is not something this app will do (MCP-SURFACE 14).
+#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
+pub struct ModelFileSpec {
+    /// Exact filename, as ComfyUI lists it in `search_models(folder=)`.
+    /// Compared verbatim: this is the string the workflow's enum slot holds.
+    pub file: String,
+    /// ComfyUI models sub-folder, e.g. `"diffusion_models"`. Not always
+    /// `"checkpoints"` -- ACE-Step 1.5 ships as a split unet/vae/text-encoder
+    /// set and puts nothing in `checkpoints` at all.
+    pub folder: String,
+    /// Direct download URL. `None` means this app cannot fetch the file and
+    /// must instead tell the user the name and folder to place it in.
+    #[serde(default)]
+    pub source_url: Option<String>,
+    /// Download size in bytes, so the total can be shown *before* the user
+    /// commits to it. ACE-Step 1.5 XL Turbo is 18.5 GiB across four files.
+    #[serde(default)]
+    pub size_bytes: Option<u64>,
+    /// Set only when this file's terms differ from the profile's own licence.
+    #[serde(default)]
+    pub license: Option<String>,
+}
 /// How this profile reaches ComfyUI.
 #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
 pub struct ComfySpec {
@@ -172,6 +202,10 @@ pub struct ComfySpec {
     /// overrides `37/6.unet_name` to the int8 file (MCP-SURFACE 6).
     #[serde(default)]
     pub slot_overrides: BTreeMap<SlotAddress, InputValue>,
+    /// Every model file this profile needs. Empty means "not declared", which
+    /// the UI reports as unknown -- never as ready.
+    #[serde(default)]
+    pub models: Vec<ModelFileSpec>,
     pub output: OutputSpec,
 }
 
diff --git a/profiles/ace-step-1.5-turbo.json b/profiles/ace-step-1.5-turbo.json
index d27a795..6c27873 100644
--- a/profiles/ace-step-1.5-turbo.json
+++ b/profiles/ace-step-1.5-turbo.json
@@ -8,6 +8,12 @@
     "template": "audio_ace_step1_5_xl_turbo",
     "workflow": null,
     "vram_gb_min": 8,
+    "models": [
+      { "file": "acestep_v1.5_xl_turbo_bf16.safetensors", "folder": "diffusion_models", "source_url": "https://huggingface.co/Comfy-Org/ace_step_1.5_ComfyUI_files/resolve/main/split_files/diffusion_models/acestep_v1.5_xl_turbo_bf16.safetensors", "size_bytes": 9974719892 },
+      { "file": "qwen_0.6b_ace15.safetensors", "folder": "text_encoders", "source_url": "https://huggingface.co/Comfy-Org/ace_step_1.5_ComfyUI_files/resolve/main/split_files/text_encoders/qwen_0.6b_ace15.safetensors", "size_bytes": 1191588248 },
+      { "file": "qwen_4b_ace15.safetensors", "folder": "text_encoders", "source_url": "https://huggingface.co/Comfy-Org/ace_step_1.5_ComfyUI_files/resolve/main/split_files/text_encoders/qwen_4b_ace15.safetensors", "size_bytes": 8379154232 },
+      { "file": "ace_1.5_vae.safetensors", "folder": "vae", "source_url": "https://huggingface.co/Comfy-Org/ace_step_1.5_ComfyUI_files/resolve/main/split_files/vae/ace_1.5_vae.safetensors", "size_bytes": 337431732 }
+    ],
     "output": { "save_node": "SaveAudioAdvanced", "prefer_lossless": true }
   },
   "loras": {
diff --git a/profiles/minimax-music-3.json b/profiles/minimax-music-3.json
index 543541a..d56cec4 100644
--- a/profiles/minimax-music-3.json
+++ b/profiles/minimax-music-3.json
@@ -8,6 +8,11 @@
     "template": "audio_minimax_music_3",
     "workflow": null,
     "vram_gb_min": 16,
+    "models": [
+      { "file": "minimax_music3_dit_int8_convrot.safetensors", "folder": "diffusion_models", "source_url": "https://huggingface.co/Comfy-Org/MiniMax-Music-3/resolve/main/diffusion_models/minimax_music3_dit_int8_convrot.safetensors", "size_bytes": 2502161682 },
+      { "file": "minimax_music3_text_encoder_pruned_int8_convrot.safetensors", "folder": "text_encoders", "source_url": "https://huggingface.co/Comfy-Org/MiniMax-Music-3/resolve/main/text_encoders/minimax_music3_text_encoder_pruned_int8_convrot.safetensors", "size_bytes": 9196611886 },
+      { "file": "minimax_music3_dav.safetensors", "folder": "vae", "source_url": "https://huggingface.co/Comfy-Org/MiniMax-Music-3/resolve/main/vae/minimax_music3_dav.safetensors", "size_bytes": 216696128 }
+    ],
     "output": { "save_node": "SaveAudioAdvanced", "prefer_lossless": true },
     "slot_overrides": {
       "37/6.unet_name": { "type": "enum", "value": "minimax_music3_dit_int8_convrot.safetensors" }
```

## Acceptance criteria
- `npm run gate` green.
- `cargo test -p create-core` goes 28 -> **34** tests.
- Both profiles still decode: the existing `profile.rs` fixture tests must pass untouched.
- The two profile JSON files gain **only** the `comfy.models` block -- no reformatting of the
  rest of the file. Their diffs are 6 and 5 added lines respectively.
- **No non-ASCII characters anywhere in the diff.**

## Out of scope
Anything that talks to ComfyUI (T-111b builds the inventory), the Tauri commands, and the UI.
`create-core` stays pure data with no I/O and no async.

## If unclear
Follow the reference implementation exactly. Do not add fields, and do not "simplify"
`ProfileReadiness` to a boolean or an `Option` -- its four arms are four different messages.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --file crates/create-core/src/readiness.rs --file crates/create-core/src/profile.rs --file crates/create-core/src/lib.rs --file profiles/ace-step-1.5-turbo.json --file profiles/minimax-music-3.json
```
