# T-108a: `llm-bridge` errors and SSE framing
**Depends:** T-004 (keychain boundary, for context only) | **Crate/dir:** `crates/llm-bridge`
**Files to create/modify:**
- `crates/llm-bridge/Cargo.toml` (modify: three dependencies)
- `crates/llm-bridge/src/error.rs` (create)
- `crates/llm-bridge/src/sse.rs` (create)
- `crates/llm-bridge/src/lib.rs` (modify: two `pub mod`, three `pub use`)

## Goal
The transport-free half of `llm-bridge`: a typed error with the one thing that makes an
LLM misconfiguration diagnosable, and an SSE decoder that survives real networks. No HTTP
client, no async, no new async dependencies. Everything here is verified against live
captures recorded in [docs/LLM-SURFACE.md](../docs/LLM-SURFACE.md) — read sections 4 and 6
before starting.

## Spec
Exactly the reference implementation below. The two contracts worth stating as claims:

**`http_error` decodes the OpenAI envelope, then falls back to the raw body.** An error
body is *not* necessarily JSON: a user who pastes `http://127.0.0.1:11434` instead of
`.../v1` gets `404 page not found` as plain text (LLM-SURFACE 4). A decoder that insists on
the envelope reports `expected value at line 1 column 1` and buries the one sentence that
would let the user fix their setting.

**`SseDecoder` buffers bytes, not text.** Four things happen on real connections and each
one breaks a naive parser:
- an event arrives split across reads — buffer until a blank line;
- a **multi-byte character** is split across reads — so UTF-8 is decoded only once a whole
  event has arrived, never per read. Lyrics are written in 50+ languages, so this is
  routine, not exotic;
- comment heartbeats (`: OPENROUTER PROCESSING`) arrive mid-stream — parsing one as JSON
  would fail the whole request;
- `

` framing appears behind proxies — a decoder that knows only `

` emits
  nothing at all and looks like a hung model.

`push` returns `Result` because invalid UTF-8 in a *complete* event means a corrupted
stream, and silently substituting replacement characters would corrupt the user's lyrics
instead of reporting the fault.

## Reference implementation
Transcribe verbatim. This compiles, `cargo fmt` is a no-op on it, `cargo clippy
--all-targets -- -D warnings` is clean, and its 9 tests pass.

### 1. `crates/llm-bridge/Cargo.toml` (complete file)
Three dependencies, all already used elsewhere in the workspace. **No async or HTTP
dependency belongs in this task** — those arrive in T-108c.

```toml
[package]
name = "llm-bridge"
description = "LLM providers for lyric writing behind one trait."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
```

### 2. `crates/llm-bridge/src/error.rs` (new file, complete)
```rust
//! Errors from any LLM provider.

use serde::Deserialize;
use thiserror::Error;

/// Anything that can go wrong talking to an LLM endpoint.
#[derive(Debug, Error)]
pub enum LlmError {
    /// The endpoint could not be reached at all: wrong host/port, DNS, TLS,
    /// or the server went away mid-stream.
    #[error("cannot reach {base_url}: {detail}")]
    Transport { base_url: String, detail: String },
    /// The endpoint answered with a non-success status.
    ///
    /// `message` is the provider's own wording when the body carried an
    /// OpenAI-style error envelope, and the raw body otherwise -- a wrong base
    /// URL path answers in plain text, not JSON (docs/LLM-SURFACE.md 4).
    #[error("{base_url} returned HTTP {status}: {message}")]
    Http {
        base_url: String,
        status: u16,
        message: String,
    },
    /// A frame arrived that is not what the wire format promises.
    #[error("could not decode the response: {0}")]
    Decode(String),
}

/// The OpenAI-style error envelope: `{"error": {"message", "type", ...}}`.
///
/// Verified live against Ollama for both an unknown model (404) and a
/// malformed request (400); the field set matches OpenAI's documented
/// envelope. Only `message` and `type` are read -- `param` and `code` are
/// null on every capture taken.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// The inner object of [`ErrorEnvelope`].
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorBody {
    pub message: String,
    #[serde(default)]
    #[serde(rename = "type")]
    pub ty: Option<String>,
}

/// Builds an [`LlmError::Http`] from a failed response.
///
/// Tries the OpenAI error envelope first and falls back to the raw body,
/// because not every non-2xx answer is JSON: a base URL missing its `/v1`
/// prefix answers `404 page not found` as plain text, and telling the user
/// that verbatim is more useful than "expected value at line 1 column 1".
pub fn http_error(base_url: &str, status: u16, body: &str) -> LlmError {
    let message = match serde_json::from_str::<ErrorEnvelope>(body) {
        Ok(envelope) => envelope.error.message,
        Err(_) => {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                "no response body".to_string()
            } else {
                trimmed.to_string()
            }
        }
    };
    LlmError::Http {
        base_url: base_url.to_string(),
        status,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protects: the provider's own wording reaches the user. Ollama's
    /// unknown-model 404, captured live.
    #[test]
    fn test_http_error_reads_the_openai_envelope() {
        let body = r#"{"error":{"message":"model 'no-such-model:99b' not found","type":"not_found_error","param":null,"code":null}}"#;
        let err = http_error("http://127.0.0.1:11434/v1", 404, body);
        match err {
            LlmError::Http {
                status, message, ..
            } => {
                assert_eq!(status, 404);
                assert_eq!(message, "model 'no-such-model:99b' not found");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    /// Protects: a non-JSON error body is passed through, not swallowed by a
    /// parse failure. A base URL missing `/v1` answers in plain text, and the
    /// user needs to see it to fix their setting.
    #[test]
    fn test_http_error_falls_back_to_a_plain_text_body() {
        let err = http_error("http://127.0.0.1:11434", 404, "404 page not found\n");
        match err {
            LlmError::Http { message, .. } => assert_eq!(message, "404 page not found"),
            other => panic!("expected Http, got {other:?}"),
        }
    }

    /// Protects: an empty body still says something. A bare "HTTP 502" with
    /// no explanation is what the user would otherwise be shown.
    #[test]
    fn test_http_error_describes_an_empty_body() {
        let err = http_error("http://example.invalid/v1", 502, "");
        match err {
            LlmError::Http { message, .. } => assert_eq!(message, "no response body"),
            other => panic!("expected Http, got {other:?}"),
        }
    }
}
```

