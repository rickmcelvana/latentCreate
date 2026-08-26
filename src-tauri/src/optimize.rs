//! The consent-gated prompt optimizer, wired to the frontend.
//!
//! One command, one round trip: assemble the brief, ask the same endpoint that
//! writes the lyrics to sharpen it, and hand **both** texts back so the
//! frontend can show them side by side. Nothing here applies the rewrite -- the
//! optimized text becomes the prompt only when the user accepts it, and only
//! then does `lyrics_generate` receive it (ARCHITECTURE 6).
//!
//! Streaming is deliberately not exposed. The diff needs the whole answer
//! before it can be read, so a partial optimizer prompt on screen would be
//! movement without information -- the opposite of the lyric stream, where the
//! draft is useful as it arrives.

use create_core::lyrics::optimize::{
    clean_optimized, optimizer_system_prompt, OPTIMIZER_MAX_TOKENS,
};
use create_core::lyrics::{assemble_user_message, LyricBrief};
use futures_util::StreamExt;
use llm_bridge::{ChatDelta, ChatMessage, ChatRequest, LlmError, OpenAiCompat, Role};
use serde::Serialize;
use tauri::State;

use crate::lyrics::{configured_llm, load_profile, model_thinks, reasoning_effort_for};
use crate::{ConfigDir, ProfilesDir};

/// One optimizer round trip, as the diff view needs it.
///
/// Both texts cross the boundary because the diff is against the brief **as
/// assembled**, not against the form fields: the user is accepting a prompt,
/// so the prompt they would otherwise have sent is the thing to compare with.
#[derive(Debug, Clone, Serialize)]
pub struct PromptOptimization {
    /// The brief as this app would have sent it.
    pub original: String,
    /// The model's rewrite, fences trimmed, otherwise untouched.
    pub optimized: String,
    /// True when the model stopped on `finish_reason: "length"`, so the rewrite
    /// is likely cut off. Reported rather than hidden: a truncated brief is
    /// still reviewable, and the user is the one deciding.
    pub truncated: bool,
}

/// Ask the configured LLM to sharpen the brief, returning both texts to diff.
///
/// Errors are for "there is nothing to review": no endpoint, no profile, a
/// refusal, a broken stream, or an answer that cleaned down to nothing. A
/// service that is simply slow or a rewrite that is merely bad are both normal
/// outcomes the user resolves with Revert.
#[tauri::command]
pub async fn lyrics_optimize(
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    brief: LyricBrief,
    profile_id: String,
) -> Result<PromptOptimization, String> {
    let (base_url, model) = configured_llm(&config_dir.0)?;
    let profile = load_profile(&profiles_dir.0, &config_dir.0, &profile_id)?;

    let original = assemble_user_message(&brief);
    let system = optimizer_system_prompt(&profile);

    let key = library::secrets::get_secret(library::SecretKey::LlmApiKey).ok();
    let client = OpenAiCompat::new(base_url.clone(), key).map_err(|e| e.to_string())?;

    let thinks = model_thinks(&base_url, &model).await;
    let request = ChatRequest {
        model,
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: system,
            },
            ChatMessage {
                role: Role::User,
                content: original.clone(),
            },
        ],
        temperature: None,
        max_tokens: Some(OPTIMIZER_MAX_TOKENS),
        reasoning_effort: reasoning_effort_for(thinks),
    };

    let collected = collect_answer(client.stream_chat(request)).await?;
    let optimized = clean_optimized(&collected.text);
    if optimized.is_empty() {
        return Err("the model returned nothing to review".to_string());
    }

    Ok(PromptOptimization {
        original,
        optimized,
        truncated: collected.truncated,
    })
}

/// One collected answer: the content, and whether the model ran out of room.
#[derive(Debug, PartialEq)]
struct Collected {
    text: String,
    truncated: bool,
}

