//! Lyric generation: stream a lyric from the user's LLM and re-emit it as
//! Tauri events.
//!
//! The backend streams; the frontend accumulates. Nothing here writes a lyric
//! document -- that is the lyrics store's job. This module owns the policy from
//! LLM-SURFACE 12.2: `reasoning_effort: "none"` is sent only when the model is
//! known to think, because the field is verified against Ollama and nothing
//! else.

use std::path::Path;
use std::sync::{Arc, Mutex};

use create_core::lyrics::{
    assemble_system_prompt, assemble_user_message, token_budget, LyricBrief,
};
use create_core::profile::ModelProfile;
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
pub(crate) fn reasoning_effort_for(thinks: Option<bool>) -> Option<String> {
    match thinks {
        Some(true) => Some("none".to_string()),
        _ => None,
    }
}

/// Whether the configured model is known to think, from the Ollama enrichment.
///
/// `None` means either the endpoint is not Ollama or the model was not found,
/// both of which leave the field unset -- never guessed.
pub(crate) async fn model_thinks(base_url: &str, model: &str) -> Option<bool> {
    let native = enrich(base_url).await?;
    native.iter().find(|m| m.name == model).map(|m| m.thinks())
}

/// The configured lyric endpoint and model, as `(base_url, model)`.
///
/// Shared with the optimizer, which talks to the same endpoint by definition
/// (ARCHITECTURE 6). Each missing piece names itself, because "no lyric LLM
/// configured" and "no model chosen" send the user to different parts of the
/// wizard.
pub(crate) fn configured_llm(config_dir: &Path) -> Result<(String, String), String> {
    let llm = library::config::load(config_dir)
        .config
        .llm
        .ok_or_else(|| "no lyric LLM configured".to_string())?;
    let base_url = llm
        .base_url
        .ok_or_else(|| "no lyric LLM base URL configured".to_string())?;
    let model = llm
        .model
        .ok_or_else(|| "no lyric model configured".to_string())?;
    Ok((base_url, model))
}

/// The model profile the studio is writing for.
pub(crate) fn load_profile(
    profiles_dir: &Path,
    config_dir: &Path,
    profile_id: &str,
) -> Result<ModelProfile, String> {
    library::profiles::load(profiles_dir, &config_dir.join("profiles"))
        .profiles
        .get(profile_id)
        .map(|loaded| loaded.profile.clone())
        .ok_or_else(|| format!("no profile named {profile_id}"))
}

/// The user message actually sent: the text the user approved, when there is
/// one, else the brief as assembled from the form.
///
/// **The override is the whole point of the optimizer** (ARCHITECTURE 6): what
/// gets sent is what the user accepted, not what the model proposed. A blank
/// override falls back to the assembled brief rather than sending an empty
/// message -- the brief is still the user's own words, and an empty prompt is
/// not something they could have meant to approve.
pub(crate) fn user_message(brief: &LyricBrief, approved: Option<&str>) -> String {
    match approved {
        Some(text) if !text.trim().is_empty() => text.to_string(),
        _ => assemble_user_message(brief),
    }
}

