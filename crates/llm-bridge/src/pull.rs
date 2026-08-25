//! Model downloads: `/api/pull`, and the NDJSON framing it uses.
//!
//! A different wire format from the chat stream -- newline-delimited JSON, no
//! `data:` prefix and no blank-line delimiter -- so it gets its own decoder
//! rather than bending [`crate::sse`] to fit two protocols.
//!
//! Shapes verified live against Ollama 0.32.15 on 2026-08-24 by pulling a
//! 46 MB model and capturing every frame: docs/LLM-SURFACE.md 9.

use futures_core::stream::BoxStream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::LlmError;

/// Reassembles newline-delimited JSON from arbitrarily chunked reads.
///
/// Byte-buffered for the same reason [`crate::sse::SseDecoder`] is: a read can
/// end in the middle of a multi-byte character, and decoding each read on its
/// own corrupts it.
#[derive(Debug, Default)]
pub struct NdjsonDecoder {
    buf: Vec<u8>,
}

impl NdjsonDecoder {
    /// A decoder with an empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one network chunk and returns every complete line in it.
    ///
    /// Blank lines are dropped. A partial line stays buffered.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, LlmError> {
        self.buf.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(end) = self.buf.iter().position(|b| *b == b'\n') {
            let line = self.buf[..end].to_vec();
            self.buf.drain(..end + 1);
            let text = std::str::from_utf8(&line)
                .map_err(|e| LlmError::Decode(format!("pull frame is not valid UTF-8: {e}")))?;
            let text = text.trim();
            if !text.is_empty() {
                lines.push(text.to_string());
            }
        }
        Ok(lines)
    }
}

/// One frame of a pull stream.
///
/// Every field except `status` is optional, and that is load-bearing rather
/// than defensive: of 23 frames in the reference capture, 19 carried `digest`
/// and `total` while only **11** carried `completed` -- the first frame for
/// each layer arrives before any bytes have landed. An error frame carries no
/// `status` at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullProgress {
    /// `"pulling manifest"`, `"pulling <short digest>"`, `"verifying sha256
    /// digest"`, `"writing manifest"`, or `"success"`.
    #[serde(default)]
    pub status: String,
    /// Layer being fetched, e.g. `"sha256:797b70c4..."`.
    #[serde(default)]
    pub digest: Option<String>,
    /// Bytes in this layer.
    #[serde(default)]
    pub total: Option<u64>,
    /// Bytes fetched so far. **Absent, not zero**, before the first byte.
    #[serde(default)]
    pub completed: Option<u64>,
    /// Set when the pull failed. See [`PullProgress::failure`].
    #[serde(default)]
    pub error: Option<String>,
}

/// Terminal status Ollama sends when a pull completed.
pub const PULL_SUCCESS: &str = "success";

impl PullProgress {
    /// Whether this frame reports the pull finished successfully.
    pub fn is_success(&self) -> bool {
        self.status == PULL_SUCCESS
    }

    /// The failure this frame reports, if any.
    ///
    /// **`/api/pull` answers HTTP 200 even when the pull fails** -- the
    /// failure arrives as a frame in the body, exactly like comfy-mcp's
    /// `Ok(is_error: true)` (MCP-SURFACE 8). A client that checks only the
    /// status code reports a failed download as a success, and the user is
    /// left with a model that is not there.
    pub fn failure(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Progress through the current layer, 0.0..=1.0.
    ///
    /// `None` when there is nothing to show a bar for: the manifest and
    /// verification steps carry no `total`. A frame with `total` but no
    /// `completed` is 0.0, not `None` -- the layer has started.
    pub fn fraction(&self) -> Option<f32> {
        match (self.total, self.completed) {
            (Some(0), _) => None,
            (Some(total), Some(completed)) => Some((completed as f32 / total as f32).min(1.0)),
            (Some(_), None) => Some(0.0),
            (None, _) => None,
        }
    }
}

impl crate::ollama::OllamaNative {
    /// Downloads a model, streaming progress.
    ///
    /// **Never call this without the user asking.** Models are gigabytes on
    /// someone else's connection and disk; the wizard offers a button, never
    /// an automatic fetch (tasks/phase-1.md T-112).
    ///
    /// The stream ends after the `success` frame, or yields an error and stops.
    pub fn pull(&self, model: &str) -> BoxStream<'static, Result<PullProgress, LlmError>> {
        let start = PullState::Connecting {
            http: self.http_client(),
            url: format!("{}/api/pull", self.base_url()),
            base_url: self.base_url().to_string(),
            body: serde_json::json!({ "model": model, "stream": true }),
        };
        Box::pin(
            futures_util::stream::unfold(start, pull_step).flat_map(futures_util::stream::iter),
        )
    }
}

