//! The wizard's LLM step: what the endpoint offers, and whether it answers.
//!
//! The backend classifies; the frontend renders. As with the other steps, no
//! service problem returns `Err`.
//!
//! **The OpenAI-compatible model list cannot answer the two questions that
//! matter** (LLM-SURFACE 11.1, 11.2). It returns ids and nothing else, so an
//! embedding model is indistinguishable from a chat model, and a model running
//! on someone else's servers is indistinguishable from a local one. On the
//! verification machine that was 2 unusable models and 8 remote ones out of 13.
//! Ollama's native API answers both, so this module enriches from it when the
//! endpoint turns out to be Ollama -- and when it does not, it reports the
//! capabilities as **unknown**. Unknown is never rendered as "local" or as
//! "can chat": claiming a model is private when nobody checked is the one
//! mistake this step must not make.
//!
//! **No secret crosses this boundary.** The key is read from the keychain to
//! sign the request and is never returned, logged, or included in an error.

use create_core::suggestions::{LyricLlmSuggestion, LyricLlmSuggestions};
use futures_util::StreamExt;
use library::{secrets, SecretKey};
use llm_bridge::{ChatDelta, ChatMessage, ChatRequest, LlmError, OllamaNative, OpenAiCompat, Role};
use serde::Serialize;
use tauri::State;

use crate::DataDir;

/// How many tokens the test call may spend.
///
/// Deliberately generous. A reasoning model spends the budget on
/// chain-of-thought before it writes a word of answer: 20 tokens produced empty
/// content and `finish_reason: length` on a healthy endpoint, where 400 gave
/// `"ok"` after 108 characters of reasoning (LLM-SURFACE 2, 11.4).
const TEST_CALL_MAX_TOKENS: u32 = 512;

/// One model the endpoint offers.
#[derive(Debug, Clone, Serialize)]
pub struct LlmModelRow {
    pub id: String,
    /// `None` means nobody could check, which the UI shows as unknown. It is
    /// **not** `false`: the OpenAI-compatible list simply does not say.
    pub can_chat: Option<bool>,
    /// Whether the model emits chain-of-thought first. Explains a pause the
    /// user would otherwise read as a hang.
    pub thinks: Option<bool>,
    /// Whether generating sends the prompt to another party. A privacy fact.
    pub is_remote: Option<bool>,
    /// Who it is sent to, when it is sent anywhere.
    pub remote_host: Option<String>,
    /// On-disk size, absent for remote models where the number is a stub.
    pub size_bytes: Option<u64>,
    /// Set when this model matches a suggestion in `data/lyric-llms.json`.
    pub suggested: Option<Suggested>,
}

/// The "recommended for lyrics" chip.
#[derive(Debug, Clone, Serialize)]
pub struct Suggested {
    pub label: String,
    pub why: Option<String>,
    pub vram_hint: Option<String>,
}

impl Suggested {
    fn from(suggestion: &LyricLlmSuggestion) -> Self {
        Suggested {
            label: suggestion.label.clone(),
            why: suggestion.why.clone(),
            vram_hint: suggestion.vram_hint.clone(),
        }
    }
}

/// A suggested model with nothing installed to satisfy it.
///
/// Becomes help text carrying the user's own pull command. **This app never
/// pulls an LLM** -- that is the user's disk and bandwidth (docs/MODELS.md).
#[derive(Debug, Clone, Serialize)]
pub struct MissingSuggestion {
    pub label: String,
    pub why: Option<String>,
    pub vram_hint: Option<String>,
    pub pull_command: Option<String>,
}

/// What the LLM step shows.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LlmStatus {
    /// No endpoint configured yet. The first-run state, not a failure.
    NotConfigured,
    /// The endpoint could not be reached or did not answer usefully.
    Unreachable {
        detail: String,
        /// A specific next step when the failure is recognisable. A base URL
        /// missing `/v1` answers 404 with a plain-text body, which is the
        /// likeliest misconfiguration and deserves better than a bare 404
        /// (LLM-SURFACE 11.3).
        hint: Option<String>,
    },
    /// The endpoint answered with its catalogue.
    Ready {
        models: Vec<LlmModelRow>,
        /// True when capabilities came from Ollama's native API. False means
        /// every `can_chat` / `is_remote` is `None`, and the UI must say the
        /// list could not be checked rather than implying it is safe.
        enriched: bool,
        /// Suggestions with nothing installed, for help text.
        missing_suggestions: Vec<MissingSuggestion>,
        /// Which model to select, honouring what is already configured.
        preselect: Option<String>,
        /// Whether an API key is stored. **The value never crosses this
        /// boundary**, and this is the only keychain read the step performs --
        /// the frontend must not call `has_secret` itself, because reading is
        /// what answers it and on macOS that can raise a prompt (T-004).
        has_key: bool,
    },
}

