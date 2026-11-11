//! The Setup ComfyUI step: find the user's ComfyUI and say what was found.
//!
//! The backend classifies; the frontend renders. Every outcome is a
//! [`ComfyStatus`] variant rather than an error string the UI has to parse,
//! because CONVENTIONS requires degraded services to degrade into a status pill
//! with a next step -- never a modal wall, never a raw error.

use std::sync::Arc;

use comfy_bridge::{discover, ComfyError, ComfyHttp, SessionLog, SystemStats, DEFAULT_URLS};
use serde::Serialize;
use tauri::State;

use crate::jobs::ComfyState;
use crate::ConfigDir;

/// What the Setup card shows for ComfyUI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ComfyStatus {
    /// No ComfyUI answered at any address tried. The card lists them and says
    /// "Start ComfyUI, then Retry" -- or, for a configured address, to check it.
    NotFound {
        /// Every address probed, in order.
        tried: Vec<String>,
        /// The user set the address (so only it was tried).
        configured: bool,
    },
    /// Something answered, but not in a way the app can use -- not a ComfyUI,
    /// or a version whose responses it cannot read.
    Unreachable {
        /// The address that answered.
        url: String,
        /// The sentence to show.
        detail: String,
    },
    /// ComfyUI is up.
    Ready {
        /// The address it answered at.
        url: String,
        /// Total VRAM in bytes. `None` is unknown, never zero.
        vram_bytes: Option<u64>,
        /// `"0.45.2"`.
        comfyui_version: Option<String>,
        /// On this machine, by address -- the condition for one-click Install.
        local: bool,
    },
}

impl ComfyStatus {
    /// Classify a successful probe.
    fn ready(comfy: &ComfyHttp, stats: &SystemStats) -> Self {
        ComfyStatus::Ready {
            url: comfy.base_url().to_string(),
            vram_bytes: stats.vram_bytes(),
            comfyui_version: stats.system.comfyui_version.clone(),
            local: comfy.is_loopback(),
        }
    }

    /// Classify a failed probe of every candidate.
    fn from_attempts(tried: Vec<(String, ComfyError)>, configured: bool) -> Self {
        // An address that answered with something unusable is the more useful
        // thing to report than the ports nobody was listening on.
        if let Some((url, e)) = tried.iter().find(|(_, e)| !e.is_unreachable()) {
            return ComfyStatus::Unreachable {
                url: url.clone(),
                detail: e.to_string(),
            };
        }
        ComfyStatus::NotFound {
            tried: tried.into_iter().map(|(url, _)| url).collect(),
            configured,
        }
    }
}

/// The addresses to probe: the user's when set, else the defaults.
pub(crate) fn candidates(configured: Option<&str>) -> (Vec<String>, bool) {
    match configured.map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => (vec![url.to_string()], true),
        None => (DEFAULT_URLS.iter().map(|u| u.to_string()).collect(), false),
    }
}

/// Find ComfyUI now and report what the Setup card should show.
///
/// `url` tests an address before it is saved; otherwise the configured one is
/// used, and with none configured the defaults are probed. The found client
/// replaces any earlier one, so Retry after starting ComfyUI is all it takes.
///
/// **Never returns `Err` for a service problem**; `Err` is reserved for this
/// app failing to open its own session log.
#[tauri::command]
pub async fn comfy_status(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    url: Option<String>,
) -> Result<ComfyStatus, String> {
    let _guard = state.connect.lock().await;
    let configured = match url {
        Some(u) if !u.trim().is_empty() => Some(u),
        _ => library::config::load(&config_dir.0).config.comfy.url,
    };
    let (urls, is_configured) = candidates(configured.as_deref());
    let log = open_log(&config_dir).map_err(EnsureError::into_message)?;
    match discover(&urls, |u| ComfyHttp::new(u, Some(log.clone()))).await {
        Ok((comfy, stats)) => {
            let status = ComfyStatus::ready(&comfy, &stats);
            state.store(comfy).await;
            Ok(status)
        }
        Err(tried) => {
            state.forget().await;
            Ok(ComfyStatus::from_attempts(tried, is_configured))
        }
    }
}

/// Why `ensure_connected` gave up.
pub(crate) enum EnsureError {
    /// No usable ComfyUI: the sentence to show, with its next step.
    Comfy(String),
    /// This app could not open its own session log. Genuinely our fault.
    Log(String),
}

impl EnsureError {
    /// The message, whichever kind it is.
    pub(crate) fn into_message(self) -> String {
        match self {
            EnsureError::Comfy(m) | EnsureError::Log(m) => m,
        }
    }
}

fn open_log(config_dir: &ConfigDir) -> Result<SessionLog, EnsureError> {
    SessionLog::open(config_dir.0.join("session.log")).map_err(|e| EnsureError::Log(e.to_string()))
}

