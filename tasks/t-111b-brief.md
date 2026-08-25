# T-111b: the models step's Tauri command
**Depends:** T-111a, T-110b | **Crate/dir:** `src-tauri`
**Files to create/modify:**
- `src-tauri/src/models.rs` (create)
- `src-tauri/src/lib.rs` (modify: one `mod`, one command registration, `ProfilesDir` state + helper)
- `src-tauri/src/comfy.rs` (modify: **two words** -- `ensure_connected` and `EnsureError` become `pub(crate)`)
- `src-tauri/tauri.conf.json` (modify: one `resources` line)

## Goal
One command that answers "which models can I use", classifying every failure into a state with
a next step. Read [docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md) **section 14** first.

## Spec
Exactly the reference implementation below.

**The backend classifies; the frontend renders** -- the same contract as T-110b.
`models_status` **never returns `Err` for a service problem**. A stopped ComfyUI, an
unreadable profile directory and a half-installed model are all states. The `Err` arm is
reserved for this app failing to open its own session log.

**Three rules that are correctness:**

- **`Unknown` is not `Missing`.** `search_models` fails with `[server_not_running]` when
  ComfyUI is stopped (MCP-SURFACE 14.1). ACE-Step is an 18.5 GiB download; telling a user who
  already has it to fetch it again because their server is off is the worst thing this step
  can do. `is_server_not_running` is the classification, and it is tested.
- **Only the folders the profiles name are listed.** One `search_models` call per folder, and
  this install reports 27 folders -- walking all of them is 27 round trips to answer a
  question about three.
