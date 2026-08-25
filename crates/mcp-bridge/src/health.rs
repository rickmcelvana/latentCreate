//! `server_info` typed, and `launch_comfyui`.
//!
//! The wizard's ComfyUI step is built entirely on these two calls: one says
//! what is running and how healthy it is, the other starts it.
//!
//! Shapes verified live 2026-08-24 against comfy-cli 1.16.0 --
//! docs/MCP-SURFACE.md section 13.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// Whether ComfyUI itself is up, and where.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningServer {
    #[serde(default)]
    pub running: bool,
    /// Base URL when running, e.g. `http://127.0.0.1:8188`.
    #[serde(default)]
    pub url: Option<String>,
}

/// The GPU comfy-cli reports, when it reports one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    #[serde(default)]
    pub vendor: Option<String>,
    /// Marketing name, e.g. `NVIDIA GeForce RTX 5060 Ti`.
    #[serde(default)]
    pub model: Option<String>,
    /// Total VRAM in bytes. This is the number a profile's `vram_gb_min` is
    /// checked against, so it is kept in bytes and converted at the edge --
    /// 17102733312 bytes is a "16 GB" card, which is 15.9 GiB.
    #[serde(default)]
    pub vram_bytes: Option<u64>,
    #[serde(default)]
    pub unified_memory: bool,
}

/// Host hardware, captured once when comfy-mcp starts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hardware {
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub os_version: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub cpu: Option<String>,
    /// System RAM in bytes.
    #[serde(default)]
    pub ram_bytes: Option<u64>,
    #[serde(default)]
    pub gpu: Option<GpuInfo>,
}

/// The ComfyUI install comfy-cli is pointed at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub path: Option<String>,
    /// `default`, or another selection mode.
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
}

/// comfy-mcp's own version handshake.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compatibility {
    #[serde(default)]
    pub comfy_cli_version: Option<String>,
    /// Non-fatal complaints. A hard incompatibility raises before the call
    /// returns at all, so anything here is advisory -- show it, do not block.
    #[serde(default)]
    pub warnings: Vec<Value>,
}

/// Installed-versus-latest for ComfyUI core.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreFreshness {
    #[serde(default)]
    pub installed: Option<String>,
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub outdated: bool,
}

/// Update status for core and node packs.
///
/// WARNING: polymorphic -- an older comfy-cli answers `{"unsupported": true}` with no
/// `core` block at all. That means "could not check", **not** "up to date" --
/// see [`Freshness::update_available`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freshness {
    #[serde(default)]
    pub unsupported: bool,
    #[serde(default)]
    pub core: Option<CoreFreshness>,
    #[serde(default)]
    pub packs: Vec<Value>,
}

impl Freshness {
    /// Whether a core update is known to be available.
    ///
    /// False when the check is unsupported, because "we could not ask" must
    /// not render as an update badge the user can never clear.
    pub fn update_available(&self) -> bool {
        if self.unsupported {
            return false;
        }
        self.core.as_ref().is_some_and(|core| core.outdated)
    }
}

/// What `server_info` reports.
///
/// Every block is optional: `hardware` is absent on comfy-cli builds that do
/// not report one, and `freshness` can be a "could not check" marker. A
/// missing block means **unknown**, never "none" -- the wizard says so rather
/// than inventing a value.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(default)]
    pub server: Option<RunningServer>,
    #[serde(default)]
    pub hardware: Option<Hardware>,
    #[serde(default)]
    pub workspace: Option<Workspace>,
    #[serde(default)]
    pub compatibility: Option<Compatibility>,
    #[serde(default)]
    pub freshness: Option<Freshness>,
}

impl ServerInfo {
    /// Whether ComfyUI is up.
    ///
    /// A missing `server` block is **not** running: comfy-mcp answering while
    /// ComfyUI is down is exactly the degraded state the wizard exists to
    /// show.
    pub fn is_running(&self) -> bool {
        self.server.as_ref().is_some_and(|s| s.running)
    }

    /// The running server's URL, when there is one.
    pub fn url(&self) -> Option<&str> {
        self.server.as_ref()?.url.as_deref()
    }