### 3. `crates/llm-bridge/src/sse.rs` (new file, complete)
```rust
//! Server-sent-events framing.
//!
//! Transport-free on purpose: bytes in, complete events out. The HTTP client
//! feeds it whatever the socket happened to deliver, which is why every rule
//! here is about *incomplete* input.

use crate::LlmError;

/// One `data:` payload lifted out of an SSE stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// A `data:` payload. JSON, for every provider this crate targets.
    Data(String),
    /// The `data: [DONE]` sentinel that ends an OpenAI-style stream.
    Done,
}

/// Reassembles SSE events from arbitrarily chunked network reads.
///
/// Buffers **bytes**, not text, and decodes UTF-8 only once a whole event has
/// arrived. A chunk boundary can fall inside a multi-byte character -- lyrics
/// are written in 50+ languages, so this is a matter of course, not an edge
/// case -- and decoding each read on its own would corrupt exactly those
/// characters.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    /// A decoder with an empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one network chunk and returns every event completed by it.
    ///
    /// A partial event stays buffered for the next call. Comment lines
    /// (`: keep-alive`) and non-`data:` fields (`event:`, `id:`, `retry:`) are
    /// dropped, per the SSE spec -- OpenRouter sends comment heartbeats while
    /// a slow model warms up.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, LlmError> {
        self.buf.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some((end, delimiter)) = find_blank_line(&self.buf) {
            let frame = self.buf[..end].to_vec();
            self.buf.drain(..end + delimiter);
            let text = std::str::from_utf8(&frame)
                .map_err(|e| LlmError::Decode(format!("event is not valid UTF-8: {e}")))?;
            if let Some(event) = parse_event(text) {
                events.push(event);
            }
        }
        Ok(events)
    }
}

/// Offset and length of the first blank line, which terminates an event.
///
/// Both `\n\n` and `\r\n\r\n` occur in the wild; proxies rewrite line endings.
fn find_blank_line(buf: &[u8]) -> Option<(usize, usize)> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i..].starts_with(b"\r\n\r\n") {
            return Some((i, 4));
        }
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
    }
    None
}

/// Turns one complete frame into an event, or `None` when it carries no data.
fn parse_event(frame: &str) -> Option<SseEvent> {
    let mut data = String::new();
    let mut seen_data = false;
    for line in frame.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let Some(value) = line.strip_prefix("data:") else {
            continue;
        };
        if seen_data {
            data.push('\n');
        }
        data.push_str(value.strip_prefix(' ').unwrap_or(value));
        seen_data = true;
    }
    if !seen_data {
        return None;
    }
    if data.trim() == "[DONE]" {
        return Some(SseEvent::Done);
    }
    Some(SseEvent::Data(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protects: an event split across two reads is reassembled, not lost or
    /// half-parsed. The split falls inside the JSON body, which is what a
    /// short TCP read looks like.
    #[test]
    fn test_event_split_across_reads_is_reassembled() {
        let mut decoder = SseDecoder::new();
        assert_eq!(decoder.push(b"data: {\"choices\":[{\"de").unwrap(), vec![]);
        let events = decoder.push(b"lta\":{\"content\":\"hi\"}}]}\n\n").unwrap();
        assert_eq!(
            events,
            vec![SseEvent::Data(
                "{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}".to_string()
            )]
        );
    }

    /// Protects: the reason this buffers bytes rather than text. The split
    /// falls between the two bytes of `e` + combining acute, and a decoder
    /// that called `from_utf8` per read would produce a replacement character
    /// in the middle of someone's lyrics.
    #[test]
    fn test_multibyte_character_split_across_reads_survives() {
        let payload = "data: {\"content\":\"caf\u{00e9}\"}\n\n".as_bytes();
        let split = payload
            .iter()
            .position(|b| *b == 0xc3)
            .expect("the two-byte e-acute");
        let mut decoder = SseDecoder::new();
        assert_eq!(decoder.push(&payload[..split + 1]).unwrap(), vec![]);
        let events = decoder.push(&payload[split + 1..]).unwrap();
        assert_eq!(
            events,
            vec![SseEvent::Data("{\"content\":\"caf\u{00e9}\"}".to_string())]
        );
    }

    /// Protects: several events in one read all come out, in order. Ollama
    /// delivers bursts of chunks in a single body read.
    #[test]
    fn test_several_events_in_one_read_come_out_in_order() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push(b"data: one\n\ndata: two\n\ndata: [DONE]\n\n")
            .unwrap();
        assert_eq!(
            events,
            vec![
                SseEvent::Data("one".to_string()),
                SseEvent::Data("two".to_string()),
                SseEvent::Done,
            ]
        );
    }

    /// Protects: heartbeats and non-data fields never reach the caller as
    /// content. OpenRouter sends `: OPENROUTER PROCESSING` comments while a
    /// model warms up; parsing one as JSON would fail the whole stream.
    #[test]
    fn test_comments_and_other_fields_are_dropped() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push(b": OPENROUTER PROCESSING\n\nevent: message\nid: 7\ndata: kept\n\n")
            .unwrap();
        assert_eq!(events, vec![SseEvent::Data("kept".to_string())]);
    }

    /// Protects: CRLF framing is recognised. Some proxies rewrite line
    /// endings, and a decoder that only knows `\n\n` buffers the whole
    /// response and emits nothing at all.
    #[test]
    fn test_crlf_framing_is_recognised() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push(b"data: one\r\n\r\ndata: [DONE]\r\n\r\n")
            .unwrap();
        assert_eq!(
            events,
            vec![SseEvent::Data("one".to_string()), SseEvent::Done]
        );
    }

    /// Protects: the multi-line `data:` rule. The SSE spec joins repeated
    /// `data:` lines with a newline; taking only the last would truncate.
    #[test]
    fn test_repeated_data_lines_join_with_a_newline() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(b"data: first\ndata: second\n\n").unwrap();
        assert_eq!(events, vec![SseEvent::Data("first\nsecond".to_string())]);
    }
}
```