- **This step never reads `local_check`.** Not `runnable`, and above all not `summary`, which
  renders a missing-model problem as node-class advice ("Update ComfyUI and its custom nodes,
  or pick another template") that cannot fix it (MCP-SURFACE 14.3, 14.4).

**`ProfilesDir` is new startup state.** The shipped profiles had no runtime home before this:
they are now a bundle resource, with a dev fallback to the repo checkout, because a dev build
has no bundle. A missing directory is not an error -- `library::profiles::load` treats it as
"no shipped profiles".

**The failure reason is carried, not dropped.** `inventory_detail` exists because "ComfyUI is
stopped" and "comfy-mcp is not installed" send the user to different places, and a banner that
cannot tell them apart says nothing useful.

## The live test
The reference includes `test_readiness_against_a_live_comfyui`, marked `#[ignore]`. It is
**not** run by CI and **not** part of the gate. It asserts the shape of the answer, not which
models happen to be on the machine. It has been run and passes; leave it exactly as written.

## Reference implementation

### `src-tauri/src/models.rs` (create)
```rust
//! The wizard's models step: which profiles can actually run, and what is
//! missing from the ones that cannot.
//!
//! The backend classifies; the frontend renders. As with the ComfyUI step, no
//! service problem returns `Err` -- a stopped ComfyUI, an unreadable profile
//! directory and a half-installed model are all states with a next step.
//!
//! **This step never reads `local_check`.** That answers "can this template run
//! here", which is not the same question: MiniMax Music 3 is fully installed
//! and still reports `runnable: false`, because the gallery template pins the
//! fp16 DiT and the int8 is what is on disk -- a mismatch the profile's own
//! `slot_overrides` corrects (MCP-SURFACE 6, 14). Worse, `local_check.summary`
//! renders every such problem as node-class advice ("Update ComfyUI and its
//! custom nodes, or pick another template"), which for a missing model sends
//! the user somewhere that cannot help. Readiness is decided by comparing the
//! profile's declared files against `search_models(folder=)`, and nothing else.

use std::collections::{BTreeMap, BTreeSet};

use create_core::profile::{ModelKind, ModelProfile};
use create_core::readiness::{ModelInventory, ProfileReadiness};
use library::profiles::{ProfileSource, ProfileWarning};
use mcp_bridge::{ComfyError, LocalComfy};
use serde::Serialize;
use tauri::State;

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::{ConfigDir, ProfilesDir};

/// One model file the user does not have.
#[derive(Debug, Clone, Serialize)]
pub struct MissingFile {
    pub file: String,
    pub folder: String,
    /// `None` means this app cannot fetch it; the UI shows the name and folder
    /// so the user can place it by hand.
    pub source_url: Option<String>,
    pub size_bytes: Option<u64>,
    /// Set only when this file's terms differ from the profile's own licence.
    pub license: Option<String>,
}

/// Whether a profile's models are installed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Readiness {
    /// Every declared file is present.
    Ready,
    /// Files are missing.
    Missing {
        files: Vec<MissingFile>,
        /// `None` when any missing file has no declared size -- a partial total
        /// shown as if complete understates an already large download.
        total_bytes: Option<u64>,
        /// Whether every missing file carries a URL, so the app can install
        /// them. False means at least one has to be placed by hand.
        installable: bool,
    },
    /// The profile lists no files, so nothing could be checked. Not "ready".
    Undeclared,
    /// ComfyUI is not running, so nothing could be checked. Not "missing" --
    /// telling a user to re-download 18 GiB because their server is stopped is
    /// the worst thing this step could do.
    Unknown,
}

/// One row of the models step.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileStatus {
    pub id: String,
    pub display_name: String,
    pub kind: ModelKind,
    /// Shown wherever the model is chosen or installed (CONVENTIONS).
    pub license: String,
    pub license_notes: Option<String>,
    /// Whether this profile shipped with the app or came from the user's own
    /// directory, so an override is visible rather than mysterious.
    pub source: ProfileSource,
    pub vram_gb_min: Option<u32>,
    pub readiness: Readiness,
}

/// What the models step shows.
#[derive(Debug, Clone, Serialize)]
pub struct ModelsView {
    pub profiles: Vec<ProfileStatus>,
    /// Profiles that could not be read. Surfaced rather than swallowed: a
    /// user's own broken profile is invisible otherwise.
    pub warnings: Vec<ProfileWarning>,
    /// False when the installed-model list could not be taken, which makes
    /// every row `Unknown`. The view shows one banner pointing back at the
    /// ComfyUI step rather than repeating the same message on every row.
    pub inventory_available: bool,
    /// Why the list could not be taken, when it could not. Carried because
    /// "ComfyUI is stopped" and "comfy-mcp is not installed" send the user to
    /// different places, and a banner that cannot tell them apart says nothing
    /// useful.
    pub inventory_detail: Option<String>,
}

/// Report every known profile and whether its models are installed.
///
/// **Never returns `Err` for a service problem.** A stopped ComfyUI yields
/// `inventory_available: false` and `Unknown` rows; the `Err` arm is reserved
/// for this app failing to open its own session log.
#[tauri::command]
pub async fn models_status(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    bin: Option<String>,
) -> Result<ModelsView, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));

    let wanted: BTreeSet<String> = set
        .profiles
        .values()
        .flat_map(|loaded| loaded.profile.comfy.models.iter())
        .map(|spec| spec.folder.clone())
        .collect();

    let (inventory, inventory_detail) =
        match take_inventory(&state, &config_dir, bin, &wanted).await {
            Ok(inventory) => (Some(inventory), None),
            Err(TakeError::Comfy(e)) => (None, Some(e.to_string())),
            Err(TakeError::Log(detail)) => return Err(detail),
        };

    let profiles = set
        .profiles
        .into_values()
        .map(|loaded| row(loaded.profile, loaded.source, inventory.as_ref()))
        .collect();

    Ok(ModelsView {
        profiles,
        warnings: set.warnings,
        inventory_available: inventory.is_some(),
        inventory_detail,
    })
}

/// Turn one profile plus the inventory into a row.
fn row(
    profile: ModelProfile,
    source: ProfileSource,
    inventory: Option<&ModelInventory>,
) -> ProfileStatus {
    let readiness = match profile.readiness(inventory) {
        ProfileReadiness::Ready => Readiness::Ready,
        ProfileReadiness::Undeclared => Readiness::Undeclared,
        ProfileReadiness::Unknown => Readiness::Unknown,
        ProfileReadiness::Missing { files, total_bytes } => {
            let installable = files.iter().all(|spec| spec.source_url.is_some());
            Readiness::Missing {
                files: files
                    .into_iter()
                    .map(|spec| MissingFile {
                        file: spec.file,
                        folder: spec.folder,
                        source_url: spec.source_url,
                        size_bytes: spec.size_bytes,
                        license: spec.license,
                    })
                    .collect(),
                total_bytes,
                installable,
            }
        }
    };
    ProfileStatus {
        id: profile.id,
        display_name: profile.display_name,
        kind: profile.kind,
        license: profile.license,
        license_notes: profile.license_notes,
        source,
        vram_gb_min: profile.comfy.vram_gb_min,
        readiness,
    }
}

/// Why the inventory could not be taken.
pub(crate) enum TakeError {
    /// A service problem, which becomes `Unknown` rather than an error.
    Comfy(ComfyError),
    /// This app could not open its own session log.
    Log(String),
}

/// List every folder the profiles name, and nothing else.
///
/// One call per folder, because `search_models` is per-folder. Only the folders
/// the profiles actually declare are listed -- this install reports 27, and
/// walking all of them would be 27 round trips to answer a question about three.
///
/// A folder that fails individually is left out rather than failing the whole
/// inventory, and [`ModelInventory::has`] then reads its files as absent. That
/// is the safe direction: it can say "missing" for something present, which the
/// user can see and correct, where the reverse would call a broken install ready.
pub(crate) async fn take_inventory(
    state: &State<'_, ComfyState>,
    config_dir: &State<'_, ConfigDir>,
    bin: Option<String>,
    folders: &BTreeSet<String>,
) -> Result<ModelInventory, TakeError> {
    let comfy: std::sync::Arc<LocalComfy> = ensure_connected(state, config_dir, bin)
        .await
        .map_err(|e| match e {
            EnsureError::Comfy(e) => TakeError::Comfy(e),
            EnsureError::Log(detail) => TakeError::Log(detail),
        })?;

    let mut listed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut any = false;
    for folder in folders {
        match comfy.list_models_in(folder).await {
            Ok(contents) => {
                any = true;
                listed.insert(
                    folder.clone(),
                    contents.files.into_iter().map(|f| f.name).collect(),
                );
            }
            Err(e) if is_server_not_running(&e) => return Err(TakeError::Comfy(e)),
            Err(_) => {}
        }
    }
    if !any && !folders.is_empty() {
        return Err(TakeError::Comfy(ComfyError::Payload {
            tool: "search_models".to_string(),
            detail: "no model folder could be listed".to_string(),
        }));
    }
    Ok(ModelInventory::new(listed))
}

/// Whether the failure was "ComfyUI is not running".
///
/// Verified live: `search_models` does **not** read the disk, it fetches
/// `http://127.0.0.1:8188/models`, so with ComfyUI stopped every call fails
/// with this code. That is the whole reason the models step has an `Unknown`
/// state rather than reporting an empty install.
fn is_server_not_running(error: &ComfyError) -> bool {
    matches!(error, ComfyError::Tool { code, .. } if code.as_deref() == Some("server_not_running"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_core::profile::ModelFileSpec;

    const ACE_STEP: &str = include_str!("../../profiles/ace-step-1.5-turbo.json");
    const MINIMAX: &str = include_str!("../../profiles/minimax-music-3.json");

    fn profile(json: &str) -> ModelProfile {
        serde_json::from_str(json).expect("profile decodes")
    }

    fn inventory(pairs: &[(&str, &[&str])]) -> ModelInventory {
        ModelInventory::new(pairs.iter().map(|(folder, files)| {
            (
                (*folder).to_string(),
                files.iter().map(|f| (*f).to_string()).collect(),
            )
        }))
    }

    /// Protects: the row carries the licence. CONVENTIONS requires per-model
    /// terms wherever a model is chosen or installed, and MiniMax is exactly
    /// why -- its weights are open but not OSI-open, with an attribution
    /// obligation the user takes on by generating with it.
    #[test]
    fn test_every_row_carries_its_licence() {
        let status = row(profile(MINIMAX), ProfileSource::Shipped, None);
        assert_eq!(status.license, "MiniMax-Music3 Community License");
        let notes = status.license_notes.expect("MiniMax has conditions");
        assert!(notes.contains("attribution"));

        let ace = row(profile(ACE_STEP), ProfileSource::Shipped, None);
        assert_eq!(ace.license, "Apache-2.0");
    }

    /// Protects: a stopped ComfyUI is `Unknown`, never `Missing`. This is the
    /// single most damaging confusion the step could make -- ACE-Step is an
    /// 18.5 GiB download, and telling a user with it already installed to fetch
    /// it again because their server is stopped is unforgivable.
    #[test]
    fn test_a_stopped_comfyui_is_unknown_not_missing() {
        let status = row(profile(ACE_STEP), ProfileSource::Shipped, None);
        match status.readiness {
            Readiness::Unknown => {}
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    /// Protects: the missing set is installable only when every file has a URL.
    /// A partly-installable set offered as a one-click install leaves the user
    /// with a model that still will not run and no idea why.
    #[test]
    fn test_installable_requires_a_url_for_every_missing_file() {
        let ace = profile(ACE_STEP);
        let status = row(ace.clone(), ProfileSource::Shipped, Some(&inventory(&[])));
        match &status.readiness {
            Readiness::Missing {
                installable,
                total_bytes,
                files,
            } => {
                assert!(installable, "the shipped profile carries every URL");
                assert_eq!(*total_bytes, Some(19_882_894_104));
                assert_eq!(files.len(), 4);
            }
            other => panic!("expected Missing, got {other:?}"),
        }

        let mut hand_placed = ace;
        hand_placed.comfy.models.push(ModelFileSpec {
            file: "extra.safetensors".to_string(),
            folder: "vae".to_string(),
            source_url: None,
            size_bytes: None,
            license: None,
        });
        let status = row(hand_placed, ProfileSource::Shipped, Some(&inventory(&[])));
        match status.readiness {
            Readiness::Missing {
                installable,
                total_bytes,
                ..
            } => {
                assert!(!installable);
                assert_eq!(
                    total_bytes, None,
                    "one unknown size makes the total unknown"
                );
            }
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    /// Protects: the live case. MiniMax's three files were installed on the
    /// verification machine and ACE-Step's four were not, so the step must show
    /// one of each -- and MiniMax must read Ready despite its template failing
    /// `local_check` on the fp16/int8 pin.
    #[test]
    fn test_the_captured_install_shows_one_ready_and_one_missing() {
        let captured = inventory(&[
            (
                "diffusion_models",
                &[
                    "minimax_h3_fl2va_pruned_int8_convrot.safetensors",
                    "minimax_music3_dit_int8_convrot.safetensors",
                ],
            ),
            (
                "text_encoders",
                &[
                    "minimax_music3_text_encoder_pruned_int8_convrot.safetensors",
                    "qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors",
                ],
            ),
            (
                "vae",
                &[
                    "minimax_h3_audio_vae_fp32.safetensors",
                    "minimax_h3_video_vae_fp16.safetensors",
                    "minimax_music3_dav.safetensors",
                ],
            ),
        ]);

        let minimax = row(profile(MINIMAX), ProfileSource::Shipped, Some(&captured));
        assert!(matches!(minimax.readiness, Readiness::Ready));

        let ace = row(profile(ACE_STEP), ProfileSource::Shipped, Some(&captured));
        match ace.readiness {
            Readiness::Missing { files, .. } => assert_eq!(files.len(), 4),
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    /// Protects: the verified stopped-server code, which is what separates
    /// `Unknown` from `Missing` at the transport edge.
    #[test]
    fn test_server_not_running_is_recognised() {
        let stopped = ComfyError::Tool {
            tool: "search_models".to_string(),
            code: Some("server_not_running".to_string()),
            message: "comfy models list-folders failed: failed to fetch \
                      http://127.0.0.1:8188/models"
                .to_string(),
        };
        assert!(is_server_not_running(&stopped));

        let other = ComfyError::Tool {
            tool: "search_models".to_string(),
            code: Some("missing_argument".to_string()),
            message: "nope".to_string(),
        };
        assert!(!is_server_not_running(&other));
    }

    /// The whole readiness path against a **real** comfy-mcp and a **running**
    /// ComfyUI: connect, list the folders the shipped profiles name, and decide.
    ///
    /// Excluded from CI, which has neither. Run it at the T-113 milestone with
    /// `cargo test -p app -- --ignored`. It asserts the shape of the answer, not
    /// which models happen to be on the machine -- the point is that every
    /// declared folder lists and every profile reaches a decided state, because
    /// a silent `Unknown` here is exactly the bug this step exists to avoid.
    #[tokio::test]
    #[ignore = "needs comfy-mcp and a running ComfyUI"]
    async fn test_readiness_against_a_live_comfyui() {
        let log = mcp_bridge::SessionLog::open(std::env::temp_dir().join("latentcreate-live.log"))
            .expect("session log opens");
        let comfy = LocalComfy::connect("comfy-mcp", log)
            .await
            .expect("comfy-mcp connects");

        let profiles = [profile(ACE_STEP), profile(MINIMAX)];
        let folders: BTreeSet<String> = profiles
            .iter()
            .flat_map(|p| p.comfy.models.iter())
            .map(|spec| spec.folder.clone())
            .collect();
        assert!(
            folders.contains("diffusion_models") && folders.contains("vae"),
            "the shipped profiles name the folders this asserts against"
        );

        let mut listed = BTreeMap::new();
        for folder in &folders {
            let contents = comfy
                .list_models_in(folder)
                .await
                .unwrap_or_else(|e| panic!("listing {folder} failed: {e}"));
            listed.insert(
                folder.clone(),
                contents
                    .files
                    .into_iter()
                    .map(|f| f.name)
                    .collect::<BTreeSet<String>>(),
            );
        }
        let inventory = ModelInventory::new(listed);

        for profile in profiles {
            let id = profile.id.clone();
            let readiness = profile.readiness(Some(&inventory));
            assert!(
                !matches!(
                    readiness,
                    ProfileReadiness::Unknown | ProfileReadiness::Undeclared
                ),
                "{id} reached no decision against a live install: {readiness:?}"
            );
        }
    }

    /// Protects: the view crosses the Tauri boundary as a tagged union with
    /// snake_case tags, one variant per state. A rename breaks every UI branch.
    #[test]
    fn test_readiness_serialises_as_a_tagged_union() {
        let json = serde_json::to_value(Readiness::Unknown).expect("serialises");
        assert_eq!(json["state"], serde_json::json!("unknown"));

        let json = serde_json::to_value(Readiness::Missing {
            files: vec![],
            total_bytes: None,
            installable: false,
        })
        .expect("serialises");
        assert_eq!(json["state"], serde_json::json!("missing"));
        assert_eq!(json["installable"], serde_json::json!(false));
    }
}
```

### `src-tauri/src/lib.rs`, `comfy.rs`, `tauri.conf.json` (modify)
Apply exactly this diff. The `comfy.rs` change is only two visibility keywords -- do not touch
anything else in that file. The `install::` registrations arrive in T-111c; leave them out here.
```diff
diff --git a/src-tauri/src/comfy.rs b/src-tauri/src/comfy.rs
index 38d6c25..d9e1112 100644
--- a/src-tauri/src/comfy.rs
+++ b/src-tauri/src/comfy.rs
@@ -141,7 +141,7 @@ fn is_port_in_use(error: &ComfyError) -> bool {
 }
 
 /// Why `ensure_connected` gave up.
-enum EnsureError {
+pub(crate) enum EnsureError {
     /// A service problem, which becomes a status rather than an error.
     Comfy(ComfyError),
     /// This app could not open its own session log. Genuinely our fault.
@@ -149,7 +149,7 @@ enum EnsureError {
 }
 
 /// The connected backend, connecting on first use.
-async fn ensure_connected(
+pub(crate) async fn ensure_connected(
     state: &State<'_, ComfyState>,
     config_dir: &State<'_, ConfigDir>,
     bin: Option<String>,
diff --git a/src-tauri/src/lib.rs b/src-tauri/src/lib.rs
index d5a531a..b2001c8 100644
--- a/src-tauri/src/lib.rs
+++ b/src-tauri/src/lib.rs
@@ -8,13 +8,23 @@ use std::path::PathBuf;
 use tauri::Manager;
 
 mod comfy;
+mod install;
 mod jobs;
+mod models;
 
 use jobs::ComfyState;
 
 /// Resolved once at startup so every command shares one location.
 struct ConfigDir(PathBuf);
 
+/// Where the profiles that ship with the app live, resolved once at startup.
+///
+/// Bundled as a resource; in a dev build the bundle has not been assembled, so
+/// this falls back to the repo's own `profiles/`. A missing directory is not an
+/// error here -- `library::profiles::load` treats it as "no shipped profiles",
+/// and the models step then has nothing to check rather than failing to open.
+struct ProfilesDir(PathBuf);
+
 /// Launches the Tauri application.
 ///
 /// # Panics
@@ -28,6 +38,7 @@ pub fn run() {
         .setup(|app| {
             let dir = app.path().app_config_dir()?;
             app.manage(ConfigDir(dir));
+            app.manage(ProfilesDir(shipped_profiles_dir(app.handle())));
             app.manage(ComfyState::default());
             Ok(())
         })
@@ -40,6 +51,9 @@ pub fn run() {
             delete_secret,
             comfy::comfy_status,
             comfy::comfy_launch,
+            models::models_status,
+            install::models_install,
+            install::models_progress,
             jobs::connect_comfy,
             jobs::run_workflow,
             jobs::cancel_job
@@ -88,6 +102,22 @@ fn delete_secret(name: String) -> Result<(), String> {
     library::secrets::delete_secret(key).map_err(|e| e.to_string())
 }
 
+/// Locate the shipped profiles directory.
+///
+/// The bundled resource wins. A dev build has no bundle, so it falls back to
+/// the repo checkout next to this crate -- which is also why this returns a
+/// path rather than failing: neither location existing is a normal state for a
+/// build with no profiles, not a startup error.
+fn shipped_profiles_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> PathBuf {
+    if let Ok(resources) = app.path().resource_dir() {
+        let bundled = resources.join("profiles");
+        if bundled.is_dir() {
+            return bundled;
+        }
+    }
+    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../profiles")
+}
+
 #[cfg(test)]
 mod tests {
     use super::app_version;
diff --git a/src-tauri/tauri.conf.json b/src-tauri/tauri.conf.json
index 5585818..78c8746 100644
--- a/src-tauri/tauri.conf.json
+++ b/src-tauri/tauri.conf.json
@@ -29,6 +29,9 @@
   },
   "bundle": {
     "active": true,
+    "resources": [
+      "../profiles/*.json"
+    ],
     "targets": "all",
     "licenseFile": "../LICENSE",
     "icon": [
```

## Acceptance criteria
- `npm run gate` green.
- `cargo test -p app` goes 12 -> **19** tests (the live test is ignored and does not count).
- `cargo test -p app -- --ignored` is **not** run by the gate.
- **No non-ASCII characters anywhere in the diff.**

## Out of scope
Installing anything (T-111c), and every line of frontend (T-111d/e). `mcp-bridge` is **not**
modified -- `list_models_in` already exists from T-105a, and that crate must stay free of a
`create-core` dependency.

## If unclear
Follow the reference implementation exactly. In particular do not collapse `Readiness`'s four
arms, and do not make a folder that failed to list abort the whole inventory.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/create-core/src/readiness.rs --read crates/mcp-bridge/src/models.rs --file src-tauri/src/models.rs --file src-tauri/src/lib.rs --file src-tauri/src/comfy.rs --file src-tauri/tauri.conf.json
```
