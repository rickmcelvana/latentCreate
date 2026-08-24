# T-102c: stderr capture + free-text redaction — the child's output reaches the log
**Depends:** T-102b | **Crate/dir:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to modify:** `crates/mcp-bridge/src/local.rs`, `crates/mcp-bridge/src/session_log.rs`, `crates/mcp-bridge/src/mock.rs`, `crates/mcp-bridge/Cargo.toml`

> Second half of the T-102b split. T-102b landed the `SessionLog`, structural `redact`, and
> `call`-wiring; this task captures `comfy-mcp`'s stderr into that log, adds free-text redaction,
> and — folding in a gap noted at the T-102b review — a mock case that exercises the
> transport-fault branch of `call` that nothing could trigger before.

## Goal
CONVENTIONS requires `comfy-mcp`'s stderr captured to the session log; today `connect` uses
`TokioChildProcess::new`, which inherits stderr, so comfy-mcp's diagnostics vanish in a packaged
build. This task switches `connect` to the builder with `Stdio::piped()`, drains the captured
`ChildStderr` on a cancellable task owned by `LocalComfy` (aborted on `shutdown`), and adds
`redact_line` so free text (stderr lines, non-JSON error messages) is scrubbed the way `redact`
already scrubs JSON.

## Verified, not recalled
- The spawn API was read from rmcp 3.1.4's `child_process.rs`, not remembered:
  `TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()` returns
  `(TokioChildProcess, Option<ChildStderr>)`; `ChildStderr` is `tokio::process::ChildStderr`.
- **The transport-abort mock case was built and run, not assumed.** `Reply::Hangup` closes the
  duplex without answering; verified that this makes rmcp's `call_tool` return `Err` (a
  `ServiceError`, surfaced as `ComfyError::Transport` via the existing `From` impl) rather than
  hang, and that `call` logs it as `ok: false`. This is the branch T-102b left untested.
- The full reference code compiles, is `cargo fmt`- and `clippy -D warnings`-clean, and 26 tests
  pass (verified in a throwaway crate outside the repo).

## Reference code

### `crates/mcp-bridge/Cargo.toml`
`drain_stderr` runs in the main library, so the `io-util` feature must move onto the main `tokio`
dependency (the dev-dependency already has it):
```toml
tokio = { version = "1.53", features = ["process", "rt", "time", "io-util"] }
```

### `crates/mcp-bridge/src/mock.rs` — full file
Two changes: a `Hangup` reply that closes the connection, and `to_result` returns `Option` so
`None` means "close instead of answer".
```rust
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
    /// Close the connection without answering. The pending `tools/call` gets a
    /// transport error from the client, not a tool result -- the shape that
    /// exercises the transport-fault branch of `LocalComfy::call`.
    Hangup,
}

impl Reply {
    /// What to answer, or `None` to close the connection without answering.
    fn to_result(&self) -> Option<Value> {
        match self {
            Reply::Json(v) => Some(json!({
                "content": [{ "type": "text", "text": v.to_string() }],
                "isError": false
            })),
            Reply::RawText(s) => Some(json!({
                "content": [{ "type": "text", "text": s }],
                "isError": false
            })),
            Reply::ToolError(msg) => Some(json!({
                "content": [{ "type": "text", "text": msg }],
                "isError": true
            })),
            Reply::ToolErrorJson(v) => Some(json!({
                "content": [{ "type": "text", "text": v.to_string() }],
                "isError": true
            })),
            Reply::Empty => Some(json!({ "content": [], "isError": false })),
            Reply::Hangup => None,
        }
    }
}

/// Drive one canned MCP session over the peer half of a duplex pair.
///
/// Answers `initialize`, ignores notifications, and serves `replies` to
/// successive `tools/call` requests in order. Running out of replies is itself
/// reported as a tool error rather than a hang, so a test that calls more often
/// than it prepared for fails with a readable message. A [`Reply::Hangup`]
/// closes the connection without answering instead.
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
                    let reply =
                        next.next().unwrap_or(Reply::ToolError("mock ran out of replies".into()));
                    match reply.to_result() {
                        Some(result) => result,
                        None => break,
                    }
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
```

### `crates/mcp-bridge/src/session_log.rs` — three additions, one replacement

1. Update the `SENSITIVE_WORDS` doc comment (it now governs free text too):
```rust
/// Words that mark a value as a secret, whether they name a JSON key or head a
/// `name=value` / `name: value` run in free text. Matched case-insensitively on
/// whole words only, so `key` in `monkey` is not a hit.
```

