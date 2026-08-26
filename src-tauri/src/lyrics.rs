//! Lyric generation: stream a lyric from the user's LLM and re-emit it as
//! Tauri events.
//!
//! The backend streams; the frontend accumulates. Nothing here writes a lyric
//! document -- that is the lyrics store's job. This module owns the policy from
//! LLM-SURFACE 12.2: `reasoning_effort: "none"` is sent only when the model is
//! known to think, because the field is verified against Ollama and nothing
//! else.

use std::sync::{Arc, Mutex};

use create_core::lyrics::{
    assemble_system_prompt, assemble_user_message, token_budget, LyricBrief,
};
use futures_util::StreamExt;
use llm_bridge::{ChatDelta, ChatMessage, ChatRequest, LlmError, OpenAiCompat, Role, TokenUsage};
use serde::Serialize;
use tauri::{async_runtime, AppHandle, Emitter, State};

use crate::llm::enrich;
use crate::{ConfigDir, ProfilesDir};

/// Emitted for each content delta. Only content reaches the document; reasoning
/// goes to [`LyricThinking`].
#[derive(Debug, Clone, Serialize)]
pub struct LyricDelta {
    pub text: String,
}

/// Emitted for each reasoning delta. Shown as status text, never appended to
/// the lyric (LLM-SURFACE 12: a generation can be 44 seconds of reasoning
/// before the first word of song).
#[derive(Debug, Clone, Serialize)]
pub struct LyricThinking {
    pub text: String,
}

/// Emitted once when the stream finishes.
#[derive(Debug, Clone, Serialize)]
pub struct LyricDone {
    /// The model's own stop reason, e.g. `"stop"` or `"length"`. **Reaches the
    /// frontend intact**: truncation is an outcome the UI must state, not an
    /// error to swallow (LLM-SURFACE 12.1).
    pub finish_reason: Option<String>,
    /// Token counts, present when the endpoint was asked for them.
    pub usage: Option<TokenUsage>,
}

/// Emitted once when the stream fails.
#[derive(Debug, Clone, Serialize)]
pub struct LyricFailed {
    pub error: String,
}

/// The active lyric generation's abort handle, held as Tauri managed state.
///
/// One generation at a time: writing a lyric is an iterative loop on a single
/// draft, and a second `lyrics_generate` aborts the first (CONVENTIONS: no
/// detached fire-and-forget loops).
#[derive(Default)]
pub struct LyricsState {
    current: Arc<Mutex<Option<tokio::task::AbortHandle>>>,
}

/// Whether to suppress reasoning for this model, per the LLM-SURFACE 12.2
/// policy.
///
/// The field is verified against Ollama only, so it is sent only when the model
/// is *known* to think -- `Some(true)` from the native enrichment. Unknown
/// (`None`) means "could not check", and the unverified path is never taken.
fn reasoning_effort_for(thinks: Option<bool>) -> Option<String> {
    match thinks {
        Some(true) => Some("none".to_string()),
        _ => None,
    }
}

/// Whether the configured model is known to think, from the Ollama enrichment.
///
/// `None` means either the endpoint is not Ollama or the model was not found,
/// both of which leave the field unset -- never guessed.
async fn model_thinks(base_url: &str, model: &str) -> Option<bool> {
    let native = enrich(base_url).await?;
    native.iter().find(|m| m.name == model).map(|m| m.thinks())
}

