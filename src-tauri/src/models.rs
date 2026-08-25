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