2. Add `log_stderr` to the `SessionLog` impl, immediately after `log_result`:
```rust
    /// Record one line of `comfy-mcp`'s stderr, redacted.
    pub fn log_stderr(&self, line: &str) {
        let entry = json!({
            "ts": now_secs(),
            "kind": "stderr",
            "line": redact_line(line),
        });
        self.write_line(entry);
    }
```

3. Replace `redact_text_or_json` (its non-JSON fallback now scrubs instead of passing through):
```rust
/// Redact a JSON document if it parses, else treat it as free text.
fn redact_text_or_json(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => redact(&value).to_string(),
        Err(_) => redact_line(text),
    }
}
```

4. Add `redact_line` and `word_at` after `redact_text_or_json`, before `is_sensitive`:
```rust
/// Scrub secret-shaped assignments out of a free-text line.
///
/// Two shapes: `NAME=value` redacts the value token, and `NAME: ...` redacts
/// the rest of the line (header-style secrets like `Authorization: Bearer xyz`
/// are conventionally line-final). Nothing else in the line is touched.
fn redact_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        let matched = SENSITIVE_WORDS
            .iter()
            .find_map(|w| word_at(&lower, i, w).then_some(w.len()));

        if let Some(wlen) = matched {
            let start = i;
            let mut j = start + wlen;
            while j < line.len() && line.as_bytes()[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < line.len() && line.as_bytes()[j] == b'=' {
                out.push_str(&line[start..=j]);
                out.push_str(REDACTED);
                let mut k = j + 1;
                while k < line.len() {
                    let b = line.as_bytes()[k];
                    if b.is_ascii_whitespace() || matches!(b, b',' | b';') {
                        break;
                    }
                    k += 1;
                }
                i = k;
                continue;
            }
            if j < line.len() && line.as_bytes()[j] == b':' {
                out.push_str(&line[start..=j]);
                out.push(' ');
                out.push_str(REDACTED);
                i = line.len();
                continue;
            }
            out.push_str(&line[start..start + wlen]);
            i = start + wlen;
            continue;
        }

        let ch = line[i..]
            .chars()
            .next()
            .expect("byte offset on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Whether `w` appears in `lower` at `at` bounded by non-alphanumeric on both
/// sides, so `key` inside `monkey` is not a match.
fn word_at(lower: &str, at: usize, w: &str) -> bool {
    if !lower[at..].starts_with(w) {
        return false;
    }
    let before_ok = at == 0
        || !lower[..at]
            .chars()
            .next_back()
            .expect("boundary")
            .is_ascii_alphanumeric();
    let after = at + w.len();
    let after_ok = after >= lower.len()
        || !lower[after..]
            .chars()
            .next()
            .expect("boundary")
            .is_ascii_alphanumeric();
    before_ok && after_ok
}
```

### `crates/mcp-bridge/src/local.rs` — six edits

1. Add two imports. After `use std::time::Duration;` add `use std::process::Stdio;`; after the
   `use serde_json::…` line add `use tokio::io::AsyncBufReadExt;`:
```rust
use std::process::Stdio;
```
```rust
use tokio::io::AsyncBufReadExt;
```

2. Add the `stderr_task` field to the struct:
```rust
pub struct LocalComfy {
    service: RunningService<RoleClient, ()>,
    log: Option<SessionLog>,
    stderr_task: Option<tokio::task::AbortHandle>,
}
```

3. Replace `connect` — switch to the builder with `Stdio::piped()`, pass the captured handle on:
```rust
    /// Spawn `comfy-mcp` over stdio, capture its stderr into `log`, and
    /// complete the MCP handshake.
    ///
    /// `bin` is the configured executable name or path; the default is
    /// `"comfy-mcp"` on PATH.
    pub async fn connect(bin: &str, log: SessionLog) -> Result<Self, ComfyError> {
        let (transport, stderr) = TokioChildProcess::builder(
            tokio::process::Command::new(bin).configure(|c| {
                c.env("PYTHONIOENCODING", "utf-8");
            }),
        )
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ComfyError::NotInstalled,
            _ => ComfyError::Spawn(e),
        })?;
        Self::from_transport_with_log(transport, Some(log), stderr).await
    }
```

4. Update `from_transport` and `from_transport_with_log` to carry the `stderr` handle (the
   `from_transport` body becomes the three-argument delegation; `from_transport_with_log` gains
   the parameter and spawns the drain when both a handle and a log are present):
