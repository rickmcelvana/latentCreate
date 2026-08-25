//! `openai_compat`: the universal baseline provider.
//!
//! One implementation covers Ollama, LM Studio, llama.cpp's server, vLLM and
//! OpenRouter, because they all speak `/v1/chat/completions`. What differs
//! between them lives in [`crate::wire`], not here.

use futures_core::stream::BoxStream;
use futures_util::StreamExt;
use serde::Deserialize;

use crate::error::http_error;
use crate::sse::{SseDecoder, SseEvent};
use crate::wire::{ChatChunk, ChatDelta, ChatRequest};
use crate::LlmError;

/// An OpenAI-compatible endpoint.
///
/// Holds the API key for the lifetime of the client. **Never derive `Debug`
/// on this type** -- see the hand-written impl below.
pub struct OpenAiCompat {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

/// Redacts the API key.
///
/// Hand-written because the derived impl would print the key, and then
/// anything that formats a client -- a log line, a panic message, an error
/// wrapped by a caller -- would leak it. T-004 keeps keys out of config and
/// off the frontend; this keeps them out of logs.
impl std::fmt::Debug for OpenAiCompat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompat")
            .field("base_url", &self.base_url)
            .field(
                "api_key",
                &if self.api_key.is_some() {
                    "<set>"
                } else {
                    "<none>"
                },
            )
            .finish()
    }
}

/// One entry of `GET /v1/models`.
#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

/// The `GET /v1/models` envelope: `{"object":"list","data":[...]}`.
#[derive(Debug, Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

impl OpenAiCompat {
    /// Builds a client for `base_url`, e.g. `http://127.0.0.1:11434/v1`.
    ///
    /// `api_key` comes from the OS keychain in the Tauri layer; it is never
    /// read from config and never sent to the frontend (T-004).
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Result<Self, LlmError> {
        let base_url = normalise_base_url(&base_url.into());
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| LlmError::Transport {
                base_url: base_url.clone(),
                detail: e.to_string(),
            })?;
        Ok(Self {
            base_url,
            api_key,
            http,
        })
    }

    /// The endpoint this client talks to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Model ids the endpoint offers, in the order it lists them.
    ///
    /// Only `id` is read: the rest of each entry (`object`, `created`,
    /// `owned_by`) carries nothing the picker needs, and `created` is a file
    /// mtime on local servers rather than a release date.
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = format!("{}/models", self.base_url);
        let response = self
            .authorised(self.http.get(&url))
            .send()
            .await
            .map_err(|e| self.transport(e))?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(|e| self.transport(e))?;
        if !(200..300).contains(&status) {
            return Err(http_error(&self.base_url, status, &body));
        }
        parse_model_list(&body)
    }

    /// Streams a chat completion as [`ChatDelta`]s.
    ///
    /// The stream ends after the provider's `[DONE]` sentinel or on the first
    /// error. Callers forward deltas to the frontend as Tauri events; nothing
    /// here accumulates the answer.
    pub fn stream_chat(
        &self,
        request: ChatRequest,
    ) -> BoxStream<'static, Result<ChatDelta, LlmError>> {
        let start = State::Connecting {
            http: self.http.clone(),
            url: format!("{}/chat/completions", self.base_url),
            api_key: self.api_key.clone(),
            body: stream_body(&request),
            base_url: self.base_url.clone(),
        };
        Box::pin(futures_util::stream::unfold(start, step).flat_map(futures_util::stream::iter))
    }

    /// Adds `Authorization: Bearer` when a key is configured, and nothing at
    /// all when there is none -- local servers reject a malformed header more
    /// readily than a missing one.
    fn authorised(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => builder.bearer_auth(key),
            None => builder,
        }
    }

    fn transport(&self, e: reqwest::Error) -> LlmError {
        LlmError::Transport {
            base_url: self.base_url.clone(),
            detail: e.to_string(),
        }
    }
}

/// Trims trailing slashes so `.../v1` and `.../v1/` build the same URL.
///
/// A trailing slash is what a user pastes out of a browser bar; joining it
/// naively yields `//chat/completions`, which some proxies answer with a 404.
fn normalise_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

/// Model ids out of a `GET /v1/models` body.
fn parse_model_list(body: &str) -> Result<Vec<String>, LlmError> {
    let list: ModelList =
        serde_json::from_str(body).map_err(|e| LlmError::Decode(format!("model list: {e}")))?;
    Ok(list.data.into_iter().map(|entry| entry.id).collect())
}

