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
use create_core::profile::ModelProfile;
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

    let key = library::secrets::get_secret(library::SecretKey::LlmApiKey).ok();
    let client = OpenAiCompat::new(base_url.clone(), key).map_err(|e| e.to_string())?;
    let thinks = model_thinks(&base_url, &model).await;

    optimize_with(&client, &profile, &brief, model, thinks).await
}

/// One optimizer round trip against an already-built client.
///
/// Split from the command so T-211's live check exercises **this** path rather
/// than a re-implementation of it: a live test that rebuilds the request proves
/// the endpoint works and nothing about the code that will call it.
async fn optimize_with(
    client: &OpenAiCompat,
    profile: &ModelProfile,
    brief: &LyricBrief,
    model: String,
    thinks: Option<bool>,
) -> Result<PromptOptimization, String> {
    let original = assemble_user_message(brief);
    let system = optimizer_system_prompt(profile);

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

    /// **T-211's optimizer measurement**, excluded from CI.
    ///
    /// `cargo test -p app -- --ignored optimizer --nocapture` with Ollama
    /// running on the default port and a gemma4 model installed.
    ///
    /// The optimizer prompt was written and shipped without ever meeting a
    /// model, which this repo treats as an unverified third-party surface
    /// (LLM-SURFACE 12.5). This run answers the two questions that decide
    /// whether its rules are worth keeping: does the rewrite come back as the
    /// **same labelled lines**, and does it reproduce the **five fixed lines**
    /// word for word.
    ///
    /// **It asserts the plumbing and prints the measurement.** Asserting a rate
    /// nobody has observed yet would encode a guess as a requirement, and the
    /// third question -- whether the rewritten brief actually writes a better
    /// song -- is a judgement no assertion can make. Paste the report into
    /// PROJECT.md's session log; the decision about the prompt follows the
    /// numbers, not the other way round.
    #[tokio::test]
    #[ignore = "T-211 live measurement: requires a local endpoint on 127.0.0.1:11434"]
    async fn test_live_optimizer_returns_a_brief_and_reports_what_it_altered() {
        use create_core::lyrics::optimize::{altered_fixed_lines, label_report, labels_in_order};

        const RUNS: usize = 5;

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
        let thinks = crate::lyrics::model_thinks(base_url, &model).await;

        let brief = LyricBrief::default();
        let expected_labels = labels_in_order(&assemble_user_message(&brief));

        println!("\n=== T-211 optimizer measurement ===");
        println!("model: {model}  thinks: {thinks:?}  runs: {RUNS}");
        println!("expected labels: {expected_labels:?}\n");

        let mut labels_held = 0;
        let mut fixed_held = 0;

        for run in 1..=RUNS {
            let started = std::time::Instant::now();
            let result = optimize_with(&client, &profile, &brief, model.clone(), thinks)
                .await
                .unwrap_or_else(|e| panic!("run {run} failed: {e}"));
            let elapsed = started.elapsed();

            let labels = labels_in_order(&result.optimized);
            let report = label_report(&result.original, &result.optimized);
            let altered = altered_fixed_lines(&result.original, &result.optimized);
            if report.is_clean() {
                labels_held += 1;
            }
            if altered.is_empty() {
                fixed_held += 1;
            }

            println!(
                "run {run}: {:.1}s  truncated={}  brief_intact={}  altered_fixed={:?}",
                elapsed.as_secs_f32(),
                result.truncated,
                report.is_clean(),
                altered
            );
            if !report.is_clean() {
                println!("  {report:?}");
                println!("  labels were: {labels:?}");
            }
            if run == 1 {
                println!("--- run 1 rewrite ---\n{}\n---", result.optimized);
            }

            assert!(
                !result.optimized.trim().is_empty(),
                "run {run} returned nothing to review"
            );
            assert!(
                labels.contains(&"Theme".to_string()),
                "run {run} came back with no Theme line at all: {:?}",
                result.optimized
            );
        }

        println!("\nbrief intact (nothing dropped, invented or shuffled): {labels_held}/{RUNS}");
        println!("fixed lines reproduced: {fixed_held}/{RUNS}");
        println!("=== end measurement ===\n");
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