```rust
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
        let service = ()
            .serve(transport)
            .await
            .map_err(|e| ComfyError::Transport(e.to_string()))?;
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
```

5. Update `shutdown` to abort the drain task before cancelling the service:
```rust
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
```

6. Add `drain_stderr` and `spawn_stderr_drain`, after `with_timeout`:
```rust
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
```

7. In `test_helpers`, update `client_with_session_log` to pass the new third argument:
```rust
        LocalComfy::from_transport_with_log(client_half, Some(log), None)
            .await
            .expect("handshake over duplex")
```

## Tests
Three new tests in `session_log.rs`'s `tests` module, two in `local.rs`'s `transport_tests`.
All 10 existing `transport_tests` and 8 existing `session_log` tests are unchanged and must
keep passing.

- `test_redact_line_scrubs_name_equals_value` — **protects:** `NAME=value` redaction. The secret
  token after `=` is gone; the name stays visible so diagnostics still make sense.
- `test_redact_line_scrubs_header_style_to_end_of_line` — **protects:** `NAME: ...` redaction.
  Everything after the colon is dropped, so `Authorization: Bearer <token>` cannot leak the token.
- `test_redact_line_does_not_scrub_substring_words` — **protects:** the same word-boundary rule
  `is_sensitive` enforces for JSON, now for free text. "monkey ate a key lime pie" must survive
  untouched; a substring match would mangle ordinary stderr and break the log's fidelity.
- `test_log_stderr_records_a_redacted_line` — **protects:** the `log_stderr` path writes a
  `kind: "stderr"` entry whose secret is gone.
- `test_drain_stderr_writes_redacted_lines_to_the_log` — **protects:** `drain_stderr` reads the
  stream line-by-line and each line is scrubbed on the way to the log. Drives `drain_stderr`
  directly with a `duplex` (write lines, drop the write half for EOF) so no child process is
  spawned.
- `test_transport_error_is_logged_as_a_failed_result` — **protects:** the transport-fault branch
  T-102b left untested. Serve `Reply::Hangup`; the call must return `ComfyError::Transport` (not
  hang), and the log must hold a `call` entry and a `result` entry with `ok: false`. This is the
  test that fails if the `Err(e) => { log_result(false, …) }` branch in `call` is deleted.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All six named tests present and passing; the pre-existing tests still pass
- [ ] No test spawns a process, opens a socket, or reaches the network
- [ ] `drain_stderr`, `spawn_stderr_drain`, `redact_line`, and `word_at` are **not** `pub`
- [ ] No changes outside the four listed files (plus `Cargo.lock`, which the `io-util` feature
  touch should leave unchanged — it is already in the dev-dependency set)
- [ ] No new dependency crate — only the `io-util` feature on the existing `tokio`

## Out of scope
Rolling the log over more than one previous generation. A diagnostics-pane Tauri command that
reads the log, and any UI. Making the mock's `Hangup` configurable beyond "close without
answering". The documented `system_stats` `argv` residual (a secret split across an array) —
still out of reach of both `redact` and `redact_line`; mitigation stays upstream at T-110.

## Notes for the executor
- `drain_stderr` takes `impl AsyncRead + Unpin` precisely so the test can drive it with a
  `duplex` stream instead of a real `ChildStderr`; do not narrow it back to `ChildStderr`.
- `spawn_stderr_drain` must be spawned **after** the handshake in `from_transport_with_log` (it
  is), and `shutdown` must abort it before `service.cancel()` — a cancelled task holding the log
  is fine, but an aborted-then-cancelled order is what makes shutdown deterministic.
- Do not add a `Drop` impl to `LocalComfy`; the child is still killed by rmcp's transport on
  drop, and the drain ends on its own when the stderr pipe closes (docs/MCP-SURFACE §8.2).
- The mock's `to_result` returning `Option` is what makes `Hangup` a clean `None => break`;
  do not add a `Reply::Hangup` arm that returns a result — closing the connection is the point.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
`error.rs` and `types.rs` are `--read`: the transport-error test asserts `ComfyError::Transport`
and `call`/`health`/`stats` reference `ServerInfo`/`SystemStats`.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/error.rs --read crates/mcp-bridge/src/types.rs --file crates/mcp-bridge/Cargo.toml --file crates/mcp-bridge/src/mock.rs --file crates/mcp-bridge/src/session_log.rs --file crates/mcp-bridge/src/local.rs
```