### 4. `crates/llm-bridge/src/lib.rs` (complete file after the change)
The existing `mod tests` block is unchanged; only the module declarations and re-exports
above it are new. Modules and re-exports are alphabetical, matching `library/src/lib.rs`.

```rust
//! LLM providers for lyric writing behind one trait.
//!
//! `openai_compat` is the universal baseline (Ollama, LM Studio, llama.cpp,
//! OpenRouter, vLLM); native providers are conveniences (ARCHITECTURE.md §4).
//! Populated in Phase 1.

pub mod error;
pub mod sse;

/// Re-export of [`error::LlmError`].
pub use error::LlmError;
/// Re-export of [`sse::SseDecoder`].
pub use sse::SseDecoder;
/// Re-export of [`sse::SseEvent`].
pub use sse::SseEvent;

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "llm-bridge");
    }
}
```

## Acceptance criteria
- [ ] `cargo test -p llm-bridge` passes: **10 tests** (the 9 new ones plus the existing
      `test_crate_name_is_stable`)
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `npm run gate` green
- [ ] no changes outside the four listed files
- [ ] no dependency beyond the three in the Cargo.toml above

## Out of scope
- Any HTTP client, `reqwest`, or async code (T-108c).
- The chat-completion chunk types and the reasoning/content split (T-108b).
- The `LlmProvider` trait. Deferred until T-109 provides a second implementation, for the
  same reason `ComfyBackend` was (ARCHITECTURE 3, 4).

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --file crates/llm-bridge/Cargo.toml --file crates/llm-bridge/src/error.rs --file crates/llm-bridge/src/sse.rs --file crates/llm-bridge/src/lib.rs
```
`docs/LLM-SURFACE.md` is `--read` because every rule in the spec above cites it, and it may
not be edited (WORKFLOW 3).
