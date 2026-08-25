//! Local backend: `comfy-mcp` spawned as a stdio child process.

use std::process::Stdio;
use std::time::Duration;

use rmcp::{
    model::{CallToolRequestParams, CallToolResult},
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, IntoTransport, TokioChildProcess},
    ServiceExt,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::io::AsyncBufReadExt;

use crate::error::{parse_error_code, ComfyError};
use crate::health::ServerInfo;
use crate::session_log::SessionLog;
use crate::types::SystemStats;

/// A live `comfy-mcp` session.
///
/// The child process is killed when this is dropped -- rmcp's
/// `TokioChildProcess` owns that cleanup, so do not add a `Drop` impl here
/// (docs/MCP-SURFACE.md 8.2).
pub struct LocalComfy {
    service: RunningService<RoleClient, ()>,
    log: Option<SessionLog>,
    stderr_task: Option<tokio::task::AbortHandle>,
}

impl LocalComfy {
    /// Spawn `comfy-mcp` over stdio, capture its stderr into `log`, and
    /// complete the MCP handshake.
    ///
    /// `bin` is the configured executable name or path; the default is
    /// `"comfy-mcp"` on PATH.
    pub async fn connect(bin: &str, log: SessionLog) -> Result<Self, ComfyError> {
        let (transport, stderr) =
            TokioChildProcess::builder(tokio::process::Command::new(bin).configure(|c| {
                c.env("PYTHONIOENCODING", "utf-8");
            }))
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ComfyError::NotInstalled,
                _ => ComfyError::Spawn(e),
            })?;
        Self::from_transport_with_log(transport, Some(log), stderr).await
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
        Self::from_transport_with_log(transport, None, None).await
    }

    /// Handshake, plus the session log and an optional captured stderr handle
    /// to drain into it. [`LocalComfy::connect`] and [`LocalComfy::from_transport`]
    /// both funnel through here.
    pub async fn from_transport_with_log<T, E, A>(
        transport: T,
        log: Option<SessionLog>,
        stderr: Option<tokio::process::ChildStderr>,
    ) -> Result<Self, ComfyError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let service = ().serve(transport).await.map_err(|e| ComfyError::Transport(e.to_string()))?;
        let stderr_task = match (stderr, log.as_ref()) {
            (Some(reader), Some(log)) => Some(spawn_stderr_drain(reader, log.clone())),
            _ => None,
        };
        Ok(Self {
            service,
            log,
            stderr_task,
        })
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
        if let Some(log) = &self.log {
            log.log_call(tool, &arguments);
        }

        let res: CallToolResult = match self
            .service
            .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
            .await
        {
            Ok(res) => res,
            Err(e) => {
                if let Some(log) = &self.log {
                    log.log_result(tool, false, &e.to_string());
                }
                return Err(e.into());
            }
        };

        let text = res
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or_default();

        if let Some(log) = &self.log {
            log.log_result(tool, !res.is_error.unwrap_or(false), text);
        }

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
        if let Some(task) = self.stderr_task {
            task.abort();
        }
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

/// Drain a child's stderr into the session log, one redacted line at a time.
/// Ends when the pipe closes (the child exits or is killed).
async fn drain_stderr(reader: impl tokio::io::AsyncRead + Unpin, log: SessionLog) {
    let mut lines = tokio::io::BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        log.log_stderr(&line);
    }
}

/// Spawn the stderr drain onto the current runtime, returning the handle the
/// caller keeps so [`LocalComfy::shutdown`] can cancel it.
fn spawn_stderr_drain(
    reader: tokio::process::ChildStderr,
    log: SessionLog,
) -> tokio::task::AbortHandle {
    tokio::spawn(drain_stderr(reader, log)).abort_handle()
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use tokio::io::duplex;

    use crate::mock::{spawn_mock, RecordedCalls, Reply};
    use crate::session_log::SessionLog;
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

    /// A client wired to a real session log, for the logging-path tests.
    pub async fn client_with_session_log(replies: Vec<Reply>, log: SessionLog) -> LocalComfy {
        let (client_half, peer_half) = duplex(8 * 1024);
        spawn_mock(peer_half, replies);
        LocalComfy::from_transport_with_log(client_half, Some(log), None)
            .await
            .expect("handshake over duplex")
    }
}

#[cfg(test)]
mod transport_tests {
    use serde_json::json;

    use crate::health::ServerInfo;
    use crate::local::test_helpers::{client_and_log, client_with, client_with_session_log};
    use crate::mock::Reply;
    use crate::session_log::SessionLog;

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
        assert!(info.is_running());
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

    #[tokio::test]
    async fn test_call_logs_call_and_result_to_the_session_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.log");
        let log = SessionLog::open(&path).expect("open log");
        let client = client_with_session_log(vec![Reply::Json(json!({ "answer": 42 }))], log).await;

        let mut args = serde_json::Map::new();
        args.insert("workflow_path".into(), json!("wf.json"));
        args.insert("api_key".into(), json!("sk-secret"));
        let _: serde_json::Value = client
            .call("list_workflow_slots", args)
            .await
            .expect("call succeeds");

        let raw = std::fs::read_to_string(&path).expect("read log");
        assert!(!raw.contains("sk-secret"));
        let entries: Vec<serde_json::Value> = raw
            .lines()
            .map(|l| serde_json::from_str(l).expect("line is JSON"))
            .collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["kind"], json!("call"));
        assert_eq!(entries[0]["tool"], json!("list_workflow_slots"));
        assert_eq!(entries[0]["arguments"]["api_key"], json!("[REDACTED]"));
        assert_eq!(entries[1]["kind"], json!("result"));
        assert_eq!(entries[1]["tool"], json!("list_workflow_slots"));
        assert_eq!(entries[1]["ok"], json!(true));
    }

    #[tokio::test]
    async fn test_drain_stderr_writes_redacted_lines_to_the_log() {
        use tokio::io::AsyncWriteExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.log");
        let log = SessionLog::open(&path).expect("open log");

        let (mut writer, reader) = tokio::io::duplex(256);
        writer
            .write_all(b"api_key=secret\nmonkey\n")
            .await
            .expect("write");
        drop(writer);

        crate::local::drain_stderr(reader, log).await;

        let raw = std::fs::read_to_string(&path).expect("read log");
        let entries: Vec<serde_json::Value> = raw
            .lines()
            .map(|l| serde_json::from_str(l).expect("line is JSON"))
            .collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["kind"], json!("stderr"));
        assert_eq!(entries[0]["line"], json!("api_key=[REDACTED]"));
        assert_eq!(entries[1]["line"], json!("monkey"));
    }

    #[tokio::test]
    async fn test_transport_error_is_logged_as_a_failed_result() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.log");
        let log = SessionLog::open(&path).expect("open log");
        let client = client_with_session_log(vec![Reply::Hangup], log).await;

        let err = client
            .call::<serde_json::Value>("any_tool", serde_json::Map::new())
            .await
            .expect_err("hangup should become a transport error");

        assert!(matches!(err, crate::ComfyError::Transport { .. }));

        let raw = std::fs::read_to_string(&path).expect("read log");
        let entries: Vec<serde_json::Value> = raw
            .lines()
            .map(|l| serde_json::from_str(l).expect("line is JSON"))
            .collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["kind"], json!("call"));
        assert_eq!(entries[0]["tool"], json!("any_tool"));
        assert_eq!(entries[1]["kind"], json!("result"));
        assert_eq!(entries[1]["ok"], json!(false));
    }
}
