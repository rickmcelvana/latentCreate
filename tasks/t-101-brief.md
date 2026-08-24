# T-101: `mcp-bridge` foundation — stdio transport, `ComfyError`, typed health
**Depends:** the rmcp verification (done 2026-08-23, docs/MCP-SURFACE.md §8) | **Dirs:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to modify:** `Cargo.toml` (workspace `rust-version` only), `crates/mcp-bridge/Cargo.toml`, `crates/mcp-bridge/src/lib.rs`

**Files to create:** `crates/mcp-bridge/src/error.rs`, `crates/mcp-bridge/src/local.rs`, `crates/mcp-bridge/src/types.rs`

## Goal
`mcp-bridge` can spawn `comfy-mcp` over stdio, complete the MCP handshake, call a tool, and
return typed results or a typed error. Only `server_info` and `system_stats` are wrapped
here — templates, slots, jobs and models are T-103/T-104/T-105. Local backend only; the
cloud backend is a later task gated on verifying a live cloud endpoint (ARCHITECTURE §3).

## This surface is verified, not recalled
Every line of reference code below was compiled against rmcp 3.1.4 and **run against the
owner's live `comfy-mcp`** on 2026-08-23. It passes `cargo clippy --all-targets -- -D
warnings` under edition 2021. Findings and evidence: **docs/MCP-SURFACE.md §8** — read it
before starting. Three of them are traps that look fine in review:

1. **`CallToolRequestParams` is `#[non_exhaustive]`.** A struct literal is compile error
   E0639. Use `::new(name).with_arguments(map)`. It is also **plural** in 3.x.
2. **A failing tool returns `Ok`, not `Err`** — bad arguments, missing files, *and unknown
   tool names* all arrive as `Ok(CallToolResult { is_error: Some(true), .. })`. Code that
   only matches `Result::Err` treats every ComfyUI failure as success.
3. **Results are JSON-in-a-text-block.** `structured_content` is always `None` and no tool
   publishes an `output_schema`. Decoding is two stages: pull `content[0]` as text, then
   `serde_json::from_str` into our own type.

## Dependencies to add (all permissive — no decisions-log entry needed)
`crates/mcp-bridge/Cargo.toml`:
```toml
[dependencies]
rmcp = { version = "3.1.4", default-features = false, features = ["client", "transport-child-process"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
tokio = { version = "1.53", features = ["process", "rt", "time"] }
```
`default-features = false` is deliberate: the default set pulls in rmcp's whole *server*
half plus `macros`/`schemars`/`uuid`/`base64`, none of which a client needs. Verified to
compile and run without them.

**Also bump the workspace `rust-version` in the root `Cargo.toml` from `1.85` to `1.88`.**
rmcp 3.1.4 declares `rust-version = "1.88"`; leaving 1.85 there makes the manifest claim
something untrue. CI runs `dtolnay/rust-toolchain@stable`, so nothing breaks. This is the
only change permitted in the root `Cargo.toml`.

## Reference code

### `crates/mcp-bridge/src/error.rs`
```rust
//! The one error type every mcp-bridge call returns.

use rmcp::service::ServiceError;

/// Everything that can go wrong talking to a local ComfyUI through `comfy-mcp`.
#[derive(Debug, thiserror::Error)]
pub enum ComfyError {
    /// The `comfy-mcp` executable is not installed or not on PATH.
    #[error("comfy-mcp is not installed. Install it with `pip install comfy-mcp`, then Retry.")]
    NotInstalled,
    /// The child process could not be spawned for some other reason.
    #[error("could not start comfy-mcp: {0}")]
    Spawn(#[source] std::io::Error),
    /// The MCP session failed at the transport or protocol level.
    #[error("comfy-mcp connection failed: {0}")]
    Transport(String),
    /// The tool ran and reported a failure. `code` is comfy-mcp's bracketed
    /// error slug when it emitted one (e.g. `workflow_not_found`).
    #[error("{tool} failed{}: {message}", .code.as_ref().map(|c| format!(" [{c}]")).unwrap_or_default())]
    Tool {
        /// Tool name as sent on the wire.
        tool: String,
        /// Machine-readable slug parsed out of the message, when present.
        code: Option<String>,
        /// Full human-readable text comfy-mcp returned.
        message: String,
    },
    /// A tool succeeded but its payload was not the JSON we expected.
    #[error("{tool} returned an unreadable payload: {detail}")]
    Payload {
        /// Tool name as sent on the wire.
        tool: String,
        /// What failed to parse.
        detail: String,
    },
}

impl From<ServiceError> for ComfyError {
    fn from(e: ServiceError) -> Self {
        ComfyError::Transport(e.to_string())
    }
}

/// Pull comfy-mcp's bracketed error slug out of a failure message.
///
/// Its failures read `... failed [workflow_not_found]: Workflow file not found`.
/// Pydantic argument errors bracket something else entirely
/// (`[type=missing, input_value={}]`) and must yield `None`.
pub(crate) fn parse_error_code(text: &str) -> Option<String> {
    let start = text.find('[')?;
    let end = text[start..].find(']')? + start;
    let code = &text[start + 1..end];
    let ok = !code.is_empty()
        && code
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit());
    if ok {
        Some(code.to_string())
    } else {
        None
    }
}
```