/// Stream a lyric for `brief` against `profile_id`, re-emitting it as
/// `lyrics://delta|thinking|done|failed` events.
///
/// The brief and the profile are the inputs; the LLM is whatever `config.json`
/// has configured. Returns once the generation has been submitted, not when it
/// finishes -- the events carry the progress.
///
/// `prompt_override` is the optimized brief the user accepted, when they
/// accepted one. It replaces the assembled brief and nothing else: the system
/// prompt still comes from the profile, and the token budget still comes from
/// the brief, because neither is user text the optimizer was shown.
#[tauri::command]
pub async fn lyrics_generate(
    app: AppHandle,
    state: State<'_, LyricsState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    brief: LyricBrief,
    profile_id: String,
    prompt_override: Option<String>,
) -> Result<(), String> {
    let current = Arc::clone(&state.current);
    if let Some(previous) = current.lock().expect("lyrics state poisoned").take() {
        previous.abort();
    }

    let (base_url, model) = configured_llm(&config_dir.0)?;
    let profile = load_profile(&profiles_dir.0, &config_dir.0, &profile_id)?;

    let system = assemble_system_prompt(&profile, &brief);
    let user = user_message(&brief, prompt_override.as_deref());

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

    /// **T-211's lyric measurement**, excluded from CI.
    ///
    /// `cargo test -p app -- --ignored lyric_generation --nocapture` with
    /// Ollama running on the default port and a gemma4 model installed.
    ///
    /// Two things only a live run can show, both with prior evidence to check
    /// against:
    /// 1. **A thinking model writes a whole song.** The same brief with no
    ///    `reasoning_effort` returned 85 characters of lyric and 7458 of
    ///    reasoning, `finish_reason: length`, first content delta 44.08 s into
    ///    a 44.65 s stream (LLM-SURFACE 12.1). Time to first content is
    ///    therefore printed: it is the number that moved.
    /// 2. **The lint fires on what the model actually writes.** Stray
    ///    production directions appeared in 10 of 13 captured generations
    ///    (LLM-SURFACE 12.5), so a clean lint on every run is likelier a broken
    ///    scanner than a well-behaved model.
    ///
    /// Asserts the plumbing -- a complete song arrives -- and prints the rest.
    #[tokio::test]
    #[ignore = "T-211 live measurement: requires a local endpoint on 127.0.0.1:11434"]
    async fn test_live_lyric_generation_writes_a_whole_song_and_the_lint_reads_it() {
        use create_core::lyrics::lint::lint_lyrics;
        use create_core::profile::ModelProfile;

        const RUNS: usize = 3;

        let profile_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../profiles/ace-step-1.5-turbo.json");
        let profile: ModelProfile = serde_json::from_str(
            &std::fs::read_to_string(&profile_path).expect("shipped profile is readable"),
        )
        .expect("shipped profile parses");

        let base_url = "http://127.0.0.1:11434/v1";
        let client = OpenAiCompat::new(base_url, None).expect("client");
        let models = client.list_models().await.expect("model list");
        let model = models
            .iter()
            .find(|id| id.starts_with("gemma4:"))
            .cloned()
            .unwrap_or_else(|| models.first().cloned().expect("endpoint offers no models"));
        let thinks = model_thinks(base_url, &model).await;

        let brief = LyricBrief::default();
        let budget = token_budget(&brief);

        println!("\n=== T-211 lyric measurement ===");
        println!("model: {model}  thinks: {thinks:?}  budget: {budget}  runs: {RUNS}");
        println!(
            "baseline to beat (LLM-SURFACE 12.1, no reasoning_effort): 85 chars, first content 44.08s\n"
        );

        for run in 1..=RUNS {
            let request = ChatRequest {
                model: model.clone(),
                messages: vec![
                    ChatMessage {
                        role: Role::System,
                        content: assemble_system_prompt(&profile, &brief),
                    },
                    ChatMessage {
                        role: Role::User,
                        content: assemble_user_message(&brief),
                    },
                ],
                temperature: None,
                max_tokens: Some(budget),
                reasoning_effort: reasoning_effort_for(thinks),
            };

            let started = std::time::Instant::now();
            let mut lyric = String::new();
            let mut reasoning_chars = 0usize;
            let mut first_content: Option<std::time::Duration> = None;

            let outcome = stream_lyrics(client.stream_chat(request), |event| match event {
                LyricEvent::Delta(text) => {
                    if first_content.is_none() {
                        first_content = Some(started.elapsed());
                    }
                    lyric.push_str(&text);
                }
                LyricEvent::Thinking(text) => reasoning_chars += text.chars().count(),
            })
            .await;

            let elapsed = started.elapsed();
            let finish_reason = match outcome {
                LyricsOutcome::Done { finish_reason, .. } => finish_reason,
                LyricsOutcome::Failed { error } => panic!("run {run} failed: {error}"),
            };

            let findings = lint_lyrics(&profile, &brief, &lyric);
            println!(
                "run {run}: {:.1}s  first content {:.2}s  {} chars lyric, {reasoning_chars} chars reasoning  finish={:?}",
                elapsed.as_secs_f32(),
                first_content.map(|d| d.as_secs_f32()).unwrap_or(f32::NAN),
                lyric.chars().count(),
                finish_reason,
            );
            println!("  lint findings ({}): {findings:?}", findings.len());
            if run == 1 {
                println!("--- run 1 lyric ---\n{lyric}\n---");
            }

            assert_ne!(
                finish_reason.as_deref(),
                Some("length"),
                "run {run} was truncated -- the reasoning_effort policy is not reaching the request"
            );
            assert!(
                lyric.chars().count() > 200,
                "run {run} wrote {} chars, which is not a song",
                lyric.chars().count()
            );
        }
        println!("=== end measurement ===\n");
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

    /// Protects: the consent gate. What is sent is what the user approved --
    /// an accepted optimized brief replaces the assembled one verbatim, and
    /// with nothing approved the form's own brief is what goes. A blank
    /// override falls back rather than sending an empty message, which is not
    /// something a user could have meant to accept.
    #[test]
    fn test_user_message_sends_the_approved_text_and_falls_back_to_the_brief() {
        let brief = LyricBrief::default();
        let assembled = assemble_user_message(&brief);

        assert_eq!(
            user_message(&brief, Some("Theme: a sharpened night drive")),
            "Theme: a sharpened night drive"
        );
        assert_eq!(user_message(&brief, None), assembled);
        assert_eq!(user_message(&brief, Some("   \n ")), assembled);
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

    /// **T-302's measurement**, excluded from CI. **Spends the user's API credits.**
    ///
    /// `cargo test -p app -- --ignored reasoning_effort --nocapture`, with a
    /// hosted endpoint configured in `config.json` and its key in the keychain.
    ///
    /// The question (LLM-SURFACE 13.1): `reasoning_effort: "none"` is sent only
    /// where `thinks` is true, and `thinks` exists only where Ollama's native
    /// enrichment answered. Against an endpoint the app cannot enrich the field
    /// is never sent, so the user waits through the whole chain-of-thought --
    /// 44 s before the first content delta, when that was measured on Ollama
    /// (LLM-SURFACE 12.1). Whether sending it anyway would help is unknown, and
    /// the three possibilities imply different rules:
    ///
    /// 1. **honoured** -- the field should go to every endpoint, not just
    ///    enriched ones;
    /// 2. **ignored** -- like Ollama's own `think: false` (12.2), so the
    ///    current rule costs nothing and stays;
    /// 3. **an error** -- the current rule is load-bearing and must stay.
    ///
    /// Prints rather than asserts, except for the premise: this endpoint must
    /// be one the app cannot enrich, or the measurement is of something else.
    /// The API key is read from the keychain and never printed.
    #[tokio::test]
    #[ignore = "T-302 live measurement: hosted endpoint + stored key; spends API credits"]
    async fn test_live_reasoning_effort_on_an_endpoint_the_app_cannot_enrich() {
        use create_core::profile::ModelProfile;
        use library::SecretKey;

        let config_dir = std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA"))
            .join("com.latentbeats.create");
        let (base_url, model) = configured_llm(&config_dir).expect("an endpoint is configured");
        let key = library::secrets::get_secret(SecretKey::LlmApiKey).ok();

        let thinks = model_thinks(&base_url, &model).await;
        assert_eq!(
            thinks, None,
            "this measurement is about endpoints the app CANNOT enrich; \
             {base_url} answered the native API, so it is the wrong subject"
        );

        let profile_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../profiles/ace-step-1.5-turbo.json");
        let profile: ModelProfile = serde_json::from_str(
            &std::fs::read_to_string(&profile_path).expect("shipped profile is readable"),
        )
        .expect("shipped profile parses");
        let brief = LyricBrief::default();
        let budget = token_budget(&brief);

        println!("\n=== T-302: reasoning_effort on an unenrichable endpoint ===");
        println!("endpoint: {base_url}");
        println!(
            "model: {model}  key stored: {}  budget: {budget}",
            key.is_some()
        );
        println!(
            "app-computed reasoning_effort for this model: {:?}",
            reasoning_effort_for(thinks)
        );
        println!("baseline (LLM-SURFACE 12.1, Ollama, no field): first content 44.08s, 85 chars\n");

        for (label, effort) in [
            ("A: no reasoning_effort (what the app sends today)", None),
            ("B: reasoning_effort = none", Some("none".to_string())),
        ] {
            let client = OpenAiCompat::new(base_url.clone(), key.clone()).expect("client");
            let request = ChatRequest {
                model: model.clone(),
                messages: vec![
                    ChatMessage {
                        role: Role::System,
                        content: assemble_system_prompt(&profile, &brief),
                    },
                    ChatMessage {
                        role: Role::User,
                        content: assemble_user_message(&brief),
                    },
                ],
                temperature: None,
                max_tokens: Some(budget),
                reasoning_effort: effort,
            };

            let started = std::time::Instant::now();
            let mut lyric = String::new();
            let mut reasoning_chars = 0usize;
            let mut first_content: Option<std::time::Duration> = None;

            let outcome = stream_lyrics(client.stream_chat(request), |event| match event {
                LyricEvent::Delta(text) => {
                    if first_content.is_none() {
                        first_content = Some(started.elapsed());
                    }
                    lyric.push_str(&text);
                }
                LyricEvent::Thinking(text) => reasoning_chars += text.chars().count(),
            })
            .await;

            println!("--- {label} ---");
            println!("  total: {:.2?}", started.elapsed());
            match first_content {
                Some(at) => println!("  first content delta: {at:.2?}"),
                None => println!("  first content delta: NONE -- no lyric arrived"),
            }
            println!(
                "  lyric chars: {}  reasoning chars: {reasoning_chars}",
                lyric.chars().count()
            );
            match &outcome {
                LyricsOutcome::Done {
                    finish_reason,
                    usage,
                } => println!("  finish_reason: {finish_reason:?}  usage: {usage:?}"),
                LyricsOutcome::Failed { error } => {
                    println!("  FAILED: {error}");
                    println!("  ^ if this is B, the endpoint REJECTS the field and the rule stays");
                }
            }
            println!();
        }

        // Which spelling the stream uses, and whether a usage frame arrives.
        // Raw SSE, because `ChatDelta::Reasoning` deliberately does not say
        // which field it decoded (LLM-SURFACE 3). A tiny prompt: this is about
        // field names, not about lyrics, and it spends the user's credits.
        let body = serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": "Say hi." }],
            "max_tokens": 64,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        let http = reqwest::Client::new();
        let mut req = http
            .post(format!(
                "{}/chat/completions",
                base_url.trim_end_matches('/')
            ))
            .json(&body);
        if let Some(k) = key.as_deref() {
            req = req.bearer_auth(k);
        }
        let raw = req
            .send()
            .await
            .expect("raw request")
            .text()
            .await
            .expect("raw body");

        let mut saw_reasoning = false;
        let mut saw_reasoning_content = false;
        let mut saw_usage = false;
        for line in raw.lines().filter(|l| l.starts_with("data: ")) {
            if line.contains("\"reasoning\"") {
                saw_reasoning = true;
            }
            if line.contains("\"reasoning_content\"") {
                saw_reasoning_content = true;
            }
            if line.contains("\"usage\"") && !line.contains("\"usage\":null") {
                saw_usage = true;
            }
        }
        println!("--- raw SSE field names ---");
        println!(
            "  \"reasoning\": {saw_reasoning}   \"reasoning_content\": {saw_reasoning_content}"
        );
        println!("  usage frame present: {saw_usage}");
        println!(
            "  (both spellings are read either way -- LLM-SURFACE 3 -- this is for the record)"
        );
    }
}