    /// Total VRAM in bytes, when comfy-cli reported a GPU.
    ///
    /// `None` means unknown, which the wizard must not render as zero -- a
    /// "0 GB VRAM" warning on a working machine reads as a broken app.
    pub fn vram_bytes(&self) -> Option<u64> {
        self.hardware.as_ref()?.gpu.as_ref()?.vram_bytes
    }

    /// Whether a ComfyUI core update is known to be available.
    pub fn update_available(&self) -> bool {
        self.freshness
            .as_ref()
            .is_some_and(Freshness::update_available)
    }
}

/// What `launch_comfyui` reports on success, captured live 2026-08-25:
///
/// ```json
/// { "background": true, "listen": "127.0.0.1", "port": 8188,
///   "url": "http://127.0.0.1:8188", "pid": 23404 }
/// ```
///
/// **There is no `ok` field**, whatever the tool's own docstring says. Every
/// field is optional here because comfy-mcp synthesises this envelope from
/// `comfy launch`'s plain-text output, so its shape is not a wire contract --
/// but a caller must never branch on a missing key to decide the launch
/// failed. Failure arrives as [`ComfyError`], not as a falsy field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaunchResult {
    /// Where the launched server is listening.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    /// Interface bound. Always loopback from this app, which passes no flags.
    #[serde(default)]
    pub listen: Option<String>,
    /// comfy-cli's recorded pid -- the handle its own `stop_comfyui` uses.
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub background: bool,
}

