# T-110b: Tauri commands for the ComfyUI step
**Depends:** T-110a | **Crate/dir:** `src-tauri`
**Files to create/modify:**
- `src-tauri/src/comfy.rs` (create)
- `src-tauri/src/jobs.rs` (modify: **one** new `impl ComfyState` block -- changed lines only)
- `src-tauri/src/lib.rs` (modify: one `mod`, two handler entries)

## Goal
Two commands, `comfy_status` and `comfy_launch`, that classify everything ComfyUI can be
doing into a value the frontend renders directly.

## Spec
Exactly the reference implementation below.

**The backend classifies; the frontend renders.** `ComfyStatus` is a serde-tagged union with
one variant per state the wizard shows, so the UI never parses an error string to decide what
to display. CONVENTIONS requires degraded services to degrade into a status pill with a next
step -- that is only possible if the states are enumerable, which is what this type is for.

**`comfy_status` never returns `Err` for a service problem.** A missing `comfy-mcp`, a dead
ComfyUI and a broken connection are all `Ok(ComfyStatus)`. The `Err` arm is reserved for this
app failing to open its own session log -- genuinely our fault, not the user's. That is why
`EnsureError` has two arms rather than being a plain `ComfyError`.

**Only a missing binary offers an install command.** `ComfyError::NotInstalled` (which
`LocalComfy::connect` already produces from `io::ErrorKind::NotFound`) becomes
`NotInstalled`; everything else becomes `Unreachable` with the reason attached. Telling
someone to reinstall when their ComfyUI merely crashed sends them down the wrong path.

**`[port_in_use]` is not a launch failure.** Verified live: launching while something already
holds 8188 fails with that code. It means something is already serving, so `comfy_launch`
ignores it and reports whatever the following health check finds (MCP-SURFACE 13.2).

**The connection is shared with the job pump.** `ensure_connected` reads and writes the same
`ComfyState` the job commands use, so connecting in the wizard leaves the app connected for
generation -- hence the two small accessors added to `jobs.rs` rather than a second
connection.

## Reference implementation
Transcribe verbatim. This compiles, `cargo fmt` is a no-op on it, `cargo clippy
--all-targets -- -D warnings` is clean, and its 5 tests pass.

### 1. `src-tauri/src/jobs.rs` -- one new block
**Do not reproduce the whole file.** Insert immediately **before** the
`/// Connect to `comfy-mcp`, replacing any existing connection.` doc comment, leaving every
other line untouched:

```rust
impl ComfyState {
    /// The connected backend, or `None` before the first successful connect.
    pub async fn connected(&self) -> Option<Arc<LocalComfy>> {
        self.comfy.read().await.clone()
    }

    /// Store a freshly connected backend, replacing any existing one, and
    /// hand back the shared handle.
    pub async fn store(&self, comfy: LocalComfy) -> Arc<LocalComfy> {
        let comfy = Arc::new(comfy);
        *self.comfy.write().await = Some(Arc::clone(&comfy));
        comfy
    }
}

```

