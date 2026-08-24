# T-102b: session log + redaction — tool traffic recorded, secrets never on disk
**Depends:** T-102 | **Crate/dir:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to create:** `crates/mcp-bridge/src/session_log.rs`

**Files to modify:** `crates/mcp-bridge/src/local.rs`, `crates/mcp-bridge/src/lib.rs`, `crates/mcp-bridge/Cargo.toml`

> T-102b is the first half of the T-102b task in the phase file, which was **split**: this
> brief delivers the *session log and redaction* and wires tool-call logging into `call`.
> **T-102c** (immediately after) delivers *stderr capture* — the `Stdio::piped()` switch and
> the drain task — plus free-text redaction (`redact_line`). Split for the ~400-line rule,
> exactly as T-103 was.

## Goal
ARCHITECTURE §3 requires every tool-call payload and result logged (redacted) to a rotating
session log for the diagnostics pane; CONVENTIONS forbids keys ever reaching a log. This task
builds the log: an append-only **NDJSON** file, one JSON object per line, that records each
`call` (redacted arguments) and its outcome (redacted result). `LocalComfy::call` is the single
choke point every wrapper funnels through, so it is where the logging goes.

## Verified, not recalled
The reference code below compiles against rmcp 3.1.4, is `cargo fmt`- and `clippy -D warnings`-
clean, and all 20 tests pass (verified in a throwaway crate outside the repo, the Phase-0
method). No third-party surface is recalled: the `call` restructure preserves the two traps
this crate already guards — a failing tool returns `Ok(is_error: true)` (docs/MCP-SURFACE §8.3)
and results are JSON-in-text (§8.4) — and the log only observes, it never changes those paths.

The `TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()` API (the *other* half of
the original task) was verified against the rmcp source this session and is deferred to T-102c,
so that brief can be written without re-verification.

## Reference code

