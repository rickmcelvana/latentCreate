//! Local backend: `comfy-mcp` spawned as a stdio child process.

use std::time::Duration;

use rmcp::{
    model::{CallToolRequestParams, CallToolResult},
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, IntoTransport, TokioChildProcess},
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
        Self::from_transport(transport).await
    }

    /// Complete the MCP handshake over an already-built transport.
    ///
    /// The seam `connect` is written on top of, and the one the mock rig uses:
    /// any `AsyncRead + AsyncWrite` is a valid transport, so no test needs a
    /// child process or a socket. A future cloud backend enters the same way.
    pub async fn from_transport<T, E, A>(transport: T) -> Result<Self, ComfyError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
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

#[cfg(test)]
pub(crate) mod test_helpers {
    use tokio::io::duplex;

    use crate::mock::{spawn_mock, RecordedCalls, Reply};
    use crate::LocalComfy;

    pub async fn client_with(replies: Vec<Reply>) -> LocalComfy {
        client_and_log(replies).await.0
    }

    /// Same, but keeps the record of what the client sent.
    pub async fn client_and_log(replies: Vec<Reply>) -> (LocalComfy, RecordedCalls) {
        let (client_half, peer_half) = duplex(8 * 1024);
        let recorded = spawn_mock(peer_half, replies);
        let client = LocalComfy::from_transport(client_half)
            .await
            .expect("handshake over duplex");
        (client, recorded)
    }
}

#[cfg(test)]
mod transport_tests {
    use serde_json::json;

    use crate::local::test_helpers::{client_and_log, client_with};
    use crate::mock::Reply;
    use crate::types::ServerInfo;

    #[tokio::test]
    async fn test_handshake_completes_over_a_duplex_transport() {
        let _client = client_with(vec![]).await;
    }

    /// Protects: the bridge must send the tool name and argument names exactly
    /// as comfy-mcp spells them. It rejects a misnamed argument outright --
    /// `path` where it wants `workflow_path` (docs/MCP-SURFACE.md 8.7) -- so a
    /// wrapper that gets this wrong passes every response-only test here and
    /// fails only against a live server, which is what this rig exists to
    /// prevent. Asserting the reply alone cannot catch it: the mock answers the
    /// same canned payload whatever it is asked.
    #[tokio::test]
    async fn test_call_sends_the_tool_name_and_arguments_verbatim() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({ "ok": true }))]).await;
        let mut args = serde_json::Map::new();
        args.insert("workflow_path".into(), json!("wf.json"));
        let _: serde_json::Value = client
            .call("list_workflow_slots", args)
            .await
            .expect("call succeeds");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["name"], json!("list_workflow_slots"));
        assert_eq!(log[0]["arguments"]["workflow_path"], json!("wf.json"));
    }

    #[tokio::test]
    async fn test_ok_payload_decodes_from_the_text_block() {
        let client = client_with(vec![Reply::Json(json!({ "answer": 42 }))]).await;
        let res: serde_json::Value = client
            .call("any_tool", serde_json::Map::new())
            .await
            .expect("decodes JSON text block");
        assert_eq!(res.get("answer").and_then(|v| v.as_i64()), Some(42));
    }

    #[tokio::test]
    async fn test_is_error_becomes_tool_error() {
        let client = client_with(vec![Reply::ToolError(
            "comfy workflow slots x failed [workflow_not_found]: nope".into(),
        )])
        .await;
        let err = client
            .call::<serde_json::Value>("list_workflow_slots", serde_json::Map::new())
            .await
            .expect_err("tool failure should become ComfyError::Tool");
        match err {
            crate::ComfyError::Tool {
                tool,
                code,
                message,
            } => {
                assert_eq!(tool, "list_workflow_slots");
                assert_eq!(code, Some("workflow_not_found".to_string()));
                assert!(message.contains("nope"));
            }
            other => panic!("expected ComfyError::Tool, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_is_error_wins_over_a_decodable_payload() {
        let client = client_with(vec![Reply::ToolErrorJson(json!({ "answer": 42 }))]).await;
        let err = client
            .call::<serde_json::Value>("any_tool", serde_json::Map::new())
            .await
            .expect_err("is_error must win over decodable payload");
        assert!(matches!(err, crate::ComfyError::Tool { .. }));
    }

    #[tokio::test]
    async fn test_non_json_text_becomes_payload_error() {
        let client = client_with(vec![Reply::RawText("not json".into())]).await;
        let err = client
            .call::<serde_json::Value>("any_tool", serde_json::Map::new())
            .await
            .expect_err("non-JSON text should become ComfyError::Payload");
        assert!(matches!(err, crate::ComfyError::Payload { .. }));
    }

    #[tokio::test]
    async fn test_empty_content_becomes_payload_error() {
        let client = client_with(vec![Reply::Empty]).await;
        let err = client
            .call::<serde_json::Value>("any_tool", serde_json::Map::new())
            .await
            .expect_err("empty content should become ComfyError::Payload");
        assert!(matches!(err, crate::ComfyError::Payload { .. }));
    }

    #[tokio::test]
    async fn test_health_decodes_the_captured_server_info() {
        let client = client_with(vec![Reply::Json(json!({
            "server": { "running": true },
            "hardware": { "gpu": {} }
        }))])
        .await;
        let info: ServerInfo = client.health().await.expect("health decodes ServerInfo");
        assert!(info.server.is_some());
        assert!(info.workspace.is_none());
    }

    #[tokio::test]
    async fn test_slots_fixture_decodes_and_keeps_subgraph_addresses() {
        let fixture = include_str!("../../../testdata/mcp/list_workflow_slots.minimax.json");
        let value: serde_json::Value =
            serde_json::from_str(fixture).expect("fixture is valid JSON");
        let client = client_with(vec![Reply::Json(value)]).await;
        let result: serde_json::Value = client
            .call("list_workflow_slots", serde_json::Map::new())
            .await
            .expect("fixture decodes");
        let slots = result
            .get("slots")
            .and_then(|s| s.as_array())
            .expect("slots array");
        assert_eq!(slots.len(), 25);
        let with_slash = slots
            .iter()
            .filter(|s| {
                s.get("address")
                    .and_then(|a| a.as_str())
                    .map(|a| a.contains('/'))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(with_slash, 24);
    }
}