### `crates/mcp-bridge/src/types.rs`
```rust
//! Wire types decoded out of comfy-mcp's JSON-in-text payloads.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Subset of `server_info` the app actually uses.
///
/// Every field is optional: `server` is absent when ComfyUI itself is down,
/// and `hardware` is absent on older comfy-cli builds. A missing block means
/// "unknown", never "none" -- the setup wizard must ask rather than assume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Running-server block: `{"running": true, "url": "http://127.0.0.1:8188"}`.
    #[serde(default)]
    pub server: Option<Value>,
    /// comfy-cli's hardware snapshot, captured once at comfy-mcp start.
    #[serde(default)]
    pub hardware: Option<Value>,
    /// Resolved ComfyUI workspace on disk.
    #[serde(default)]
    pub workspace: Option<Value>,
}

/// `system_stats` payload: `{"devices": [...], "system": {...}}`.
///
/// Left as `Value` deliberately -- T-105 types the device blocks once the
/// VRAM-gating rules need them, and guessing the shape now would be recall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// Per-device blocks, each carrying `vram_free` among other keys.
    #[serde(default)]
    pub devices: Vec<Value>,
    /// Host-level block.
    #[serde(default)]
    pub system: Value,
}
```

### `crates/mcp-bridge/src/local.rs`
```rust
//! Local backend: `comfy-mcp` spawned as a stdio child process.

use std::time::Duration;

use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::error::{parse_error_code, ComfyError};
use crate::types::{ServerInfo, SystemStats};

/// A live `comfy-mcp` session.
///
/// The child process is killed when this is dropped -- rmcp's
/// `TokioChildProcess` owns that cleanup, so do not add a `Drop` impl here
/// (docs/MCP-SURFACE.md 8.2).
pub struct LocalComfy {
    service: RunningService<RoleClient, ()>,
}

impl LocalComfy {
    /// Spawn `comfy-mcp` over stdio and complete the MCP handshake.
    ///
    /// `bin` is the configured executable name or path; the default is
    /// `"comfy-mcp"` on PATH.
    pub async fn connect(bin: &str) -> Result<Self, ComfyError> {
        let transport =
            TokioChildProcess::new(tokio::process::Command::new(bin).configure(|c| {
                c.env("PYTHONIOENCODING", "utf-8");
            }))
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ComfyError::NotInstalled,
                _ => ComfyError::Spawn(e),
            })?;
        let service = ()
            .serve(transport)
            .await
            .map_err(|e| ComfyError::Transport(e.to_string()))?;
        Ok(Self { service })
    }

    /// Call a tool and decode its JSON-in-text payload.
    ///
    /// Handles both failure paths: `Err` from the transport, and the far more
    /// common `Ok(is_error: true)` that every comfy-mcp tool failure takes.
    pub async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        tool: &'static str,
        arguments: Map<String, Value>,
    ) -> Result<T, ComfyError> {
        let res: CallToolResult = self
            .service
            .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
            .await?;

        let text = res
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or_default();

        if res.is_error.unwrap_or(false) {
            return Err(ComfyError::Tool {
                tool: tool.to_string(),
                code: parse_error_code(text),
                message: text.to_string(),
            });
        }

        serde_json::from_str(text).map_err(|e| ComfyError::Payload {
            tool: tool.to_string(),
            detail: e.to_string(),
        })
    }

    /// `server_info` -- health, workspace, and the startup hardware snapshot.
    pub async fn health(&self) -> Result<ServerInfo, ComfyError> {
        self.call("server_info", Map::new()).await
    }

    /// `system_stats` -- live device/VRAM figures used to gate heavy runs.
    pub async fn stats(&self) -> Result<SystemStats, ComfyError> {
        self.call("system_stats", Map::new()).await
    }

    /// Close the session and wait for the child to exit.
    pub async fn shutdown(self) -> Result<(), ComfyError> {
        self.service
            .cancel()
            .await
            .map_err(|e| ComfyError::Transport(e.to_string()))?;
        Ok(())
    }
}

/// Bound a call so a wedged server cannot hang the UI.
///
/// rmcp's `call_tool` sends with `PeerRequestOptions::no_options()`, which sets
/// no timeout at all (docs/MCP-SURFACE.md 8.6). Long generations get
/// `send_cancellable_request` in T-104; cheap calls use this.
pub async fn with_timeout<T>(
    d: Duration,
    fut: impl std::future::Future<Output = Result<T, ComfyError>>,
) -> Result<T, ComfyError> {
    tokio::time::timeout(d, fut)
        .await
        .map_err(|_| ComfyError::Transport(format!("timed out after {}s", d.as_secs())))?
}
```