### 2. `src-tauri/src/comfy.rs` (new file, complete)
```rust
//! The wizard's ComfyUI step: one status call, one launch call.
//!
//! The backend classifies; the frontend renders. Every way this can go wrong
//! becomes a [`ComfyStatus`] variant rather than an error string the UI has to
//! parse, because CONVENTIONS requires degraded services to degrade into a
//! status pill with a next step -- never a modal wall, never a raw error.

use mcp_bridge::{ComfyError, LocalComfy, ServerInfo, SessionLog};
use serde::Serialize;
use tauri::State;

use crate::jobs::ComfyState;
use crate::ConfigDir;

/// What the setup wizard shows for ComfyUI.
///
/// Ordered worst to best. Each variant carries exactly what its pill needs to
/// say, and every one of them has a next step the user can take.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ComfyStatus {
    /// `comfy-mcp` is not on PATH. The only state with an install step.
    NotInstalled {
        /// What to run, shown verbatim so it can be copied.
        install_command: String,
    },
    /// `comfy-mcp` exists but could not be spoken to.
    Unreachable { detail: String },
    /// `comfy-mcp` answers, but ComfyUI itself is not running. This is the
    /// state the Launch button exists for.
    ServerDown {
        /// Install comfy-cli is pointed at, so the user can tell which one
        /// would start.
        workspace: Option<String>,
    },
    /// ComfyUI is up.
    Ready {
        url: Option<String>,
        /// Total VRAM in bytes. `None` means comfy-cli did not report a GPU,
        /// which the UI shows as unknown rather than as zero.
        vram_bytes: Option<u64>,
        workspace: Option<String>,
        comfy_cli_version: Option<String>,
        /// A ComfyUI core update is available. A quiet badge, never a block.
        update_available: bool,
    },
}

impl ComfyStatus {
    /// The install command shown when `comfy-mcp` is missing.
    const INSTALL_COMMAND: &'static str = "pip install comfy-mcp";

    /// Classify a [`ServerInfo`] into what the wizard should show.
    fn from_info(info: &ServerInfo) -> Self {
        let workspace = info.workspace.as_ref().and_then(|w| w.path.clone());
        if !info.is_running() {
            return ComfyStatus::ServerDown { workspace };
        }
        ComfyStatus::Ready {
            url: info.url().map(str::to_string),
            vram_bytes: info.vram_bytes(),
            workspace,
            comfy_cli_version: info
                .compatibility
                .as_ref()
                .and_then(|c| c.comfy_cli_version.clone()),
            update_available: info.update_available(),
        }
    }

    /// Classify a failure. Only a missing binary is [`ComfyStatus::NotInstalled`];
    /// everything else is unreachable with the reason attached.
    fn from_error(error: &ComfyError) -> Self {
        match error {
            ComfyError::NotInstalled => ComfyStatus::NotInstalled {
                install_command: Self::INSTALL_COMMAND.to_string(),
            },
            other => ComfyStatus::Unreachable {
                detail: other.to_string(),
            },
        }
    }
}

/// Connect if needed, then report what the wizard should show.
///
/// **Never returns `Err` for a service problem.** A missing binary, a dead
/// ComfyUI and a broken connection are all states with a next step, so they
/// come back as `Ok(ComfyStatus)`; the `Err` arm is reserved for this app
/// failing to open its own session log.
#[tauri::command]
pub async fn comfy_status(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    bin: Option<String>,
) -> Result<ComfyStatus, String> {
    let comfy = match ensure_connected(&state, &config_dir, bin).await {
        Ok(comfy) => comfy,
        Err(EnsureError::Comfy(e)) => return Ok(ComfyStatus::from_error(&e)),
        Err(EnsureError::Log(detail)) => return Err(detail),
    };

    match comfy.health().await {
        Ok(info) => Ok(ComfyStatus::from_info(&info)),
        Err(e) => Ok(ComfyStatus::from_error(&e)),
    }
}

/// Start ComfyUI, then report the resulting status.
///
/// `[port_in_use]` is **not** surfaced as a failure: it means something is
/// already serving that port, so the honest answer is whatever the following
/// health check reports.
#[tauri::command]
pub async fn comfy_launch(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    bin: Option<String>,
) -> Result<ComfyStatus, String> {
    let comfy = match ensure_connected(&state, &config_dir, bin).await {
        Ok(comfy) => comfy,
        Err(EnsureError::Comfy(e)) => return Ok(ComfyStatus::from_error(&e)),
        Err(EnsureError::Log(detail)) => return Err(detail),
    };

    if let Err(e) = comfy.launch().await {
        if !is_port_in_use(&e) {
            return Ok(ComfyStatus::from_error(&e));
        }
    }

    match comfy.health().await {
        Ok(info) => Ok(ComfyStatus::from_info(&info)),
        Err(e) => Ok(ComfyStatus::from_error(&e)),
    }
}

/// Whether a launch failure was "something already holds the port".
fn is_port_in_use(error: &ComfyError) -> bool {
    matches!(error, ComfyError::Tool { code, .. } if code.as_deref() == Some("port_in_use"))
}

/// Why `ensure_connected` gave up.
enum EnsureError {
    /// A service problem, which becomes a status rather than an error.
    Comfy(ComfyError),
    /// This app could not open its own session log. Genuinely our fault.
    Log(String),
}

/// The connected backend, connecting on first use.
async fn ensure_connected(
    state: &State<'_, ComfyState>,
    config_dir: &State<'_, ConfigDir>,
    bin: Option<String>,
) -> Result<std::sync::Arc<LocalComfy>, EnsureError> {
    if let Some(comfy) = state.connected().await {
        return Ok(comfy);
    }
    let bin = bin.unwrap_or_else(|| "comfy-mcp".to_string());
    let log = SessionLog::open(config_dir.0.join("session.log"))
        .map_err(|e| EnsureError::Log(e.to_string()))?;
    let comfy = LocalComfy::connect(&bin, log)
        .await
        .map_err(EnsureError::Comfy)?;
    Ok(state.store(comfy).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcp_bridge::ServerInfo;

    const CAPTURED: &str = include_str!("../../testdata/mcp/server_info.json");

    /// Protects: the healthy path reads the live payload, including the two
    /// facts the pill shows beyond "up" -- VRAM and the update badge.
    #[test]
    fn test_running_server_classifies_as_ready() {
        let info: ServerInfo = serde_json::from_str(CAPTURED).expect("decodes");
        match ComfyStatus::from_info(&info) {
            ComfyStatus::Ready {
                url,
                vram_bytes,
                update_available,
                comfy_cli_version,
                ..
            } => {
                assert_eq!(url.as_deref(), Some("http://127.0.0.1:8188"));
                assert_eq!(vram_bytes, Some(17_102_733_312));
                assert!(update_available);
                assert_eq!(comfy_cli_version.as_deref(), Some("1.16.0"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    /// Protects: comfy-mcp answering while ComfyUI is down is its own state,
    /// not an error. This is the one the Launch button exists for, and it must
    /// carry the workspace so the user can see which install would start.
    #[test]
    fn test_stopped_server_classifies_as_server_down() {
        let info: ServerInfo = serde_json::from_value(serde_json::json!({
            "server": { "running": false },
            "workspace": { "path": "C:/Comfy/ComfyUI", "type": "default" }
        }))
        .expect("decodes");
        match ComfyStatus::from_info(&info) {
            ComfyStatus::ServerDown { workspace } => {
                assert_eq!(workspace.as_deref(), Some("C:/Comfy/ComfyUI"));
            }
            other => panic!("expected ServerDown, got {other:?}"),
        }
    }

    /// Protects: only a missing binary offers an install command. Every other
    /// failure is unreachable-with-a-reason, because telling a user to
    /// reinstall when their ComfyUI merely crashed sends them down the wrong
    /// path.
    #[test]
    fn test_only_a_missing_binary_offers_an_install_command() {
        match ComfyStatus::from_error(&ComfyError::NotInstalled) {
            ComfyStatus::NotInstalled { install_command } => {
                assert_eq!(install_command, "pip install comfy-mcp");
            }
            other => panic!("expected NotInstalled, got {other:?}"),
        }

        let spawn = ComfyError::Spawn(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ));
        match ComfyStatus::from_error(&spawn) {
            ComfyStatus::Unreachable { detail } => assert!(detail.contains("permission denied")),
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    /// Protects: the verified already-running launch failure is recognised, so
    /// the wizard reports what is actually there instead of an alarming error.
    #[test]
    fn test_port_in_use_is_recognised() {
        let busy = ComfyError::Tool {
            tool: "launch_comfyui".to_string(),
            code: Some("port_in_use".to_string()),
            message: "The 8188 port is already in use.".to_string(),
        };
        assert!(is_port_in_use(&busy));

        let other = ComfyError::Tool {
            tool: "launch_comfyui".to_string(),
            code: Some("workspace_not_found".to_string()),
            message: "nope".to_string(),
        };
        assert!(!is_port_in_use(&other));
    }

    /// Protects: the status crosses the Tauri boundary as a tagged union the
    /// frontend can switch on, with snake_case tags. A rename here silently
    /// breaks every branch of the UI.
    #[test]
    fn test_status_serialises_as_a_tagged_union() {
        let json =
            serde_json::to_value(ComfyStatus::ServerDown { workspace: None }).expect("serialises");
        assert_eq!(json["state"], serde_json::json!("server_down"));

        let json = serde_json::to_value(ComfyStatus::NotInstalled {
            install_command: "pip install comfy-mcp".to_string(),
        })
        .expect("serialises");
        assert_eq!(json["state"], serde_json::json!("not_installed"));
        assert_eq!(
            json["install_command"],
            serde_json::json!("pip install comfy-mcp")
        );
    }
}
```

### 3. `src-tauri/src/lib.rs` -- two edits
Add `mod comfy;` immediately before `mod jobs;`. Then add two handler entries immediately
before `jobs::connect_comfy,` in the `generate_handler!` list:

```rust
            comfy::comfy_status,
            comfy::comfy_launch,
```

## Acceptance criteria
- [ ] `cargo test -p app` passes: **12 tests** (7 before, 5 here)
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `npm run gate` green
- [ ] no changes outside the three listed files; **no new dependencies**
- [ ] **no non-ASCII characters anywhere in the diff** (CONVENTIONS)

## Out of scope
- Any frontend (T-110c).
- Persisting the ComfyUI setting to config -- `bin` is passed per call; the wizard writes
  config in a later task.
- Removing `jobs::connect_comfy`. It stays; the frontend jobs bridge still calls it.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/health.rs --read crates/mcp-bridge/src/error.rs --file src-tauri/src/comfy.rs --file src-tauri/src/jobs.rs --file src-tauri/src/lib.rs
```
`health.rs` and `error.rs` are `--read` because the reference code reads `ServerInfo` and
matches on `ComfyError` variants. Neither may be edited (WORKFLOW 3).