### `crates/mcp-bridge/src/session_log.rs` — full file
```rust
//! Rotating, redacting session log (ARCHITECTURE.md §3).
//!
//! One append-only NDJSON file: every tool call and result, one JSON object
//! per line for the diagnostics pane. Secrets are redacted before anything
//! touches disk.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

/// What a redacted value is replaced with.
const REDACTED: &str = "[REDACTED]";

/// Default size at which the log rolls over to a `.1` sibling.
pub const DEFAULT_MAX_BYTES: u64 = 1 << 20; // 1 MiB

/// Words that mark a value as a secret when they name a JSON key. Matched
/// case-insensitively on whole words only, so `key` in `monkey` is not a hit.
const SENSITIVE_WORDS: &[&str] = &[
    "apikey",
    "key",
    "token",
    "secret",
    "password",
    "passwd",
    "authorization",
    "auth",
    "credential",
    "bearer",
];

/// A rotating session log.
///
/// `Clone` and cheap to pass around: every handle shares one file. Append
/// operations are serialised through an internal lock, so `log_call` /
/// `log_result` may be called from concurrent tasks.
#[derive(Clone)]
pub struct SessionLog {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    path: PathBuf,
    max_bytes: u64,
    file: Option<File>,
    bytes_written: u64,
}

impl SessionLog {
    /// Open (creating if needed) the log at `path` with the default size limit.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Self::with_max_bytes(path, DEFAULT_MAX_BYTES)
    }

    /// Open the log at `path`, rolling over once it exceeds `max_bytes`.
    ///
    /// `max_bytes` is exposed so tests can force a rollover without writing a
    /// megabyte; callers use [`SessionLog::open`].
    pub fn with_max_bytes(path: impl AsRef<Path>, max_bytes: u64) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = open_append(&path)?;
        let bytes_written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                path,
                max_bytes,
                file: Some(file),
                bytes_written,
            })),
        })
    }

    /// Record a tool invocation with its (redacted) arguments.
    pub fn log_call(&self, tool: &str, arguments: &Map<String, Value>) {
        let entry = json!({
            "ts": now_secs(),
            "kind": "call",
            "tool": tool,
            "arguments": redact(&Value::Object(arguments.clone())),
        });
        self.write_line(entry);
    }

    /// Record a tool outcome. `text` is the raw payload comfy-mcp returned --
    /// JSON on success, an error message otherwise -- redacted either way.
    pub fn log_result(&self, tool: &str, ok: bool, text: &str) {
        let entry = json!({
            "ts": now_secs(),
            "kind": "result",
            "tool": tool,
            "ok": ok,
            "text": redact_text_or_json(text),
        });
        self.write_line(entry);
    }

    fn write_line(&self, entry: Value) {
        let mut line = serde_json::to_string(&entry).unwrap_or_default();
        line.push('\n');

        let mut inner = self.inner.lock().expect("session log lock poisoned");
        if inner.file.is_none() {
            inner.file = open_append(&inner.path).ok();
        }

        let overflow = inner.max_bytes > 0
            && inner.bytes_written > 0
            && inner.bytes_written + line.len() as u64 > inner.max_bytes;
        if overflow {
            let _ = inner.file.take();
            let rotated = rotated_path(&inner.path);
            // `rename` does not overwrite an existing destination on Windows, so
            // drop the previous generation first. One previous log is kept.
            let _ = fs::remove_file(&rotated);
            let _ = fs::rename(&inner.path, &rotated);
            inner.file = open_append(&inner.path).ok();
            inner.bytes_written = 0;
        }

        if let Some(file) = inner.file.as_mut() {
            if file.write_all(line.as_bytes()).is_ok() {
                inner.bytes_written += line.len() as u64;
            }
        }
    }
}

/// Replace values under secret-named keys with `[REDACTED]`, recursively.
///
/// Keys are matched case-insensitively on whole words (`api_key`, `API-KEY`,
/// `apikey`, `access_token`, ...). Values under other keys -- prompts, lyrics,
/// slot values -- pass through untouched. Over-redacting a log is acceptable;
/// under-redacting is not.
fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, val) in map {
                if is_sensitive(key) {
                    out.insert(key.clone(), Value::String(REDACTED.to_string()));
                } else {
                    out.insert(key.clone(), redact(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        other => other.clone(),
    }
}

/// Redact a JSON document if it parses, else pass the text through as-is.
///
/// Free-text redaction (for stderr and non-JSON error messages) arrives with
/// the stderr-capture task T-102c; until then a non-JSON result is logged raw.
fn redact_text_or_json(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => redact(&value).to_string(),
        Err(_) => text.to_string(),
    }
}

/// Whether `key` names a secret: equal to a sensitive word once non-alphanumeric
/// separators are removed, or containing a sensitive word as a whole word.
fn is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    let normalized: String = lower
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if SENSITIVE_WORDS.contains(&normalized.as_str()) {
        return true;
    }
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| SENSITIVE_WORDS.contains(&word))
}

/// Open the log file for append, creating its parent directory and the file.
fn open_append(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// The rollover sibling: `session.log` -> `session.log.1`.
fn rotated_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".1");
    PathBuf::from(os)
}

/// Epoch seconds, for the entry timestamp. Zero on a clock before 1970, which
/// for a diagnostics log is fine.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use serde_json::{json, Value};

    use super::*;

    fn read_log(path: &Path) -> Vec<Value> {
        let mut text = String::new();
        fs::File::open(path)
            .expect("log exists")
            .read_to_string(&mut text)
            .expect("readable");
        text.lines()
            .map(|l| serde_json::from_str(l).expect("each line is JSON"))
            .collect()
    }

    #[test]
    fn test_redact_scrubs_sensitive_keys_recursively() {
        let input = json!({
            "api_key": "sk-12345",
            "prompt": "a red fox",
            "nested": { "Authorization": "Bearer xyz", "steps": 8 },
            "list": [{ "refresh_token": "t-1" }, { "bpm": 120 }]
        });
        let out = redact(&input);
        assert_eq!(out["api_key"], json!("[REDACTED]"));
        assert_eq!(out["prompt"], json!("a red fox"));
        assert_eq!(out["nested"]["Authorization"], json!("[REDACTED]"));
        assert_eq!(out["nested"]["steps"], json!(8));
        assert_eq!(out["list"][0]["refresh_token"], json!("[REDACTED]"));
        assert_eq!(out["list"][1]["bpm"], json!(120));
    }

    #[test]
    fn test_redact_leaves_user_content_alone() {
        let input = json!({
            "tags": "keyboard, monkey, token of affection",
            "lyrics": "[Verse] I found the key to your heart",
            "keyscale": "E minor"
        });
        let out = redact(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn test_is_sensitive_matches_word_forms_not_substrings() {
        assert!(is_sensitive("api_key"));
        assert!(is_sensitive("API-KEY"));
        assert!(is_sensitive("apikey"));
        assert!(is_sensitive("access_token"));
        assert!(!is_sensitive("monkey"));
        assert!(!is_sensitive("keyscale"));
        assert!(!is_sensitive("monkey_keyboard"));
    }

    #[test]
    fn test_log_records_redacted_call_and_result() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.log");
        let log = SessionLog::open(&path).expect("open");

        let mut args = Map::new();
        args.insert("workflow_path".into(), json!("wf.json"));
        args.insert("api_key".into(), json!("sk-secret"));
        log.log_call("set_workflow_slot", &args);
        log.log_result(
            "set_workflow_slot",
            true,
            r#"{"applied":["a"],"wrote":"wf.json"}"#,
        );

        let entries = read_log(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["kind"], json!("call"));
        assert_eq!(entries[0]["tool"], json!("set_workflow_slot"));
        assert_eq!(entries[0]["arguments"]["api_key"], json!("[REDACTED]"));

        let raw = fs::read_to_string(&path).expect("read");
        assert!(!raw.contains("sk-secret"));
    }

    #[test]
    fn test_log_rotates_when_it_exceeds_max_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.log");
        let log = SessionLog::with_max_bytes(&path, 64).expect("open");

        for n in 0..20 {
            log.log_result("tool", true, &format!("line {n} padding padding padding"));
        }

        assert!(rotated_path(&path).exists(), "rolled-over sibling exists");
        let current = read_log(&path);
        assert!(
            current.len() < 20,
            "current file starts over after rotation"
        );
    }
}
```

