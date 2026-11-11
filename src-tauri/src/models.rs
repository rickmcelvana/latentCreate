//! The wizard's models step: which profiles can actually run, and what is
//! missing from the ones that cannot.
//!
//! The backend classifies; the frontend renders. As with the ComfyUI step, no
//! service problem returns `Err` -- a stopped ComfyUI, an unreadable profile
//! directory and a half-installed model are all states with a next step.
//!
//! Readiness is exact filename membership in ComfyUI's own `/models/<folder>`
//! lists -- what the loader nodes will accept -- and nothing else. Those lists
//! refresh live (COMFY-HTTP-SURFACE 3), so a file the app (or the user) puts in
//! ComfyUI's folder reads Installed on the next check, with no restart. That
//! is the T-519 fix for installs that finished and never read as installed.

use std::collections::{BTreeMap, BTreeSet};

use comfy_bridge::ComfyHttp;
use create_core::profile::{ModelKind, ModelProfile};
use create_core::readiness::{ModelInventory, ProfileReadiness};
use library::profiles::{ProfileSource, ProfileWarning};
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
    /// The gallery template this profile rides, when it rides one
    /// (`ComfySpec.template`). The model catalog joins on it: a gallery row whose
    /// `name` equals this is the same model, so the catalog shows this profile's
    /// readiness and install instead of the row's own `local_check`. `None` for a
    /// profile that uses an imported workflow rather than a gallery template.
    pub template: Option<String>,
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
    /// Why the list could not be taken, when it could not -- already a
    /// sentence with a next step.
    pub inventory_detail: Option<String>,
    /// One-click Install can work: ComfyUI is on this machine. False for a
    /// ComfyUI at another address, where the files must be placed there.
    pub install_available: bool,
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
) -> Result<ModelsView, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));

    let wanted: BTreeSet<String> = set
        .profiles
        .values()
        .flat_map(|loaded| loaded.profile.comfy.models.iter())
        .map(|spec| spec.folder.clone())
        .collect();

    let (inventory, inventory_detail, install_available) =
        match take_inventory(&state, &config_dir, &wanted).await {
            Ok((inventory, comfy)) => (Some(inventory), None, comfy.is_loopback()),
            Err(TakeError::Comfy(detail)) => (None, Some(detail), false),
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
        install_available,
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
        template: profile.comfy.template,
        readiness,
    }
}

/// Why the inventory could not be taken.
pub(crate) enum TakeError {
    /// No usable ComfyUI -- the sentence to show. Becomes `Unknown` rows.
    Comfy(String),
    /// This app could not open its own session log.
    Log(String),
}

/// List every folder the profiles name, and nothing else, and hand back the
/// client that answered (install needs it).
///
/// A folder ComfyUI does not know answers `404` and is left out, so its files
/// read as absent. That is the safe direction: it can say "missing" for
/// something present, which the user can see and correct, where the reverse
/// would call a broken install ready. Nothing listing at all is "could not
/// check", never an empty install.
pub(crate) async fn take_inventory(
    state: &State<'_, ComfyState>,
    config_dir: &State<'_, ConfigDir>,
    folders: &BTreeSet<String>,
) -> Result<(ModelInventory, std::sync::Arc<ComfyHttp>), TakeError> {
    let comfy = ensure_connected(state, config_dir)
        .await
        .map_err(|e| match e {
            EnsureError::Comfy(detail) => TakeError::Comfy(detail),
            EnsureError::Log(detail) => TakeError::Log(detail),
        })?;
    let inventory = inventory_of(&comfy, folders)
        .await
        .map_err(TakeError::Comfy)?;
    Ok((inventory, comfy))
}

/// The folder listings, through one client.
pub(crate) async fn inventory_of(
    comfy: &ComfyHttp,
    folders: &BTreeSet<String>,
) -> Result<ModelInventory, String> {
    let mut listed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for folder in folders {
        match comfy.models_in(folder).await {
            Ok(files) => {
                listed.insert(folder.clone(), files.into_iter().collect());
            }
            Err(e) if e.is_unreachable() => return Err(e.to_string()),
            Err(_) => {}
        }
    }
    if listed.is_empty() && !folders.is_empty() {
        return Err(
            "ComfyUI answered but listed none of the model folders. Update ComfyUI, then Retry."
                .to_string(),
        );
    }
    Ok(ModelInventory::new(listed))
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

    /// Protects: the row carries its gallery template name, the key the model
    /// catalog joins a gallery row to a shipped profile on. A drift here (or a
    /// dropped field) silently un-curates every catalog row -- it falls back to
    /// `local_check`, resurrecting the MiniMax `runnable: false` bug.
    #[test]
    fn test_a_row_carries_its_gallery_template() {
        let ace = row(profile(ACE_STEP), ProfileSource::Shipped, None);
        assert_eq!(ace.template.as_deref(), Some("audio_ace_step1_5_xl_turbo"));

        let minimax = row(profile(MINIMAX), ProfileSource::Shipped, None);
        assert_eq!(minimax.template.as_deref(), Some("audio_minimax_music_3"));
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

    /// Protects: the inventory comes from ComfyUI's own folder lists -- the
    /// live `/models/diffusion_models` answer -- and a folder ComfyUI does not
    /// know is left out rather than failing the whole check.
    #[tokio::test]
    async fn test_inventory_reads_comfyuis_own_folder_lists() {
        use comfy_bridge::transport::{FakeTransport, Method};
        let fake = std::sync::Arc::new(FakeTransport::new());
        let path = format!(
            "{}/../testdata/comfy-http/models_diffusion_models.json",
            env!("CARGO_MANIFEST_DIR")
        );
        fake.on(
            Method::Get,
            "/models/diffusion_models",
            200,
            std::fs::read_to_string(path).expect("f"),
        );
        fake.on(Method::Get, "/models/unet", 404, "");
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake, None);
        let folders: BTreeSet<String> = ["diffusion_models", "unet"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let inventory = inventory_of(&comfy, &folders).await.expect("listed");
        assert!(inventory.has("diffusion_models", "acestep_v1.5_xl_turbo_bf16.safetensors"));
        assert!(!inventory.has("unet", "anything"));
    }

    /// Protects: a stopped ComfyUI is "could not check" with a next step --
    /// never an empty install that tells the user to download 18 GiB again.
    #[tokio::test]
    async fn test_a_stopped_comfyui_cannot_be_inventoried() {
        use comfy_bridge::transport::{FakeTransport, Method, TransportError};
        let fake = std::sync::Arc::new(FakeTransport::new());
        fake.fail(
            Method::Get,
            "/models/vae",
            TransportError::Connect("refused".into()),
        );
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake, None);
        let folders: BTreeSet<String> = ["vae".to_string()].into();
        let err = inventory_of(&comfy, &folders).await.expect_err("down");
        assert!(err.contains("Start ComfyUI"), "{err}");
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