/// Drain a chat stream into one answer.
///
/// **Reasoning is dropped, not collected.** The optimizer's output is a text
/// the user is about to accept as their own prompt, and chain-of-thought
/// reaching it would be the same class of mistake as reasoning reaching the
/// lyric document (LLM-SURFACE 12). A refusal is an error for the same reason:
/// the model's apology must not become the prompt.
async fn collect_answer<S>(mut stream: S) -> Result<Collected, String>
where
    S: futures_util::Stream<Item = Result<ChatDelta, LlmError>> + Unpin,
{
    let mut text = String::new();
    let mut truncated = false;

    while let Some(delta) = stream.next().await {
        match delta {
            Ok(ChatDelta::Content(chunk)) => text.push_str(&chunk),
            Ok(ChatDelta::Reasoning(_)) => {}
            Ok(ChatDelta::Refusal(reason)) => return Err(reason),
            Ok(ChatDelta::Finished { reason }) => {
                truncated = reason.as_deref() == Some("length");
            }
            Ok(ChatDelta::Usage(_)) => {}
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok(Collected { text, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn canned(
        items: Vec<Result<ChatDelta, LlmError>>,
    ) -> impl futures_util::Stream<Item = Result<ChatDelta, LlmError>> + Unpin {
        stream::iter(items).boxed()
    }

    /// Protects: the answer is the content and only the content. Reasoning
    /// deltas are the majority of a thinking model's stream, and one of them
    /// reaching the text would put chain-of-thought into the prompt the user is
    /// asked to accept.
    #[tokio::test]
    async fn test_collect_answer_joins_content_and_drops_reasoning() {
        let collected = collect_answer(canned(vec![
            Ok(ChatDelta::Reasoning("the theme is vague".to_string())),
            Ok(ChatDelta::Content("Theme: a night drive".to_string())),
            Ok(ChatDelta::Content(" out of a coastal town".to_string())),
            Ok(ChatDelta::Finished {
                reason: Some("stop".to_string()),
            }),
        ]))
        .await
        .expect("a healthy stream collects");

        assert_eq!(
            collected,
            Collected {
                text: "Theme: a night drive out of a coastal town".to_string(),
                truncated: false,
            }
        );
    }

    /// Protects: truncation is carried, not swallowed. A rewrite cut off
    /// mid-brief is missing the settings lines the prompt told the model to
    /// reproduce, and the user should be told before they accept it.
    #[tokio::test]
    async fn test_collect_answer_reports_a_length_stop_as_truncated() {
        let collected = collect_answer(canned(vec![
            Ok(ChatDelta::Content("Theme: a night".to_string())),
            Ok(ChatDelta::Finished {
                reason: Some("length".to_string()),
            }),
        ]))
        .await
        .expect("a truncated stream still collects");
        assert!(collected.truncated);
    }

    /// Protects: a refusal is an error, never an optimized prompt. The model
    /// declining to rewrite must not put its apology in the user's prompt box.
    #[tokio::test]
    async fn test_collect_answer_maps_a_refusal_to_an_error() {
        let error = collect_answer(canned(vec![Ok(ChatDelta::Refusal(
            "I can't help with that".to_string(),
        ))]))
        .await
        .expect_err("a refusal is not an answer");
        assert_eq!(error, "I can't help with that");
    }

    /// Protects: a broken stream fails rather than returning a half prompt.
    #[tokio::test]
    async fn test_collect_answer_maps_a_stream_error_to_an_error() {
        let error = collect_answer(canned(vec![
            Ok(ChatDelta::Content("Theme:".to_string())),
            Err(LlmError::Decode("bad frame".to_string())),
        ]))
        .await
        .expect_err("a broken stream is not an answer");
        assert!(error.contains("bad frame"), "{error}");
    }

    /// Protects: both texts cross the boundary with the names the frontend
    /// mirrors, so the diff has an original to diff against.
    #[test]
    fn test_optimization_serialises_both_texts() {
        let json = serde_json::to_value(PromptOptimization {
            original: "Theme: a night drive".to_string(),
            optimized: "Theme: a night drive down a wet coast road".to_string(),
            truncated: false,
        })
        .expect("serialises");
        assert_eq!(json["original"], serde_json::json!("Theme: a night drive"));
        assert_eq!(
            json["optimized"],
            serde_json::json!("Theme: a night drive down a wet coast road")
        );
        assert_eq!(json["truncated"], serde_json::json!(false));
    }
}