### `crates/mcp-bridge/src/lib.rs`
Replace the placeholder body. Keep the existing `test_crate_name_is_stable` test.
```rust
//! MCP client for a local ComfyUI via `comfy-mcp`.
//!
//! Implements the `ComfyBackend` seam (ARCHITECTURE.md 3) over stdio.
//! Tool names are the verified LOCAL ones -- see docs/MCP-SURFACE.md, never the
//! cloud documentation.

mod error;
mod local;
mod types;

pub use error::ComfyError;
pub use local::{with_timeout, LocalComfy};
pub use types::{ServerInfo, SystemStats};
```

## Tests
`crates/mcp-bridge/src/error.rs`, in `#[cfg(test)] mod tests` — **no test may spawn
`comfy-mcp` or need a running ComfyUI** (WORKFLOW §5; the mock rig is T-102).

- `test_parse_error_code_reads_the_slug` — **protects:** the slug is what the UI turns into
  an actionable message ("Start ComfyUI, then Retry"). Verified real input:
  `"comfy workflow slots x failed [workflow_not_found]: Workflow file not found"` yields
  `Some("workflow_not_found")`.
- `test_parse_error_code_rejects_a_non_slug_bracket` — **protects:** pydantic argument
  errors also contain brackets. Verified real input:
  `"Field required [type=missing, input_value={'path': 'x'}]"` must yield `None`, not
  `Some("type=missing, ...")`. A naive "text between brackets" reading passes the test above
  and produces garbage error codes on the most common failure the app will actually hit.
- `test_parse_error_code_is_none_without_brackets`.
- `test_tool_error_displays_the_code_when_present` — **protects:** the error a user sees
  must name the failure. Assert the `Display` of a `ComfyError::Tool` with
  `code: Some("workflow_not_found")` contains both the tool name and the slug, and that one
  with `code: None` does not render an empty `[]`.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All four named tests present and passing
- [ ] No test spawns a process or opens a socket
- [ ] Every public type, function and trait has a `///` doc comment (CONVENTIONS)
- [ ] No dependency beyond the five listed; workspace `rust-version` is the only root change

## Out of scope
The `ComfyBackend` trait itself — **deliberately deferred.** ARCHITECTURE §3 specifies it,
but with one impl it would be an untested abstraction, and the dyn-vs-enum dispatch question
(async fns in traits are not object-safe) should be decided when T-104 puts a backend into
Tauri managed state and knows what it needs. `LocalComfy`'s method names already match the
trait's, so extracting it later is mechanical. Also out: templates, slots, jobs, models,
node registry, any Tauri command, any UI, the cloud backend, and the session log.

## Notes for the executor
- Do not add a `Drop` impl for `LocalComfy`; rmcp already kills the child.
- Do not "fix" `CallToolRequestParams::new(..).with_arguments(..)` into a struct literal —
  the type is `#[non_exhaustive]` and it will not compile.
- Do not treat `Ok(res)` as success without checking `res.is_error`.
- `stats()` returns `SystemStats` with `Value` interiors on purpose. Do not invent typed
  device fields; that shape has not been verified yet.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --file Cargo.toml --file crates/mcp-bridge/Cargo.toml --file crates/mcp-bridge/src/lib.rs --file crates/mcp-bridge/src/error.rs --file crates/mcp-bridge/src/local.rs --file crates/mcp-bridge/src/types.rs
```