/// Stream a lyric for `brief` against `profile_id`, re-emitting it as
/// `lyrics://delta|thinking|done|failed` events.
///
/// The brief and the profile are the inputs; the LLM is whatever `config.json`
/// has configured. Returns once the generation has been submitted, not when it
/// finishes -- the events carry the progress.
#[tauri::command]
pub async fn lyrics_generate(
    app: AppHandle,
    state: State<'_, LyricsState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    brief: LyricBrief,
    profile_id: String,
) -> Result<(), String> {
    let current = Arc::clone(&state.current);
    if let Some(previous) = current.lock().expect("lyrics state poisoned").take() {
        previous.abort();
    }

    let config = library::config::load(&config_dir.0).config;
    let llm = config
        .llm
        .ok_or_else(|| "no lyric LLM configured".to_string())?;
    let base_url = llm
        .base_url
        .ok_or_else(|| "no lyric LLM base URL configured".to_string())?;
    let model = llm
        .model
        .ok_or_else(|| "no lyric model configured".to_string())?;

    let profile = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"))
        .profiles
        .get(&profile_id)
        .map(|loaded| loaded.profile.clone())
        .ok_or_else(|| format!("no profile named {profile_id}"))?;

    let system = assemble_system_prompt(&profile, &brief);
    let user = assemble_user_message(&brief);

    let key = library::secrets::get_secret(library::SecretKey::LlmApiKey).ok();
    let client = OpenAiCompat::new(base_url.clone(), key).map_err(|e| e.to_string())?;

    let thinks = model_thinks(&base_url, &model).await;
    let request = ChatRequest {
        model: model.clone(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: system,
            },
            ChatMessage {
                role: Role::User,
                content: user,
            },
        ],
        temperature: None,
        max_tokens: Some(token_budget(&brief)),
        reasoning_effort: reasoning_effort_for(thinks),
    };

    let stream = client.stream_chat(request);
    let handle = async_runtime::spawn(pump_lyrics(app, stream));
    current
        .lock()
        .expect("lyrics state poisoned")
        .replace(handle.inner().abort_handle());
    Ok(())
}

/// Abort the in-flight generation, if any.
#[tauri::command]
pub async fn lyrics_cancel(state: State<'_, LyricsState>) -> Result<(), String> {
    if let Some(handle) = state.current.lock().expect("lyrics state poisoned").take() {
        handle.abort();
    }
    Ok(())
}

/// Drive the stream to its end, emitting deltas and one terminal event.
async fn pump_lyrics(
    app: AppHandle,
    stream: impl futures_util::Stream<Item = Result<ChatDelta, LlmError>> + Unpin + Send + 'static,
) {
    let outcome = stream_lyrics(stream, |event| match event {
        LyricEvent::Delta(text) => {
            let _ = app.emit("lyrics://delta", LyricDelta { text });
        }
        LyricEvent::Thinking(text) => {
            let _ = app.emit("lyrics://thinking", LyricThinking { text });
        }
    })
    .await;

    match outcome {
        LyricsOutcome::Done {
            finish_reason,
            usage,
        } => {
            let _ = app.emit(
                "lyrics://done",
                LyricDone {
                    finish_reason,
                    usage,
                },
            );
        }
        LyricsOutcome::Failed { error } => {
            let _ = app.emit("lyrics://failed", LyricFailed { error });
        }
    }
}

/// One non-terminal thing to re-emit.
#[derive(Debug, PartialEq)]
enum LyricEvent {
    Delta(String),
    Thinking(String),
}

/// The terminal result of a lyric stream.
#[derive(Debug)]
enum LyricsOutcome {
    Done {
        finish_reason: Option<String>,
        usage: Option<TokenUsage>,
    },
    Failed {
        error: String,
    },
}