/// Where a pull has got to.
enum PullState {
    Connecting {
        http: reqwest::Client,
        url: String,
        base_url: String,
        body: serde_json::Value,
    },
    Streaming {
        body: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
        decoder: NdjsonDecoder,
        base_url: String,
    },
    Done,
}

/// Advances a pull by one network read.
async fn pull_step(state: PullState) -> Option<(Vec<Result<PullProgress, LlmError>>, PullState)> {
    match state {
        PullState::Connecting {
            http,
            url,
            base_url,
            body,
        } => {
            let response = match http.post(&url).json(&body).send().await {
                Ok(response) => response,
                Err(e) => {
                    let detail = e.to_string();
                    return Some((
                        vec![Err(LlmError::Transport { base_url, detail })],
                        PullState::Done,
                    ));
                }
            };
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                let body = response.text().await.unwrap_or_default();
                return Some((
                    vec![Err(crate::error::http_error(&base_url, status, &body))],
                    PullState::Done,
                ));
            }
            Some((
                Vec::new(),
                PullState::Streaming {
                    body: Box::pin(response.bytes_stream()),
                    decoder: NdjsonDecoder::new(),
                    base_url,
                },
            ))
        }
        PullState::Streaming {
            mut body,
            mut decoder,
            base_url,
        } => {
            let chunk = match body.next().await {
                Some(Ok(chunk)) => chunk,
                Some(Err(e)) => {
                    let detail = e.to_string();
                    return Some((
                        vec![Err(LlmError::Transport { base_url, detail })],
                        PullState::Done,
                    ));
                }
                None => return None,
            };
            let lines = match decoder.push(&chunk) {
                Ok(lines) => lines,
                Err(e) => return Some((vec![Err(e)], PullState::Done)),
            };
            let mut out = Vec::new();
            for line in lines {
                match serde_json::from_str::<PullProgress>(&line) {
                    Ok(frame) => {
                        if let Some(detail) = frame.failure() {
                            out.push(Err(LlmError::Reported {
                                detail: detail.to_string(),
                            }));
                            return Some((out, PullState::Done));
                        }
                        let finished = frame.is_success();
                        out.push(Ok(frame));
                        if finished {
                            return Some((out, PullState::Done));
                        }
                    }
                    Err(e) => out.push(Err(LlmError::Decode(format!("pull frame: {e}")))),
                }
            }
            Some((
                out,
                PullState::Streaming {
                    body,
                    decoder,
                    base_url,
                },
            ))
        }
        PullState::Done => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real pull of `all-minilm` (46 MB), captured 2026-08-24.
    const PULL: &str = include_str!("../../../testdata/llm/ollama-pull.ndjson");

    fn frames() -> Vec<PullProgress> {
        let mut decoder = NdjsonDecoder::new();
        let mut out = Vec::new();
        // 48-byte reads, so lines land mid-JSON as they do on a socket.
        for piece in PULL.as_bytes().chunks(48) {
            for line in decoder.push(piece).expect("decodes") {
                out.push(serde_json::from_str::<PullProgress>(&line).expect("frame"));
            }
        }
        out
    }

    /// Protects: the whole capture replays through the decoder, and the pull
    /// ends the way a caller detects completion.
    #[test]
    fn test_real_pull_replays_and_ends_in_success() {
        let frames = frames();
        assert_eq!(frames.len(), 23);
        assert_eq!(frames[0].status, "pulling manifest");
        assert!(frames.last().expect("last frame").is_success());
        assert_eq!(frames.iter().filter(|f| f.is_success()).count(), 1);
    }

    /// Protects: `completed` is absent, not zero, on a layer's first frame.
    /// Typing it `u64` with a serde default would silently report 0 bytes
    /// fetched, which is indistinguishable from a stalled download.
    #[test]
    fn test_completed_is_absent_on_a_layers_first_frame() {
        let frames = frames();
        let with_total = frames.iter().filter(|f| f.total.is_some()).count();
        let with_completed = frames.iter().filter(|f| f.completed.is_some()).count();
        assert_eq!(with_total, 19);
        assert_eq!(with_completed, 11);

        let first_layer = frames
            .iter()
            .find(|f| f.total == Some(45_949_216))
            .expect("the 46 MB layer");
        assert_eq!(first_layer.completed, None);
        assert_eq!(first_layer.fraction(), Some(0.0), "started, not stalled");
    }

    /// Protects: steps with nothing to measure show no bar rather than a bar
    /// stuck at zero.
    #[test]
    fn test_manifest_and_verify_steps_have_no_fraction() {
        let frames = frames();
        for status in ["pulling manifest", "verifying sha256 digest", "success"] {
            let frame = frames
                .iter()
                .find(|f| f.status == status)
                .unwrap_or_else(|| panic!("no {status} frame"));
            assert_eq!(frame.fraction(), None, "{status} should have no fraction");
        }
    }

    /// Protects: a completed layer reads as exactly 1.0, so a bar reaches the
    /// end rather than resting at 99%.
    #[test]
    fn test_a_finished_layer_is_exactly_one() {
        let frames = frames();
        let done = frames
            .iter()
            .find(|f| f.total.is_some() && f.total == f.completed)
            .expect("a finished layer");
        assert_eq!(done.fraction(), Some(1.0));
    }

    /// Protects: the trap. `/api/pull` answers **HTTP 200** and puts the
    /// failure in the body, so a client trusting the status code tells the
    /// user a download succeeded when no model was fetched. Frame captured
    /// verbatim from a pull of a nonexistent model.
    #[test]
    fn test_a_failure_frame_is_recognised_despite_http_200() {
        let frame: PullProgress =
            serde_json::from_str(r#"{"error":"pull model manifest: file does not exist"}"#)
                .expect("error frame decodes");
        assert_eq!(
            frame.failure(),
            Some("pull model manifest: file does not exist")
        );
        assert!(!frame.is_success());
        assert_eq!(frame.status, "", "an error frame carries no status");
    }

    /// Protects: NDJSON lines split across reads are reassembled rather than
    /// producing two invalid fragments.
    #[test]
    fn test_line_split_across_reads_is_reassembled() {
        let mut decoder = NdjsonDecoder::new();
        assert_eq!(
            decoder.push(b"{\"status\":\"pull").expect("ok"),
            Vec::<String>::new()
        );
        assert_eq!(
            decoder.push(b"ing manifest\"}\n").expect("ok"),
            vec!["{\"status\":\"pulling manifest\"}".to_string()]
        );
    }

    /// Live check against a real server, excluded from CI.
    ///
    /// `cargo test -p llm-bridge -- --ignored` with Ollama running. Pulls
    /// `all-minilm` (46 MB), which the capture above installed, so it
    /// re-verifies an existing model rather than downloading one -- it proves
    /// the NDJSON transport against the live server without spending
    /// bandwidth. On a machine without that model it does download it.
    #[tokio::test]
    #[ignore = "requires a local Ollama on 127.0.0.1:11434"]
    async fn test_live_pull_of_an_installed_model_reaches_success() {
        use futures_util::StreamExt;

        let client = crate::ollama::OllamaNative::new("http://127.0.0.1:11434").expect("client");
        let mut stream = client.pull("all-minilm");

        let mut statuses = Vec::new();
        let mut succeeded = false;
        while let Some(frame) = stream.next().await {
            let frame = frame.expect("pull frame");
            succeeded |= frame.is_success();
            statuses.push(frame.status);
        }

        assert!(succeeded, "pull did not reach success: {statuses:?}");
        assert_eq!(
            statuses.first().map(String::as_str),
            Some("pulling manifest")
        );
    }
}
