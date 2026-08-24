//! Local backend: `comfy-mcp` spawned as a stdio child process.

use std::time::Duration;

use rmcp::{
    model::{CallToolRequestParams, CallToolResult},
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
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
        let transport = TokioChildProcess::new(tokio::process::Command::new(bin).configure(|c| {
            c.env("PYTHONIOENCODING", "utf-8");
        }))
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ComfyError::NotInstalled,
            _ => ComfyError::Spawn(e),
        })?;
        let service = ().serve(transport).await.map_err(|e| ComfyError::Transport(e.to_string()))?;
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