/// Consume the stream, emitting each content/thinking delta and returning the
/// terminal outcome.
///
/// A refusal is a failure: the model declined to write, and its text must not
/// reach the document. A stream error is a failure. Anything else that reaches
/// the end of the stream is `Done`, finish reason intact.
async fn stream_lyrics<S>(mut stream: S, mut emit: impl FnMut(LyricEvent)) -> LyricsOutcome
where
    S: futures_util::Stream<Item = Result<ChatDelta, LlmError>> + Unpin,
{
    let mut finish_reason = None;
    let mut usage = None;
    let mut refusal: Option<String> = None;

    while let Some(delta) = stream.next().await {
        match delta {
            Ok(ChatDelta::Content(text)) => emit(LyricEvent::Delta(text)),
            Ok(ChatDelta::Reasoning(text)) => emit(LyricEvent::Thinking(text)),
            Ok(ChatDelta::Refusal(text)) => refusal = Some(text),
            Ok(ChatDelta::Finished { reason }) => finish_reason = reason,
            Ok(ChatDelta::Usage(usage_delta)) => usage = Some(usage_delta),
            Err(e) => {
                return LyricsOutcome::Failed {
                    error: e.to_string(),
                }
            }
        }
    }

    match refusal {
        Some(error) => LyricsOutcome::Failed { error },
        None => LyricsOutcome::Done {
            finish_reason,
            usage,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn canned(
        items: Vec<Result<ChatDelta, LlmError>>,
    ) -> impl futures_util::Stream<Item = Result<ChatDelta, LlmError>> + Unpin + Send + 'static
    {
        stream::iter(items).boxed()
    }

    fn content(text: &str) -> Result<ChatDelta, LlmError> {
        Ok(ChatDelta::Content(text.to_string()))
    }

    fn usage() -> Result<ChatDelta, LlmError> {
        Ok(ChatDelta::Usage(TokenUsage {
            prompt_tokens: 1,
            completion_tokens: 2,
            total_tokens: 3,
        }))
    }

    /// Protects: the policy itself. `reasoning_effort` is sent only when the
    /// model is known to think; unknown (`None`) is treated as "do not send",
    /// because the field is verified against Ollama and nothing else.
    #[test]
    fn test_reasoning_effort_is_sent_only_when_the_model_known_to_think() {
        assert_eq!(reasoning_effort_for(Some(true)).as_deref(), Some("none"));
        assert_eq!(reasoning_effort_for(Some(false)), None);
        assert_eq!(reasoning_effort_for(None), None);
    }

    /// Protects: content and reasoning are split into the right events, in
    /// order, and the terminal `Done` carries both the finish reason and the
    /// usage the model reported.
    #[tokio::test]
    async fn test_stream_emits_deltas_and_returns_done_with_reason_and_usage() {
        let mut emitted = Vec::new();
        let outcome = stream_lyrics(
            canned(vec![
                Ok(ChatDelta::Reasoning("weighing it up".to_string())),
                content("first line"),
                content(" second line"),
                Ok(ChatDelta::Finished {
                    reason: Some("length".to_string()),
                }),
                usage(),
            ]),
            |event| emitted.push(event),
        )
        .await;

        match outcome {
            LyricsOutcome::Done {
                finish_reason,
                usage,
            } => {
                assert_eq!(finish_reason.as_deref(), Some("length"));
                assert_eq!(usage.expect("usage").completion_tokens, 2);
            }
            other => panic!("expected Done, got {other:?}"),
        }
        assert_eq!(
            emitted,
            vec![
                LyricEvent::Thinking("weighing it up".to_string()),
                LyricEvent::Delta("first line".to_string()),
                LyricEvent::Delta(" second line".to_string()),
            ]
        );
    }

    /// Protects: truncation reaches the frontend intact. A `length` finish
    /// reason is the signal the UI uses to show a retry-with-more-budget
    /// banner, so swallowing it as an error would hide a fixable outcome.
    #[test]
    fn test_done_serialises_the_finish_reason_intact() {
        let json = serde_json::to_value(LyricDone {
            finish_reason: Some("length".to_string()),
            usage: Some(TokenUsage {
                prompt_tokens: 20,
                completion_tokens: 85,
                total_tokens: 105,
            }),
        })
        .expect("serialises");
        assert_eq!(json["finish_reason"], serde_json::json!("length"));
        assert_eq!(json["usage"]["completion_tokens"], serde_json::json!(85));
    }

    /// Protects: a stream error is a failed generation, not a silent end. The
    /// frontend must know a lyric did not finish.
    #[tokio::test]
    async fn test_stream_error_maps_to_failed() {
        let outcome = stream_lyrics(
            canned(vec![Err(LlmError::Decode("bad frame".to_string()))]),
            |_| {},
        )
        .await;
        assert!(matches!(
            outcome,
            LyricsOutcome::Failed { error } if error.contains("bad frame")
        ));
    }

    /// Protects: a refusal is a failure, and the model's wording is what the
    /// user sees. Refusal text must not reach the document, and a silent empty
    /// result would look like the model never answered.
    #[tokio::test]
    async fn test_stream_refusal_maps_to_failed() {
        let outcome = stream_lyrics(
            canned(vec![Ok(ChatDelta::Refusal(
                "I can't write that".to_string(),
            ))]),
            |_| {},
        )
        .await;
        assert!(matches!(
            outcome,
            LyricsOutcome::Failed { error } if error == "I can't write that"
        ));
    }
}