/// The outcome of a test call.
#[derive(Debug, Clone, Serialize)]
pub struct LlmTestResult {
    pub ok: bool,
    /// What the model actually said, trimmed. May be empty on a model that
    /// spent its budget thinking, which is still a success.
    pub content: String,
    /// Whether chain-of-thought arrived. Proves the reasoning split works
    /// against this endpoint, which is what the lyric flow depends on.
    pub saw_reasoning: bool,
    pub detail: Option<String>,
}

/// Report what the configured endpoint offers.
///
/// **Never returns `Err` for a service problem.** An unreachable endpoint is a
/// state with a next step; the `Err` arm is reserved for this app failing to
/// read its own shipped data.
#[tauri::command]
pub async fn llm_probe(
    data_dir: State<'_, DataDir>,
    base_url: Option<String>,
    configured_model: Option<String>,
) -> Result<LlmStatus, String> {
    let Some(base_url) = base_url.filter(|u| !u.trim().is_empty()) else {
        return Ok(LlmStatus::NotConfigured);
    };
    let suggestions = library::suggestions::load(&data_dir.0).suggestions;

    // One keychain read for the whole step. `has_secret` would read it too, so
    // fetching it here is strictly fewer touches than asking twice.
    let key = secrets::get_secret(SecretKey::LlmApiKey).ok();
    let has_key = key.is_some();

    let client = match OpenAiCompat::new(base_url.clone(), key) {
        Ok(client) => client,
        Err(e) => return Ok(unreachable(&base_url, &e)),
    };
    let ids = match client.list_models().await {
        Ok(ids) => ids,
        Err(e) => return Ok(unreachable(&base_url, &e)),
    };

    let native = enrich(&base_url).await;
    let enriched = native.is_some();
    let models: Vec<LlmModelRow> = ids
        .iter()
        .map(|id| row(id.as_str(), native.as_deref(), &suggestions))
        .collect();

    // Only models that can chat are offered, and a model whose capabilities
    // are unknown stays on the list -- hiding it would strand a user on a
    // non-Ollama endpoint with an empty picker.
    let selectable: Vec<&str> = models
        .iter()
        .filter(|m| m.can_chat.unwrap_or(true))
        .map(|m| m.id.as_str())
        .collect();
    let preselect = suggestions
        .preselect(&selectable, configured_model.as_deref())
        .map(str::to_string);

    let missing_suggestions = suggestions
        .missing(&selectable)
        .into_iter()
        .map(|s| MissingSuggestion {
            label: s.label.clone(),
            why: s.why.clone(),
            vram_hint: s.vram_hint.clone(),
            pull_command: s.pull_command.clone(),
        })
        .collect();

    Ok(LlmStatus::Ready {
        models,
        enriched,
        missing_suggestions,
        preselect,
        has_key,
    })
}

/// Ask the endpoint one trivial question and report whether it answered.
///
/// **Success is a well-formed response, not non-empty content.** A reasoning
/// model can spend the whole budget on chain-of-thought; treating that as a
/// failure reports a broken endpoint to a user whose setup is fine
/// (LLM-SURFACE 11.4).
#[tauri::command]
pub async fn llm_test(base_url: String, model: String) -> Result<LlmTestResult, String> {
    let key = secrets::get_secret(SecretKey::LlmApiKey).ok();
    let client = match OpenAiCompat::new(base_url.clone(), key) {
        Ok(client) => client,
        Err(e) => return Ok(failed_test(&e)),
    };

    let request = ChatRequest {
        model,
        messages: vec![ChatMessage {
            role: Role::User,
            content: "Reply with exactly: ok".to_string(),
        }],
        temperature: None,
        max_tokens: Some(TEST_CALL_MAX_TOKENS),
    };

    let mut stream = client.stream_chat(request);
    let mut content = String::new();
    let mut saw_reasoning = false;
    let mut finished = false;
    while let Some(delta) = stream.next().await {
        match delta {
            Ok(ChatDelta::Content(text)) => content.push_str(&text),
            Ok(ChatDelta::Reasoning(_)) => saw_reasoning = true,
            Ok(ChatDelta::Finished { .. }) => finished = true,
            Ok(_) => {}
            Err(e) => return Ok(failed_test(&e)),
        }
    }

    let answered = finished || !content.is_empty() || saw_reasoning;
    Ok(LlmTestResult {
        ok: answered,
        content: content.trim().to_string(),
        saw_reasoning,
        detail: if answered {
            None
        } else {
            Some("the endpoint closed the stream without answering".to_string())
        },
    })
}