impl LocalComfy {
    /// Start the local ComfyUI, detached, returning once it is up.
    ///
    /// **Call [`LocalComfy::health`] first and only launch when
    /// [`ServerInfo::is_running`] is false.** A second launch fails with
    /// `[port_in_use]`, verified live:
    ///
    /// ```text
    /// comfy launch --background failed [port_in_use]: The 8188 port is
    /// already in use. A new ComfyUI server cannot be launched.
    /// ```
    ///
    /// That arrives as [`ComfyError::Tool`] with `code = "port_in_use"`, which
    /// usually means the user already has ComfyUI running -- a state to report,
    /// not an error to alarm them with.
    ///
    /// No arguments are passed. `extra_args` exists on the tool but every
    /// network-exposing flag needs the user's explicit confirmation, and
    /// ComfyUI has no authentication, so this app does not offer them.
    pub async fn launch(&self) -> Result<LaunchResult, ComfyError> {
        self.call("launch_comfyui", Map::new()).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::health::{Freshness, ServerInfo};
    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;
    use crate::ComfyError;

    /// The live `server_info` payload, captured 2026-08-24 from comfy-cli
    /// 1.16.0 (home-directory path replaced with `USER`).
    const SERVER_INFO: &str = include_str!("../../../testdata/mcp/server_info.json");

    fn captured() -> ServerInfo {
        serde_json::from_str(SERVER_INFO).expect("server_info decodes")
    }

    /// Protects: the health pill's three facts, read from the real payload.
    #[test]
    fn test_captured_server_info_decodes_the_health_facts() {
        let info = captured();
        assert!(info.is_running());
        assert_eq!(info.url(), Some("http://127.0.0.1:8188"));
        assert_eq!(info.vram_bytes(), Some(17_102_733_312));
        assert!(info.update_available(), "v0.33.3 against latest v0.33.4");
    }

    /// Protects: ComfyUI counts as up only when the block says so. comfy-mcp
    /// answering while ComfyUI is down is the degraded state the wizard exists
    /// for, and it arrives two ways -- no `server` block at all, which must not
    /// read as unknown-so-probably-fine, and a block that says `running: false`,
    /// which must not read as "a server block exists, so we are up".
    #[test]
    fn test_server_is_running_only_when_the_block_says_so() {
        let info: ServerInfo = serde_json::from_value(json!({})).expect("empty decodes");
        assert!(!info.is_running());
        assert_eq!(info.url(), None);

        let stopped: ServerInfo = serde_json::from_value(json!({
            "server": { "running": false }
        }))
        .expect("stopped decodes");
        assert!(!stopped.is_running());
    }

    /// Protects: unknown VRAM stays unknown. Rendering `None` as zero would
    /// put a "0 GB VRAM" warning on a working machine.
    #[test]
    fn test_missing_gpu_reports_unknown_vram_not_zero() {
        let info: ServerInfo = serde_json::from_value(json!({
            "hardware": { "os": "Linux", "ram_bytes": 1024 }
        }))
        .expect("decodes");
        assert_eq!(info.vram_bytes(), None);
    }

    /// Protects: the polymorphic `freshness`. An older comfy-cli answers
    /// `{"unsupported": true}` with no `core` block; treating that as
    /// "outdated" would show an update badge the user can never clear, and
    /// treating the whole payload as undecodable would break the health pill
    /// outright.
    #[test]
    fn test_unsupported_freshness_is_not_an_update_and_still_decodes() {
        let info: ServerInfo = serde_json::from_value(json!({
            "server": { "running": true, "url": "http://127.0.0.1:8188" },
            "freshness": { "unsupported": true }
        }))
        .expect("unsupported freshness decodes");
        assert!(info.is_running());
        assert!(!info.update_available());

        let freshness = info.freshness.expect("freshness block");
        assert!(freshness.unsupported);
        assert!(freshness.core.is_none());
    }

    /// Protects: `unsupported` outranks any `core` block shipped beside it.
    /// The early return in `update_available` is the entire rule -- without it
    /// a payload carrying both falls through to a stale `outdated` reading and
    /// raises a badge from a check that never actually ran.
    #[test]
    fn test_unsupported_freshness_outranks_a_stale_core_block() {
        let freshness: Freshness = serde_json::from_value(json!({
            "unsupported": true,
            "core": { "installed": "v0.33.3", "latest": "v0.33.4", "outdated": true }
        }))
        .expect("decodes");
        assert!(!freshness.update_available());
    }
    /// Protects: an up-to-date install shows no badge.
    #[test]
    fn test_current_core_reports_no_update() {
        let freshness: Freshness = serde_json::from_value(json!({
            "core": { "installed": "v0.33.4", "latest": "v0.33.4", "outdated": false },
            "packs": []
        }))
        .expect("decodes");
        assert!(!freshness.update_available());
    }

    /// Protects: the argument set. `launch_comfyui` takes none from this app --
    /// every flag it accepts exposes an unauthenticated ComfyUI to the network.
    #[tokio::test]
    async fn test_launch_sends_no_arguments() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "background": true,
            "listen": "127.0.0.1",
            "port": 8188,
            "url": "http://127.0.0.1:8188",
            "pid": 23404
        }))])
        .await;

        let result = client.launch().await.expect("launch succeeds");
        assert_eq!(result.url.as_deref(), Some("http://127.0.0.1:8188"));
        assert_eq!(result.port, Some(8188));
        assert_eq!(result.listen.as_deref(), Some("127.0.0.1"));

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("launch_comfyui"));
        assert_eq!(log[0]["arguments"], json!({}));
    }

    /// Protects: a launch succeeded when the call returned `Ok`, never because
    /// some field in the body said so. The live payload carries no `ok` key
    /// even though the tool's own docstring promises one, so a wrapper reading
    /// success out of the body reports every real launch as a failure.
    #[tokio::test]
    async fn test_launch_success_does_not_depend_on_any_field() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({}))]).await;
        let result = client
            .launch()
            .await
            .expect("an empty body is still a success");
        assert_eq!(result.url, None);
        assert_eq!(result.pid, None);
    }
    /// Protects: the verified already-running failure. It arrives as a tool
    /// error with the `port_in_use` code, which the wizard reports as "already
    /// running" rather than as a fault.
    #[tokio::test]
    async fn test_launch_on_a_used_port_is_a_coded_tool_error() {
        let (client, _recorded) = client_and_log(vec![Reply::ToolError(
            "comfy launch --background failed [port_in_use]: The 8188 port is already in use. \
             A new ComfyUI server cannot be launched."
                .into(),
        )])
        .await;

        match client.launch().await {
            Err(ComfyError::Tool { code, message, .. }) => {
                assert_eq!(code.as_deref(), Some("port_in_use"));
                assert!(message.contains("already in use"));
            }
            other => panic!("expected a coded tool error, got {other:?}"),
        }
    }
}