### `crates/mcp-bridge/src/lib.rs`
Add the module declaration (alphabetical, between `preflight` and `slots`) and the re-export:
```rust
mod session_log;
```
```rust
pub use session_log::SessionLog;
```

### `crates/mcp-bridge/Cargo.toml`
Add one dev-dependency. `[dependencies]` is unchanged. `tempfile` is MIT OR Apache-2.0
(permissive), already a dev-dependency of `library`:
```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1.53", features = ["macros", "io-util"] }
```

### `crates/mcp-bridge/src/local.rs` — five edits

1. Add the import, after the `use crate::error::…` line:
```rust
use crate::session_log::SessionLog;
```

2. Add the `log` field to the struct:
```rust
pub struct LocalComfy {
    service: RunningService<RoleClient, ()>,
    log: Option<SessionLog>,
}
```

3. Replace `connect` (it now takes the log and funnels through the new seam):
```rust
    /// Spawn `comfy-mcp` over stdio and complete the MCP handshake.
    ///
    /// `bin` is the configured executable name or path; the default is
    /// `"comfy-mcp"` on PATH. Tool traffic is recorded to `log`.
    pub async fn connect(bin: &str, log: SessionLog) -> Result<Self, ComfyError> {
        let transport = TokioChildProcess::new(tokio::process::Command::new(bin).configure(|c| {
            c.env("PYTHONIOENCODING", "utf-8");
        }))
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ComfyError::NotInstalled,
            _ => ComfyError::Spawn(e),
        })?;
        Self::from_transport_with_log(transport, Some(log)).await
    }
```

4. Make `from_transport` delegate, and add `from_transport_with_log` right after it:
```rust
    pub async fn from_transport<T, E, A>(transport: T) -> Result<Self, ComfyError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::from_transport_with_log(transport, None).await
    }

    /// Handshake plus an optional session log. [`LocalComfy::connect`] and
    /// [`LocalComfy::from_transport`] both funnel through here.
    pub async fn from_transport_with_log<T, E, A>(
        transport: T,
        log: Option<SessionLog>,
    ) -> Result<Self, ComfyError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let service = ()
            .serve(transport)
            .await
            .map_err(|e| ComfyError::Transport(e.to_string()))?;
        Ok(Self { service, log })
    }
```

5. Replace `call` with the logging version. **Note the order: log the call before
   `arguments` is moved into `call_tool`, and log the transport-error outcome too** —
   not just the success and `is_error` paths:
```rust
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
```

6. Add a test helper to `test_helpers`, after `client_and_log`:
```rust
    /// A client wired to a real session log, for the logging-path tests.
    pub async fn client_with_session_log(replies: Vec<Reply>, log: SessionLog) -> LocalComfy {
        let (client_half, peer_half) = duplex(8 * 1024);
        spawn_mock(peer_half, replies);
        LocalComfy::from_transport_with_log(client_half, Some(log))
            .await
            .expect("handshake over duplex")
    }
```
   This needs two imports already present in `test_helpers` plus one new: add
   `use crate::session_log::SessionLog;` to the `test_helpers` `use` block.