/// The connected client, probing on first use.
///
/// Holds the connect lock across the check-and-probe so two first-use
/// commands (the Setup view's ComfyUI and Models steps probe on mount) do not
/// both probe and store.
pub(crate) async fn ensure_connected(
    state: &State<'_, ComfyState>,
    config_dir: &State<'_, ConfigDir>,
) -> Result<Arc<ComfyHttp>, EnsureError> {
    let _guard = state.connect.lock().await;
    if let Some(comfy) = state.connected().await {
        return Ok(comfy);
    }
    let configured = library::config::load(&config_dir.0).config.comfy.url;
    let (urls, is_configured) = candidates(configured.as_deref());
    let log = open_log(config_dir)?;
    match discover(&urls, |u| ComfyHttp::new(u, Some(log.clone()))).await {
        Ok((comfy, _)) => Ok(state.store(comfy).await),
        Err(tried) => Err(EnsureError::Comfy(
            match ComfyStatus::from_attempts(tried, is_configured) {
                ComfyStatus::Unreachable { detail, .. } => detail,
                _ => not_found_sentence(&urls),
            },
        )),
    }
}

/// "No ComfyUI answered at ..." with every address tried.
pub(crate) fn not_found_sentence(urls: &[String]) -> String {
    format!(
        "No ComfyUI answered at {}. Start ComfyUI, then Retry.",
        urls.join(" or ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use comfy_bridge::transport::{FakeTransport, Method, TransportError};

    fn stats() -> SystemStats {
        let path = format!(
            "{}/../testdata/comfy-http/system_stats.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("JSON")
    }

    #[test]
    fn test_candidates_prefer_the_configured_address() {
        assert_eq!(
            candidates(Some(" http://gpu:8188 ")),
            (vec!["http://gpu:8188".to_string()], true)
        );
        let (defaults, configured) = candidates(None);
        assert_eq!(defaults, ["http://127.0.0.1:8188", "http://127.0.0.1:8000"]);
        assert!(!configured);
        assert!(!candidates(Some("  ")).1, "a blank address is no address");
    }

    /// Protects: the live payload becomes the Ready pill, local by address.
    #[test]
    fn test_a_running_comfyui_is_ready_with_version_vram_and_locality() {
        let fake = Arc::new(FakeTransport::new());
        let comfy = ComfyHttp::with_transport("127.0.0.1:8188", fake.clone(), None);
        assert_eq!(
            ComfyStatus::ready(&comfy, &stats()),
            ComfyStatus::Ready {
                url: "http://127.0.0.1:8188".into(),
                vram_bytes: Some(17_074_749_440),
                comfyui_version: Some("0.45.2".into()),
                local: true,
            }
        );
        let remote = ComfyHttp::with_transport("http://192.168.1.20:8188", fake, None);
        assert!(matches!(
            ComfyStatus::ready(&remote, &stats()),
            ComfyStatus::Ready { local: false, .. }
        ));
    }

    /// Protects: nothing listening is NotFound listing every address -- the
    /// state the owner's other PCs were in, which used to read "Unreachable"
    /// with a comfy-cli traceback behind it.
    #[test]
    fn test_nobody_listening_is_not_found_with_every_address() {
        let tried = DEFAULT_URLS
            .iter()
            .map(|u| {
                (
                    u.to_string(),
                    ComfyError::Unreachable {
                        url: u.to_string(),
                        detail: "refused".into(),
                    },
                )
            })
            .collect();
        assert_eq!(
            ComfyStatus::from_attempts(tried, false),
            ComfyStatus::NotFound {
                tried: DEFAULT_URLS.iter().map(|u| u.to_string()).collect(),
                configured: false,
            }
        );
    }

    #[tokio::test]
    async fn test_something_that_is_not_comfyui_is_reported_as_such() {
        let fake = Arc::new(FakeTransport::new());
        fake.on(Method::Get, "/system_stats", 200, r#"{"hello": "world"}"#);
        fake.fail(Method::Get, "/never", TransportError::Timeout);
        let comfy = ComfyHttp::with_transport("127.0.0.1:8000", fake, None);
        let err = comfy.system_stats().await.expect_err("not comfy");
        match ComfyStatus::from_attempts(vec![("http://127.0.0.1:8000".into(), err)], false) {
            ComfyStatus::Unreachable { url, .. } => assert_eq!(url, "http://127.0.0.1:8000"),
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn test_not_found_sentence_names_every_address_and_the_next_step() {
        let text = not_found_sentence(&[
            "http://127.0.0.1:8188".into(),
            "http://127.0.0.1:8000".into(),
        ]);
        assert_eq!(
            text,
            "No ComfyUI answered at http://127.0.0.1:8188 or http://127.0.0.1:8000. Start ComfyUI, then Retry."
        );
    }
}
