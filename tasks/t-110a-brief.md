# T-110a: typed `server_info`, and `launch_comfyui`
**Depends:** T-101 | **Crate/dir:** `crates/mcp-bridge`
**Files to create/modify:**
- `crates/mcp-bridge/src/health.rs` (create)
- `crates/mcp-bridge/src/types.rs` (modify: **delete** the old `ServerInfo`; keep `SystemStats`)
- `crates/mcp-bridge/src/local.rs` (modify: **two** import lines)
- `crates/mcp-bridge/src/lib.rs` (modify: one `mod`, one re-export block, one re-export line)

## Goal
The two calls the setup wizard's ComfyUI step is built on: a `server_info` typed against the
real payload, and `launch_comfyui`. Read [docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md)
section 13 first -- every rule below is a captured fact.

## Spec
Exactly the reference implementation below.

**The old `ServerInfo` was written before the payload was seen.** It modelled three blocks as
opaque `Value`s; the live payload carries seven, and four of them drive the wizard: `server`
(running + url), `hardware` (`gpu.vram_bytes`, the number a profile's `vram_gb_min` is checked
against), `workspace` (which install would start), `compatibility` (comfy-cli version), and
`freshness` (the quiet update badge). It moves to `health.rs` and gains real types.

**Three rules that are correctness, not modelling taste:**

- **A missing `server` block is not running.** comfy-mcp answers happily while ComfyUI itself
  is down -- exactly the degraded state the wizard exists to show. Reading absence as
  "unknown, probably fine" hides it.
- **Absent VRAM stays absent.** `hardware` is missing on comfy-cli builds that do not report
  one, so `vram_bytes()` returns `Option<u64>`. Rendering that as `0` puts a hardware warning
  on a working machine.
- **`freshness` is polymorphic.** An older comfy-cli answers `{"unsupported": true}` with no
  `core` block. That means "could not check", **not** "up to date": treating it as outdated
  shows a badge the user can never clear, and failing to decode it breaks the health pill
  outright. `update_available()` encodes the distinction.

**`launch` passes no arguments.** The tool accepts `extra_args`, but every network-exposing
flag publishes an *unauthenticated* ComfyUI API to the network, so this app does not offer
them. A launch onto a busy port fails with `[port_in_use]`, which the caller treats as
"something is already serving" rather than as a fault -- that handling is T-110b's, not here.

## Fixture
`testdata/mcp/server_info.json` is already committed -- the live payload, with the
home-directory path replaced by `USER` and nothing else changed. **Do not edit it**; the
tests assert its exact values (`17102733312` VRAM, comfy-cli `1.16.0`, core outdated).

## Reference implementation
Transcribe verbatim. This compiles, `cargo fmt` is a no-op on it, `cargo clippy
--all-targets -- -D warnings` is clean, and its 7 tests pass.

### 1. `crates/mcp-bridge/src/health.rs` (new file, complete)
```rust
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

/// What `launch_comfyui` reports on success.
///
/// comfy-mcp synthesises this envelope because `comfy launch` itself prints
/// plain text. Only `ok` is relied on.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaunchResult {
    #[serde(default)]
    pub ok: bool,
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

    /// Protects: comfy-mcp answering while ComfyUI is down is the degraded
    /// state the wizard exists for. An absent `server` block must read as
    /// not-running, never as unknown-so-probably-fine.
    #[test]
    fn test_absent_server_block_is_not_running() {
        let info: ServerInfo = serde_json::from_value(json!({})).expect("empty decodes");
        assert!(!info.is_running());
        assert_eq!(info.url(), None);
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
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({ "ok": true }))]).await;

        let result = client.launch().await.expect("launch succeeds");
        assert!(result.ok);

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("launch_comfyui"));
        assert_eq!(log[0]["arguments"], json!({}));
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
```

### 2. `crates/mcp-bridge/src/types.rs` -- delete the old `ServerInfo`
Remove the `ServerInfo` doc comment and struct entirely (the block from
`/// Subset of `server_info` the app actually uses.` through its closing brace). Keep the
file's imports and `SystemStats` exactly as they are -- `Value` is still used.

### 3. `crates/mcp-bridge/src/local.rs` -- two import lines
Replace the single line

```rust
use crate::types::{ServerInfo, SystemStats};
```

with

```rust
use crate::health::ServerInfo;
use crate::types::SystemStats;
```

and inside `mod tests`, replace `use crate::types::ServerInfo;` with
`use crate::health::ServerInfo;`. Nothing else in the file changes; the existing
`test_health_decodes_the_captured_server_info` still passes against the new type.

### 4. `crates/mcp-bridge/src/lib.rs` -- three edits
Add `mod health;` after `mod error;`. Add this re-export block immediately **before**
`pub use jobs::`:

```rust
pub use health::{
    Compatibility, CoreFreshness, Freshness, GpuInfo, Hardware, LaunchResult, RunningServer,
    ServerInfo, Workspace,
};
```

and change `pub use types::{ServerInfo, SystemStats};` to `pub use types::SystemStats;`.

## Acceptance criteria
- [ ] `cargo test -p mcp-bridge` passes: **86 tests** (79 before, 7 here)
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `npm run gate` green
- [ ] no changes outside the four listed files; **no new dependencies**
- [ ] **no non-ASCII characters anywhere in the diff** (CONVENTIONS; this has cost a review
      round on each of the last three tasks)

## Out of scope
- Tauri commands and the status classification (T-110b).
- Any UI (T-110c).
- `system_stats` typing -- `SystemStats` stays as it is.
- `stop_comfyui` / `update_comfyui`, neither captured nor needed yet.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/error.rs --read crates/mcp-bridge/src/mock.rs --file crates/mcp-bridge/src/health.rs --file crates/mcp-bridge/src/types.rs --file crates/mcp-bridge/src/local.rs --file crates/mcp-bridge/src/lib.rs
```
`error.rs` and `mock.rs` are `--read`: the tests build `ComfyError::Tool` and `Reply` values.
Neither may be edited (WORKFLOW 3).