/// Build one row, enriched when the native catalogue is available.
fn row(
    id: &str,
    native: Option<&[llm_bridge::OllamaModel]>,
    suggestions: &LyricLlmSuggestions,
) -> LlmModelRow {
    let found = native.and_then(|models| models.iter().find(|m| m.name == id));
    LlmModelRow {
        id: id.to_string(),
        can_chat: found.map(|m| m.can_chat()),
        thinks: found.map(|m| m.thinks()),
        is_remote: found.map(|m| m.is_remote()),
        remote_host: found.and_then(|m| m.remote_host.clone()),
        size_bytes: found.and_then(|m| m.disk_size()).filter(|size| *size > 0),
        suggested: suggestions.for_model(id).map(Suggested::from),
    }
}

/// Fetch Ollama's native catalogue, when this endpoint is Ollama.
///
/// `from_openai_base_url` only strips a `/v1` suffix; LM Studio and vLLM also
/// end in `/v1` and do not answer `/api/tags`, so the version call is the real
/// test. A failure here is not an error -- it means "not Ollama", and the step
/// carries on with capabilities unknown.
async fn enrich(base_url: &str) -> Option<Vec<llm_bridge::OllamaModel>> {
    let root = OllamaNative::from_openai_base_url(base_url)?;
    let native = OllamaNative::new(root).ok()?;
    native.version().await.ok()?;
    native.list_models().await.ok()
}

/// Classify a failed probe, with a next step where the failure is recognisable.
fn unreachable(base_url: &str, error: &LlmError) -> LlmStatus {
    let detail = error.to_string();
    let hint = if is_missing_v1(base_url, &detail) {
        Some(format!(
            "Try {}/v1 -- most endpoints serve their OpenAI-compatible API under /v1.",
            base_url.trim_end_matches('/')
        ))
    } else {
        None
    };
    LlmStatus::Unreachable { detail, hint }
}

/// Whether this looks like a base URL missing its `/v1` suffix.
///
/// Verified live: `http://127.0.0.1:11434/models` answers **404 with the
/// plain-text body `404 page not found`**, not JSON. That is the likeliest
/// misconfiguration a user makes, and a bare "404" tells them nothing.
fn is_missing_v1(base_url: &str, detail: &str) -> bool {
    !base_url.trim_end_matches('/').ends_with("/v1") && detail.contains("404")
}

