# T-102: mock transport test rig
**Depends:** T-101 | **Dirs:** `crates/mcp-bridge/`, `testdata/mcp/` | **Executor:** Aider

**Files to create:** `crates/mcp-bridge/src/mock.rs`

**Files to modify:** `crates/mcp-bridge/Cargo.toml`, `crates/mcp-bridge/src/lib.rs`, `crates/mcp-bridge/src/local.rs`

*(`testdata/mcp/list_workflow_slots.minimax.json` already exists — captured live, do not edit it.)*

## Goal
Every later `mcp-bridge` task gets offline tests. A fake MCP peer speaks the real protocol
over an in-memory pipe, so no test spawns `comfy-mcp`, opens a socket, or needs a running
ComfyUI (WORKFLOW §5).

**T-101 left its most important branch untested** — `call()` turning `Ok(is_error: true)`
into `ComfyError::Tool`, the finding the whole rmcp verification turned on
(docs/MCP-SURFACE.md §8.3). Covering it needs a transport, which is this task. That branch
is the reason T-102 exists; the rig is the means.

## Verified mechanism — do not substitute another approach
Built and run in a throwaway crate on 2026-08-23 before this brief was written. All eight
tests pass; the two `is_error` tests were confirmed to **fail** when the guard in `call()`
is deleted, so they are not vacuous.

- `tokio::io::duplex(8 * 1024)` returns two joined halves. rmcp implements `IntoTransport`
  for **any** `AsyncRead + AsyncWrite + Send + 'static`, so a duplex half *is* a valid
  client transport. No child process, no socket, no `comfy-mcp`.
- **No new rmcp feature is needed.** `transport-child-process` already enables
  `transport-async-rw`, which carries that impl.
- The wire framing is **newline-delimited JSON-RPC**, so the fake peer is a plain
  `BufReader::lines()` loop writing `{"jsonrpc":"2.0","id":<id>,"result":{...}}\n`.
- The handshake needs exactly one canned reply — `initialize` — plus ignoring the
  `notifications/initialized` that follows (a message with no `id`).

## Dependencies
`crates/mcp-bridge/Cargo.toml` — add a dev-dependencies section. Nothing in `[dependencies]`
changes.
```toml
[dev-dependencies]
tokio = { version = "1.53", features = ["macros", "io-util"] }
```
`macros` for `#[tokio::test]`, `io-util` for `duplex`. `rt` already comes from the main
dependency. Verified to be the minimal set.

## Reference code

### `crates/mcp-bridge/src/local.rs` — add one method
Insert after `connect`, leaving `connect` and everything else untouched. Add
`IntoTransport` to the existing `rmcp::transport` import.
```rust
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
        let service = ()
            .serve(transport)
            .await
            .map_err(|e| ComfyError::Transport(e.to_string()))?;
        Ok(Self { service })
    }
```
Then rewrite `connect`'s body to end with `Self::from_transport(transport).await` instead of
duplicating the `serve` call — one handshake path, not two.

### `crates/mcp-bridge/src/mock.rs`
```rust
//! A fake MCP peer over an in-memory pipe, for offline tests.
//!
//! Test-only: no child process, no socket, no running ComfyUI. See
//! docs/MCP-SURFACE.md 8.3 for the result shapes this imitates.

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

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
pub fn spawn_mock(peer: DuplexStream, replies: Vec<Reply>) {
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
                "tools/call" => next
                    .next()
                    .unwrap_or(Reply::ToolError("mock ran out of replies".into()))
                    .to_result(),
                _ => json!({}),
            };

            let out = json!({ "jsonrpc": "2.0", "id": id, "result": result });
            if w.write_all(format!("{out}\n").as_bytes()).await.is_err() {
                break;
            }
            let _ = w.flush().await;
        }
    });
}
```

### `crates/mcp-bridge/src/lib.rs`
Add alongside the existing module declarations. **Test-only — it must not ship in a release
build**, and `Reply`/`spawn_mock` are not part of the crate's public API:
```rust
#[cfg(test)]
mod mock;
```

