//! A fake MCP peer over an in-memory pipe, for offline tests.
//!
//! Test-only: no child process, no socket, no running ComfyUI. See
//! docs/MCP-SURFACE.md 8.3 for the result shapes this imitates.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

/// The `params` of every `tools/call` the fake peer received, in order.
///
/// Lets a test assert what the bridge **sent**, not just how it read the reply.
/// comfy-mcp rejects a wrong argument name outright (`path` where it wants
/// `workflow_path` -- docs/MCP-SURFACE.md 8.7), so a wrapper that misnames one
/// passes every response-only test and then fails against a real server.
pub type RecordedCalls = Arc<Mutex<Vec<Value>>>;

/// What the fake peer answers the next `tools/call` with.
#[derive(Clone, Debug)]
pub enum Reply {
    /// A success whose text block carries this JSON document.
    Json(Value),
    /// A success whose text block is not JSON at all.
    RawText(String),
    /// A tool-level failure: `is_error: true` carrying this message.
    ToolError(String),
    /// A tool-level failure whose text happens to be valid JSON. The only
    /// shape that catches a bridge ignoring `is_error`, because every other
    /// failure text also fails to decode.
    ToolErrorJson(Value),
    /// A success with no content blocks at all.
    Empty,
}

impl Reply {
    fn to_result(&self) -> Value {
        match self {
            Reply::Json(v) => json!({
                "content": [{ "type": "text", "text": v.to_string() }],
                "isError": false
            }),
            Reply::RawText(s) => json!({
                "content": [{ "type": "text", "text": s }],
                "isError": false
            }),
            Reply::ToolError(msg) => json!({
                "content": [{ "type": "text", "text": msg }],
                "isError": true
            }),
            Reply::ToolErrorJson(v) => json!({
                "content": [{ "type": "text", "text": v.to_string() }],
                "isError": true
            }),
            Reply::Empty => json!({ "content": [], "isError": false }),
        }
    }
}

/// Drive one canned MCP session over the peer half of a duplex pair.
///
/// Answers `initialize`, ignores notifications, and serves `replies` to
/// successive `tools/call` requests in order. Running out of replies is itself
/// reported as a tool error rather than a hang, so a test that calls more often
/// than it prepared for fails with a readable message.
///
/// Returns the [`RecordedCalls`] log so a test can assert on what was sent.
pub fn spawn_mock(peer: DuplexStream, replies: Vec<Reply>) -> RecordedCalls {
    let recorded: RecordedCalls = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&recorded);
    tokio::spawn(async move {
        let (r, mut w) = tokio::io::split(peer);
        let mut lines = BufReader::new(r).lines();
        let mut next = replies.into_iter();

        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(id) = msg.get("id").cloned() else {
                continue; // a notification: nothing to answer
            };
            let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

            let result = match method {
                "initialize" => json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "mock-comfy-mcp", "version": "0" }
                }),
                "tools/call" => {
                    if let Ok(mut log) = sink.lock() {
                        log.push(msg.get("params").cloned().unwrap_or(Value::Null));
                    }
                    next.next()
                        .unwrap_or(Reply::ToolError("mock ran out of replies".into()))
                        .to_result()
                }
                _ => json!({}),
            };

            let out = json!({ "jsonrpc": "2.0", "id": id, "result": result });
            if w.write_all(format!("{out}\n").as_bytes()).await.is_err() {
                break;
            }
            let _ = w.flush().await;
        }
    });
    recorded
}
