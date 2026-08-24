//! Rotating, redacting session log (ARCHITECTURE.md 3).
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