## Tests
New `#[cfg(test)] mod transport_tests` in `crates/mcp-bridge/src/local.rs`, with this helper:
```rust
    async fn client_with(replies: Vec<Reply>) -> LocalComfy {
        let (client_half, peer_half) = tokio::io::duplex(8 * 1024);
        crate::mock::spawn_mock(peer_half, replies);
        LocalComfy::from_transport(client_half)
            .await
            .expect("handshake over duplex")
    }
```

- `test_handshake_completes_over_a_duplex_transport` — **protects:** the rig itself. If the
  canned `initialize` reply is wrong, every other test in this module fails for a reason
  that has nothing to do with what it was testing. Assert `client_with(vec![])` returns.
- `test_ok_payload_decodes_from_the_text_block` — **protects:** the two-stage decode.
  comfy-mcp puts a JSON *document inside a text block* and never sets `structured_content`,
  so a bridge reading `structured_content` gets `None` forever. Serve `Reply::Json`, assert
  a field comes back.
- `test_is_error_becomes_tool_error` — **protects:** the finding this task exists for. Serve
  `Reply::ToolError` carrying a real message
  (`"comfy workflow slots x failed [workflow_not_found]: nope"`), assert
  `ComfyError::Tool` with `code == Some("workflow_not_found")`.
- `test_is_error_wins_over_a_decodable_payload` — **protects:** the same branch against the
  test above passing for the wrong reason. With `Reply::ToolErrorJson`, a bridge that
  ignores `is_error` returns `Ok` instead of failing at the decode step, so this is the only
  test that catches deleting the guard outright. Assert `ComfyError::Tool`.
- `test_non_json_text_becomes_payload_error` — `Reply::RawText` → `ComfyError::Payload`.
- `test_empty_content_becomes_payload_error` — **protects:** a result with no content blocks
  must not be read as success. `Reply::Empty` → `ComfyError::Payload`.
- `test_health_decodes_the_captured_server_info` — **protects:** `ServerInfo`'s optional
  fields. Serve `Reply::Json(json!({"server": {"running": true}, "hardware": {"gpu": {}}}))`
  and assert `server` is `Some` while the absent `workspace` is `None`, not an error.
- `test_slots_fixture_decodes_and_keeps_subgraph_addresses` — **protects:** the shape T-103
  is built on, against a **live-captured** payload. Load
  `testdata/mcp/list_workflow_slots.minimax.json` with
  `include_str!("../../../testdata/mcp/list_workflow_slots.minimax.json")`, serve it as
  `Reply::Json`, and assert: 25 slots come back, and **24 of the 25 addresses contain `/`**.
  That ratio is the real one measured on this workflow — a fixture edited down to flat
  addresses would silently retire the case T-103 must handle.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All eight named tests present and passing
- [ ] **No test spawns a process, opens a socket, or reads an env var**
- [ ] `mod mock` is `#[cfg(test)]`; `Reply`/`spawn_mock` are not exported from the crate
- [ ] `connect` and `from_transport` share one handshake path
- [ ] `testdata/mcp/list_workflow_slots.minimax.json` is unmodified
- [ ] No dependency beyond the one dev-dependency listed

## Out of scope
Wrappers for any other tool — `list_workflow_slots`, templates, jobs, models are T-103+.
A `Slot` type or address parsing (T-103 owns both; this task only asserts the fixture's
addresses as raw strings). Session logging and stderr capture (T-102b). Anything that makes
the mock configurable at runtime, or a builder API for it — it serves canned replies in
order, and that is the whole design.

## Notes for the executor
- Do not add a `server` feature to rmcp. The fake peer is hand-written JSON-RPC on purpose;
  pulling in rmcp's server half to test its client half would test the two against each
  other rather than against the protocol.
- Do not make `mock.rs` public or move it to `tests/`. In-crate `#[cfg(test)]` keeps it
  usable from unit tests in sibling modules, which is what T-103+ need.
- The fixture's `workflow` field was normalised to a relative path when captured; every
  other value in it is verbatim from the live server. Do not assert on `workflow`.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read testdata/mcp/list_workflow_slots.minimax.json --file crates/mcp-bridge/Cargo.toml --file crates/mcp-bridge/src/lib.rs --file crates/mcp-bridge/src/local.rs --file crates/mcp-bridge/src/mock.rs
```
