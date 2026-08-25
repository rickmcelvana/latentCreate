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