## Tests
Five new tests in `session_log.rs`'s `tests` module and one in `local.rs`'s `transport_tests`.
The nine existing `transport_tests` and five `error` tests are unchanged and must keep passing.

- `test_redact_scrubs_sensitive_keys_recursively` — **protects:** a secret under a
  sensitive-named key never reaches the log, and redaction is *recursive* and *structural*
  (nested objects and arrays are walked; only key names trigger it, values never do).
- `test_redact_leaves_user_content_alone` — **protects:** user content (prompts, lyrics, slot
  values) passes through untouched. `redact` must not mangle values, and a value containing
  `key` as data must not be touched. This is the guard against over-redaction.
- `test_is_sensitive_matches_word_forms_not_substrings` — **protects:** the key-name match is
  word-based, not substring. `api_key`/`API-KEY`/`apikey`/`access_token` are hits; `monkey`,
  `keyscale`, `monkey_keyboard` are not. A substring match would redact `keyscale` (a real
  ACE-Step control name) and silently corrupt the log's fidelity.
- `test_log_records_redacted_call_and_result` — **protects:** end-to-end. The file holds NDJSON
  with a `call` entry and a `result` entry, and the secret string is absent from the raw bytes.
- `test_log_rotates_when_it_exceeds_max_bytes` — **protects:** rotation. Once the file passes
  `max_bytes` it rolls to `<path>.1` and the current file starts over. (Guards the Windows
  `rename`-doesn't-overwrite case: the previous `.1` is removed first.)
- `test_call_logs_call_and_result_to_the_session_log` (in `local.rs`) — **protects:** the
  wiring. `call()` really routes through the `SessionLog` — both a `call` and a `result` entry
  appear, the tool name is recorded, and a secret argument never reaches disk. This is the test
  that fails if `call` merely holds a `log` field it never uses.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All six named tests present and passing; the pre-existing tests still pass
- [ ] No test spawns a process, opens a socket, or reaches the network
- [ ] `SessionLog` is `pub` and re-exported from the crate; `redact`, `is_sensitive`,
  `redact_text_or_json`, `open_append`, `rotated_path`, `now_secs`, `write_line`, and
  `Inner` are **not** `pub`
- [ ] `mod session_log` is a normal (non-`cfg(test)`) module — it ships in release builds
- [ ] No changes outside the four listed files
- [ ] No dependency beyond the one `tempfile` dev-dependency

## Out of scope
Stderr capture — `TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()`, the drain
task, and the abort-on-shutdown — is **T-102c**. Until then `connect` still uses
`TokioChildProcess::new` and inherits stderr, and `redact_text_or_json` passes non-JSON text
through raw (no free-text redaction; a non-JSON *error message* is logged verbatim until
T-102c). Any Tauri command or UI reading the log (the diagnostics pane) is later still.
The `system_stats` `argv` residual — a secret embedded in free text or split across an array —
is a documented limitation of structural redaction and is not addressed here.

## Notes for the executor
- `session_log.rs` uses **only** `std` and `serde_json`. Do not add a `tokio` dependency to it;
  `Mutex` is `std::sync::Mutex`, file I/O is `std::fs`.
- Keep `redact`, `is_sensitive`, `redact_text_or_json`, `write_line`, `open_append`,
  `rotated_path`, `now_secs` private to the module. Only `SessionLog` (and the
  `DEFAULT_MAX_BYTES` const) are `pub`, and only `SessionLog` is re-exported.
- Do not switch `connect` to the spawn builder in this task. The `Stdio::piped()` stderr capture
  is T-102c; this task leaves the child's stderr inherited, exactly as T-101 left it.
- In `call`, log the call **before** `arguments` is moved into `call_tool` (`log_call` borrows
  it), and log the transport-error branch (`call_tool` returning `Err`) as `ok: false` too.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
`error.rs`, `types.rs` and `mock.rs` are `--read`: `call` constructs `ComfyError` variants and
`e.into()` uses the `From<ServiceError>` impl; `health`/`stats` return `ServerInfo`/`SystemStats`;
the new test uses `Reply`/`spawn_mock`/`RecordedCalls`.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/error.rs --read crates/mcp-bridge/src/mock.rs --read crates/mcp-bridge/src/types.rs --file crates/mcp-bridge/Cargo.toml --file crates/mcp-bridge/src/lib.rs --file crates/mcp-bridge/src/session_log.rs --file crates/mcp-bridge/src/local.rs
```