fn failed_test(error: &LlmError) -> LlmTestResult {
    LlmTestResult {
        ok: false,
        content: String::new(),
        saw_reasoning: false,
        detail: Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm_bridge::OllamaModel;

    fn suggestions() -> LyricLlmSuggestions {
        serde_json::from_str(include_str!("../../data/lyric-llms.json")).expect("decodes")
    }

    /// The live catalogue, trimmed to the rows that matter, captured
    /// 2026-08-25 from Ollama 0.32.15.
    fn native() -> Vec<OllamaModel> {
        serde_json::from_value(serde_json::json!([
            { "name": "all-minilm:latest", "size": 45960996, "capabilities": ["embedding"] },
            { "name": "gemma4:12b-32k", "size": 7194619904u64,
              "capabilities": ["completion", "thinking", "tools", "vision"] },
            { "name": "kimi-k3:cloud", "size": 308, "remote_host": "https://ollama.com",
              "capabilities": ["completion", "thinking", "tools", "vision"] },
            { "name": "mistral-large-3:675b-cloud", "size": 319,
              "remote_host": "https://ollama.com",
              "capabilities": ["completion", "tools", "vision"] }
        ]))
        .expect("captured rows decode")
    }

    /// Protects: an embedding model is marked unusable. `/v1/models` lists
    /// `all-minilm` indistinguishably from a chat model, so without enrichment
    /// the wizard offers a model that cannot answer at all and the failure
    /// surfaces later, at lyric time, far from this screen.
    #[test]
    fn test_an_embedding_model_is_marked_as_unable_to_chat() {
        let native = native();
        let embedding = row("all-minilm:latest", Some(&native), &suggestions());
        assert_eq!(embedding.can_chat, Some(false));

        let chat = row("gemma4:12b-32k", Some(&native), &suggestions());
        assert_eq!(chat.can_chat, Some(true));
    }

    /// Protects: the privacy fact reaches the row. Eight of the thirteen models
    /// on the verification machine run on someone else's servers, and
    /// `/v1/models` gives no way to tell -- the `:cloud` suffix is a naming
    /// convention, not a contract. The user's unreleased lyrics leaving the
    /// machine is a disclosure, not a detail.
    #[test]
    fn test_a_remote_model_carries_its_host() {
        let native = native();
        let remote = row("kimi-k3:cloud", Some(&native), &suggestions());
        assert_eq!(remote.is_remote, Some(true));
        assert_eq!(remote.remote_host.as_deref(), Some("https://ollama.com"));
        assert_eq!(remote.size_bytes, None, "a stub manifest is not a size");

        let local = row("gemma4:12b-32k", Some(&native), &suggestions());
        assert_eq!(local.is_remote, Some(false));
        assert_eq!(local.remote_host, None);
        assert_eq!(local.size_bytes, Some(7_194_619_904));
    }

    /// Protects: **unknown is not false.** Against a non-Ollama endpoint
    /// nothing can be checked, and reporting `is_remote: false` would tell the
    /// user their lyrics stay on the machine when nobody verified that. Every
    /// capability must be `None`.
    #[test]
    fn test_an_unenriched_row_reports_unknown_not_false() {
        let unchecked = row("some-model", None, &suggestions());
        assert_eq!(unchecked.can_chat, None);
        assert_eq!(unchecked.thinks, None);
        assert_eq!(unchecked.is_remote, None);
        assert_eq!(unchecked.remote_host, None);
        assert_eq!(unchecked.size_bytes, None);
    }

    /// Protects: the recommended chip lands on the installed variant. The
    /// suggestion is `gemma4:12b` and the installed tag is `gemma4:12b-32k`.
    #[test]
    fn test_a_suggested_variant_gets_its_chip() {
        let native = native();
        let suggested = row("gemma4:12b-32k", Some(&native), &suggestions());
        let chip = suggested.suggested.expect("12B variant is suggested");
        assert_eq!(chip.label, "Gemma 4 12B");
        assert!(chip.why.is_some());

        assert!(row("qwen3.5:9b", Some(&native), &suggestions())
            .suggested
            .is_none());
    }

    /// Protects: the thinking flag, which explains a pause the user would
    /// otherwise read as a hang. Not every model has it -- mistral-large-3
    /// does not, on the same endpoint.
    #[test]
    fn test_the_thinking_flag_distinguishes_models_on_one_endpoint() {
        let native = native();
        assert_eq!(
            row("kimi-k3:cloud", Some(&native), &suggestions()).thinks,
            Some(true)
        );
        assert_eq!(
            row("mistral-large-3:675b-cloud", Some(&native), &suggestions()).thinks,
            Some(false)
        );
    }

    /// Protects: the base-URL mistake gets a real next step. A missing `/v1`
    /// answers 404 with a plain-text body, and relaying a bare "404" leaves the
    /// user guessing at the one thing most likely to be wrong.
    #[test]
    fn test_a_missing_v1_suffix_is_recognised() {
        assert!(is_missing_v1(
            "http://127.0.0.1:11434",
            "HTTP 404: 404 page not found"
        ));
        assert!(is_missing_v1(
            "http://127.0.0.1:11434/",
            "404 page not found"
        ));
        assert!(
            !is_missing_v1("http://127.0.0.1:11434/v1", "HTTP 404: no such model"),
            "already has /v1, so 404 means something else"
        );
        assert!(
            !is_missing_v1("http://127.0.0.1:11434", "connection refused"),
            "a refused connection is not a path problem"
        );
    }

    /// The enrichment path against a **live** Ollama.
    ///
    /// Excluded from CI, which has none. Run with
    /// `cargo test -p app -- --ignored --nocapture`.
    ///
    /// It asserts the shape of what enrichment buys, not this machine's exact
    /// catalogue: that every id the OpenAI-compatible list returns is also
    /// found natively, that at least one model is rejected as unable to chat,
    /// and that any remote model names its host. Those three are the whole
    /// argument for `ollama_native` existing.
    #[tokio::test]
    #[ignore = "needs a running Ollama"]
    async fn test_enrichment_against_a_live_ollama() {
        const BASE: &str = "http://127.0.0.1:11434/v1";

        let client = OpenAiCompat::new(BASE, None).expect("client");
        let ids = client
            .list_models()
            .await
            .expect("the endpoint lists models");
        let native = enrich(BASE).await.expect("this endpoint is Ollama");
        println!("{} ids, {} enriched", ids.len(), native.len());

        let suggestions = suggestions();
        let rows: Vec<LlmModelRow> = ids
            .iter()
            .map(|id| row(id.as_str(), Some(&native), &suggestions))
            .collect();

        assert!(
            rows.iter().all(|r| r.can_chat.is_some()),
            "every id the OpenAI list returned must be found in the native catalogue"
        );
        assert!(
            rows.iter().any(|r| r.can_chat == Some(false)),
            "at least one embedding model, which /v1/models cannot distinguish"
        );
        for remote in rows.iter().filter(|r| r.is_remote == Some(true)) {
            assert!(
                remote.remote_host.is_some(),
                "{} is remote but names no host, so no disclosure could be shown",
                remote.id
            );
            assert_eq!(remote.size_bytes, None, "a stub manifest is not a size");
        }

        let unusable = rows.iter().filter(|r| r.can_chat == Some(false)).count();
        let remote = rows.iter().filter(|r| r.is_remote == Some(true)).count();
        println!("enrichment removed {unusable} unusable model(s)");
        println!("enrichment disclosed {remote} remote model(s) that /v1/models hides");
    }
    /// The test call against a **live** endpoint, on a model that thinks.
    ///
    /// Excluded from CI. Run with `cargo test -p app -- --ignored --nocapture`.
    ///
    /// This is the one that catches the trap in LLM-SURFACE 11.4: with a small
    /// budget a reasoning model returns **empty content** on a healthy
    /// endpoint, and a test that asserted non-empty content would report a
    /// broken setup to a user whose setup is fine. It asserts `ok`, not
    /// `content`, and prints both so a regression in the budget is visible.
    #[tokio::test]
    #[ignore = "needs a running Ollama"]
    async fn test_the_test_call_succeeds_on_a_thinking_model() {
        let result = llm_test(
            "http://127.0.0.1:11434/v1".to_string(),
            "gemma4:12b-32k".to_string(),
        )
        .await
        .expect("the command itself does not fail");

        println!(
            "ok={} saw_reasoning={} content={:?} detail={:?}",
            result.ok, result.saw_reasoning, result.content, result.detail
        );
        assert!(result.ok, "a healthy endpoint must read as success");
        assert!(
            result.saw_reasoning,
            "gemma4:12b thinks, so the reasoning split must fire -- if this stops              being true the budget in TEST_CALL_MAX_TOKENS is no longer being exercised"
        );
    }
    /// Protects: the status crosses the Tauri boundary as a tagged union with
    /// snake_case tags, and **carries no secret**. `has_key` is a boolean; the
    /// key itself must never appear in the payload.
    #[test]
    fn test_status_serialises_as_a_tagged_union_without_the_key() {
        let json = serde_json::to_value(LlmStatus::Ready {
            models: vec![],
            enriched: false,
            missing_suggestions: vec![],
            preselect: None,
            has_key: true,
        })
        .expect("serialises");
        assert_eq!(json["state"], serde_json::json!("ready"));
        assert_eq!(json["has_key"], serde_json::json!(true));

        let text = json.to_string();
        assert!(
            !text.contains("api_key"),
            "no key field crosses the boundary"
        );

        let json = serde_json::to_value(LlmStatus::NotConfigured).expect("serialises");
        assert_eq!(json["state"], serde_json::json!("not_configured"));
    }
}