/// The JSON body for a streaming chat request.
///
/// `stream` and `stream_options.include_usage` are set here rather than by the
/// caller, so a streaming call cannot go out without them. Asking for usage is
/// what produces the trailing `choices: []` frame the wire types handle.
fn stream_body(request: &ChatRequest) -> serde_json::Value {
    let mut body = serde_json::to_value(request).unwrap_or(serde_json::Value::Null);
    if let Some(map) = body.as_object_mut() {
        map.insert("stream".to_string(), serde_json::Value::Bool(true));
        map.insert(
            "stream_options".to_string(),
            serde_json::json!({ "include_usage": true }),
        );
    }
    body
}

/// Where the stream has got to.
enum State {
    Connecting {
        http: reqwest::Client,
        url: String,
        api_key: Option<String>,
        body: serde_json::Value,
        base_url: String,
    },
    Streaming {
        body: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
        decoder: SseDecoder,
        base_url: String,
    },
    Done,
}

/// Advances the stream by one network read, yielding whatever that completed.
///
/// Yields a `Vec` rather than one item because a single read routinely
/// completes several chunks, and the usage frame adds one more with no text.
async fn step(state: State) -> Option<(Vec<Result<ChatDelta, LlmError>>, State)> {
    match state {
        State::Connecting {
            http,
            url,
            api_key,
            body,
            base_url,
        } => {
            let mut builder = http.post(&url).json(&body);
            if let Some(key) = &api_key {
                builder = builder.bearer_auth(key);
            }
            let response = match builder.send().await {
                Ok(response) => response,
                Err(e) => {
                    let detail = e.to_string();
                    return Some((
                        vec![Err(LlmError::Transport { base_url, detail })],
                        State::Done,
                    ));
                }
            };
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                let body = response.text().await.unwrap_or_default();
                return Some((vec![Err(http_error(&base_url, status, &body))], State::Done));
            }
            Some((
                Vec::new(),
                State::Streaming {
                    body: Box::pin(response.bytes_stream()),
                    decoder: SseDecoder::new(),
                    base_url,
                },
            ))
        }
        State::Streaming {
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
                        State::Done,
                    ));
                }
                None => return None,
            };
            let events = match decoder.push(&chunk) {
                Ok(events) => events,
                Err(e) => return Some((vec![Err(e)], State::Done)),
            };
            let mut out = Vec::new();
            for event in events {
                match event {
                    SseEvent::Done => return Some((out, State::Done)),
                    SseEvent::Data(json) => match serde_json::from_str::<ChatChunk>(&json) {
                        Ok(chunk) => out.extend(chunk.deltas().into_iter().map(Ok)),
                        Err(e) => out.push(Err(LlmError::Decode(format!("chunk: {e}")))),
                    },
                }
            }
            Some((
                out,
                State::Streaming {
                    body,
                    decoder,
                    base_url,
                },
            ))
        }
        State::Done => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live `GET /v1/models` body from Ollama, trimmed to three entries
    /// (captured 2026-08-24, docs/LLM-SURFACE.md 1).
    const MODEL_LIST: &str = r#"{"object":"list","data":[
        {"id":"kimi-k3:cloud","object":"model","created":1787613495,"owned_by":"library"},
        {"id":"gemma4:12b-32k","object":"model","created":1784893439,"owned_by":"library"},
        {"id":"qwen3.5:9b","object":"model","created":1782414066,"owned_by":"library"}]}"#;

    /// Protects: the API key never reaches a log line. Anything that formats
    /// a client -- a `log::debug!`, a panic message, a caller wrapping the
    /// error -- goes through this impl, so a derived `Debug` would put the
    /// user's OpenRouter key in their diagnostics pane.
    #[test]
    fn test_debug_never_prints_the_api_key() {
        let client = OpenAiCompat::new("http://127.0.0.1:11434/v1", Some("sk-secret-123".into()))
            .expect("client");
        let rendered = format!("{client:?}");
        assert!(
            !rendered.contains("sk-secret-123"),
            "Debug leaked the key: {rendered}"
        );
        assert!(rendered.contains("<set>"));

        let anonymous = OpenAiCompat::new("http://127.0.0.1:11434/v1", None).expect("client");
        assert!(format!("{anonymous:?}").contains("<none>"));
    }

    /// Protects: a pasted base URL with a trailing slash still builds a valid
    /// path. `.../v1/` + `/models` would be `//models`, which some proxies
    /// answer with a 404 the user cannot interpret.
    #[test]
    fn test_base_url_trailing_slash_is_normalised() {
        let client = OpenAiCompat::new("  http://127.0.0.1:11434/v1/  ", None).expect("client");
        assert_eq!(client.base_url(), "http://127.0.0.1:11434/v1");
    }

    /// Protects: the model list decodes to ids in wire order. Wire order is
    /// Ollama's recency order, which the wizard's picker relies on.
    #[test]
    fn test_model_list_decodes_ids_in_order() {
        let ids = parse_model_list(MODEL_LIST).expect("model list");
        assert_eq!(ids, ["kimi-k3:cloud", "gemma4:12b-32k", "qwen3.5:9b"]);
    }

    /// Protects: an endpoint answering with something other than the model
    /// envelope is a typed decode error, not a panic or an empty list. An
    /// empty list would read as "you have no models installed".
    #[test]
    fn test_model_list_rejects_a_non_envelope_body() {
        let err = parse_model_list("404 page not found").expect_err("should not decode");
        assert!(matches!(err, LlmError::Decode(_)), "got {err:?}");
    }

    /// Protects: streaming is not optional. A request that went out without
    /// `stream: true` would return one whole completion and the UI would sit
    /// blank until the model finished; without `include_usage` there are no
    /// token counts to show.
    #[test]
    fn test_stream_body_always_requests_streaming_and_usage() {
        let body = stream_body(&ChatRequest {
            model: "gemma4:12b-it-qat".to_string(),
            messages: vec![crate::wire::ChatMessage {
                role: crate::wire::Role::User,
                content: "write a chorus".to_string(),
            }],
            temperature: None,
            max_tokens: None,
        });
        assert_eq!(body["stream"], serde_json::json!(true));
        assert_eq!(
            body["stream_options"]["include_usage"],
            serde_json::json!(true)
        );
        assert_eq!(body["model"], serde_json::json!("gemma4:12b-it-qat"));
        assert_eq!(body["messages"][0]["role"], serde_json::json!("user"));
    }

    /// Protects: optional fields stay out of the body when unset. Sending
    /// `"temperature": null` is rejected outright by some OpenAI-compatible
    /// servers, which is a failure the user cannot diagnose.
    #[test]
    fn test_unset_options_are_omitted_not_null() {
        let body = stream_body(&ChatRequest {
            model: "m".to_string(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
        });
        let map = body.as_object().expect("object");
        assert!(!map.contains_key("temperature"));
        assert!(!map.contains_key("max_tokens"));
    }

    /// Live check against a real endpoint, excluded from CI.
    ///
    /// `cargo test -p llm-bridge -- --ignored` with Ollama running on the
    /// default port. Proves the parts unit tests cannot: that reqwest streams
    /// the body incrementally rather than buffering it, that the request is
    /// accepted as written, and that a real answer comes back separated into
    /// content and reasoning.
    #[tokio::test]
    #[ignore = "requires a local OpenAI-compatible endpoint on 127.0.0.1:11434"]
    async fn test_live_stream_returns_content_separated_from_reasoning() {
        use futures_util::StreamExt;

        let client = OpenAiCompat::new("http://127.0.0.1:11434/v1", None).expect("client");
        let models = client.list_models().await.expect("model list");
        assert!(!models.is_empty(), "endpoint offers no models");

        let model = models
            .iter()
            .find(|id| id.starts_with("gemma4:") || id.starts_with("qwen3.5:9b"))
            .cloned()
            .unwrap_or_else(|| models[0].clone());

        let mut stream = client.stream_chat(ChatRequest {
            model,
            messages: vec![crate::wire::ChatMessage {
                role: crate::wire::Role::User,
                content: "Reply with exactly: tulip".to_string(),
            }],
            temperature: None,
            max_tokens: Some(300),
        });

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut finished = None;
        while let Some(delta) = stream.next().await {
            match delta.expect("delta") {
                ChatDelta::Content(text) => content.push_str(&text),
                ChatDelta::Reasoning(text) => reasoning.push_str(&text),
                ChatDelta::Finished { reason } => finished = reason,
                ChatDelta::Usage(_) | ChatDelta::Refusal(_) => {}
            }
        }

        assert!(
            content.to_lowercase().contains("tulip"),
            "content was {content:?} (reasoning {} chars)",
            reasoning.chars().count()
        );
        assert_eq!(finished.as_deref(), Some("stop"));
    }
}
