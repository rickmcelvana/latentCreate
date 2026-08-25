# T-109a: `ollama_native` — model listing the OpenAI shape cannot express
**Depends:** T-108a/b/c | **Crate/dir:** `crates/llm-bridge`
**Files to create/modify:**
- `crates/llm-bridge/src/ollama.rs` (create)
- `crates/llm-bridge/src/lib.rs` (modify: **one** `pub mod` line and **two** `pub use` blocks — see section 2, which gives the changed lines only, not the whole file)

## Goal
Read Ollama's `/api/tags` so the wizard can tell the user things `/v1/models` cannot say:
which models can chat at all, which think before answering, which run on someone else's
hardware. Read [docs/LLM-SURFACE.md](../docs/LLM-SURFACE.md) section 8 first — every rule
below is a captured fact.

## Spec
Exactly the reference implementation below.

**This module does not chat, and that is the design.** Ollama's `/v1/chat/completions` is
already handled by `OpenAiCompat`; a second path to the same tokens would be two things to
keep correct. `OllamaNative` is an *enrichment layer* over an endpoint that happens to be
Ollama, not a peer provider — which is why it implements no shared trait (ARCHITECTURE 4).

**Three facts come out of `capabilities`, and each one prevents a real failure:**

- **`completion` absent means the model cannot chat.** `nomic-embed-text` reports
  `["embedding"]` and nothing else, yet `/v1/models` lists it exactly like a chat model. An
  embedding model in the lyric picker fails only at generation time, after the user has
  chosen it and typed a brief.
- **`thinking` means the model will emit `delta.reasoning`** (T-108's finding). This is the
  only advance warning that part of the token budget goes to chain-of-thought.
- **`remote_host` is a privacy fact, not a performance one.** Present only on cloud
  entries, it means the user's unreleased lyrics leave their machine — which this app's
  whole premise says they should not, silently. `disk_size()` returns `None` for those,
  because the reported `size` is a stub manifest: a 2.81T model reports **308 bytes**.

**Three decode traps, all captured live:**

- `families` arrives as **JSON `null`**, not absent, on cloud entries. `#[serde(default)]`
  on a plain `Vec<String>` rejects an explicit null, so the *entire model list* would fail
  to decode the moment a user signs in to Ollama's cloud. Hence `Option<Vec<String>>`.
- `parameter_size` is **not normalised**: one install reported `"1t"`, `"1T"`, `"756b"`,
  `"2.81T"`, and `""`. A label to display, never a number to parse or sort on.
- `parent_model` / `format` / `family` are **empty strings** on cloud entries.

**Do not build the list from `/api/show`.** It returns **68 KB per model** — a 667-entry
tensor manifest plus the full licence text — against 5.7 KB for all twelve models from
`/api/tags`. `/api/show` is a details-panel call for one chosen model, and is out of scope
here.

## Fixture
`testdata/llm/ollama-tags.json` is already committed: four live entries chosen to cover
every trap — a local chat model, an embedding-only model, and two cloud models (one with
`families: null`, one with an empty `parameter_size`). **Do not edit or regenerate it**;
the tests assert against its exact values, including `size: 7151003754`.

## Reference implementation
Transcribe verbatim. This compiles, `cargo fmt` is a no-op on it, `cargo clippy
--all-targets -- -D warnings` is clean, its 7 offline tests pass, and its ignored live test
passed against Ollama 0.32.15 on 2026-08-24.

### 1. `crates/llm-bridge/src/ollama.rs` (new file, complete)
```rust
//! `ollama_native`: what Ollama knows that the OpenAI-compatible endpoint
//! cannot say.
//!
//! **This is not a second [`crate::openai::OpenAiCompat`].** It does not chat.
//! Ollama's `/v1/chat/completions` already handles that, and duplicating it
//! here would be two code paths to the same tokens. What the native API adds
//! is *facts about models* the OpenAI shape has nowhere to put: which models
//! can chat at all, which think, which run on someone else's hardware, how
//! much context they hold. All of it from one 5.7 KB call.
//!
//! Shapes verified live against Ollama 0.32.15 on 2026-08-24 --
//! docs/LLM-SURFACE.md 8.

use serde::{Deserialize, Serialize};

use crate::error::http_error;
use crate::LlmError;

/// Capability string marking a model that can answer a chat request.
///
/// Its **absence** is the useful part: an embedding model is listed by
/// `/v1/models` exactly like a chat model, so without this an embedding model
/// sits in the lyric picker and fails at generation time.
pub const CAPABILITY_COMPLETION: &str = "completion";

/// Capability string marking a model that emits chain-of-thought.
///
/// Ties directly to the T-108 finding: a thinking model spends part of its
/// token budget on `delta.reasoning` before any lyrics appear (LLM-SURFACE 2).
/// This is the only way to know *before* generating.
pub const CAPABILITY_THINKING: &str = "thinking";

/// The `details` block of a listed model.
///
/// Everything here is optional in practice: cloud entries carry empty strings
/// for `family`/`format`, one installed model reported an empty
/// `parameter_size`, and `families` arrives as **JSON `null`** rather than
/// being absent -- which `#[serde(default)]` alone does not survive, hence
/// `Option<Vec<String>>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDetails {
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub families: Option<Vec<String>>,
    /// Human-readable parameter count, e.g. `"11.9B"`. **Not normalised by
    /// Ollama**: the same install reported `"1t"`, `"1T"`, `"756b"` and
    /// `"2.81T"`, so this is a label to show, never a number to sort on.
    #[serde(default)]
    pub parameter_size: String,
    /// e.g. `"Q4_0"`, `"MXFP4"`, or empty.
    #[serde(default)]
    pub quantization_level: String,
    /// Maximum context in tokens. Ranges from 2048 to 1048576 on one install,
    /// and is the budget lyrics plus reasoning must fit inside.
    #[serde(default)]
    pub context_length: u64,
}

/// One model as `/api/tags` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OllamaModel {
    /// Tag as the API names it, e.g. `"gemma4:12b-it-qat"`. This is the id to
    /// send as `model` in a chat request.
    pub name: String,
    /// On-disk size in bytes. **Meaningless for remote models** -- a cloud
    /// entry reported 308 bytes, which is its stub manifest, not the weights.
    #[serde(default)]
    pub size: u64,
    /// Present only for models executed on Ollama's servers, e.g.
    /// `"https://ollama.com"`.
    #[serde(default)]
    pub remote_host: Option<String>,
    #[serde(default)]
    pub details: ModelDetails,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl OllamaModel {
    /// Whether this model can answer a chat request at all.
    ///
    /// False for embedding models, which the OpenAI-compatible list offers
    /// indistinguishably from chat models.
    pub fn can_chat(&self) -> bool {
        self.has_capability(CAPABILITY_COMPLETION)
    }

    /// Whether this model emits chain-of-thought before its answer.
    pub fn thinks(&self) -> bool {
        self.has_capability(CAPABILITY_THINKING)
    }

    /// Whether generating with this model sends the prompt to another party.
    ///
    /// **A privacy fact, not a performance one.** latentCreate's premise is
    /// that generation happens on the user's own hardware; a remote model
    /// means their unreleased lyrics leave the machine, so the UI must say so
    /// wherever the model is chosen.
    pub fn is_remote(&self) -> bool {
        self.remote_host.is_some()
    }

    /// Size on disk, or `None` when the number would be misleading.
    pub fn disk_size(&self) -> Option<u64> {
        if self.is_remote() {
            None
        } else {
            Some(self.size)
        }
    }

    fn has_capability(&self, wanted: &str) -> bool {
        self.capabilities.iter().any(|c| c == wanted)
    }
}

/// The `/api/tags` envelope.
#[derive(Debug, Clone, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

/// The `/api/version` envelope.
#[derive(Debug, Clone, Deserialize)]
struct VersionResponse {
    #[serde(default)]
    version: String,
}

/// A client for Ollama's own API.
///
/// Takes the server root (`http://127.0.0.1:11434`), **not** the `/v1` base
/// URL the OpenAI-compatible client uses.
#[derive(Debug, Clone)]
pub struct OllamaNative {
    base_url: String,
    http: reqwest::Client,
}

impl OllamaNative {
    /// Builds a client for an Ollama server root.
    pub fn new(base_url: impl Into<String>) -> Result<Self, LlmError> {
        let base_url = base_url.into().trim().trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| LlmError::Transport {
                base_url: base_url.clone(),
                detail: e.to_string(),
            })?;
        Ok(Self { base_url, http })
    }

    /// The Ollama root that matches an OpenAI-compatible base URL, when that
    /// URL looks like Ollama's.
    ///
    /// The wizard uses this to decide whether the native enrichment is even
    /// worth trying: the user configures one OpenAI-compatible URL, and if it
    /// ends in `/v1` the server root is its parent. Returns `None` otherwise
    /// rather than guessing -- LM Studio and vLLM also end in `/v1` and do not
    /// answer `/api/tags`, so the caller must still confirm with
    /// [`OllamaNative::version`].
    pub fn from_openai_base_url(base_url: &str) -> Option<String> {
        let trimmed = base_url.trim().trim_end_matches('/');
        trimmed
            .strip_suffix("/v1")
            .filter(|root| !root.is_empty())
            .map(|root| root.to_string())
    }

    /// The HTTP client, shared with `pull` so both use one connection pool.
    pub(crate) fn http_client(&self) -> reqwest::Client {
        self.http.clone()
    }

    /// The server root this client talks to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The server's version string, e.g. `"0.32.15"`.
    ///
    /// Doubles as the probe for "is this actually Ollama": any other
    /// OpenAI-compatible server answers this path with a 404.
    pub async fn version(&self) -> Result<String, LlmError> {
        let body = self.get("/api/version").await?;
        let parsed: VersionResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::Decode(format!("version: {e}")))?;
        Ok(parsed.version)
    }

    /// Every model the server has, with its metadata.
    ///
    /// One call, 5.7 KB for twelve models. **Do not build this list from
    /// `/api/show`** -- that returns 68 KB per model, most of it a tensor
    /// manifest, and is for a details panel on one chosen model only.
    pub async fn list_models(&self) -> Result<Vec<OllamaModel>, LlmError> {
        let body = self.get("/api/tags").await?;
        let parsed: TagsResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::Decode(format!("tags: {e}")))?;
        Ok(parsed.models)
    }

    /// Models that can actually answer a lyric request.
    ///
    /// Filtering, not decoration: an embedding model in the picker is a
    /// failure the user only discovers when generation fails.
    pub async fn list_chat_models(&self) -> Result<Vec<OllamaModel>, LlmError> {
        Ok(self
            .list_models()
            .await?
            .into_iter()
            .filter(OllamaModel::can_chat)
            .collect())
    }

    async fn get(&self, path: &str) -> Result<String, LlmError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| LlmError::Transport {
                base_url: self.base_url.clone(),
                detail: e.to_string(),
            })?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(|e| LlmError::Transport {
            base_url: self.base_url.clone(),
            detail: e.to_string(),
        })?;
        if !(200..300).contains(&status) {
            return Err(http_error(&self.base_url, status, &body));
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four entries from a live `/api/tags`, captured 2026-08-24: a local chat
    /// model, an embedding model, a cloud model, and one whose
    /// `parameter_size` came back empty.
    const TAGS: &str = include_str!("../../../testdata/llm/ollama-tags.json");

    fn models() -> Vec<OllamaModel> {
        let parsed: TagsResponse = serde_json::from_str(TAGS).expect("tags decode");
        parsed.models
    }

    /// Protects: the whole reason this module exists. An embedding model is
    /// indistinguishable from a chat model on `/v1/models`, and putting one in
    /// the lyric picker produces a failure the user meets only at generation
    /// time.
    #[test]
    fn test_embedding_models_are_not_chat_models() {
        let models = models();
        let embed = models
            .iter()
            .find(|m| m.name.starts_with("nomic-embed-text"))
            .expect("the embedding model");
        assert!(!embed.can_chat());
        assert_eq!(embed.capabilities, ["embedding"]);

        let chat = models
            .iter()
            .find(|m| m.name == "gemma4:12b-it-qat")
            .expect("the chat model");
        assert!(chat.can_chat());
    }

    /// Protects: thinking is knowable before generating. A thinking model
    /// spends part of its token budget on reasoning before any lyrics appear
    /// (LLM-SURFACE 2), and this is the only advance warning available.
    #[test]
    fn test_thinking_capability_is_visible_before_generating() {
        let models = models();
        let gemma = models
            .iter()
            .find(|m| m.name == "gemma4:12b-it-qat")
            .expect("gemma");
        assert!(gemma.thinks(), "gemma4 streams delta.reasoning");

        let embed = models
            .iter()
            .find(|m| m.name.starts_with("nomic-embed-text"))
            .expect("embed");
        assert!(!embed.thinks());
    }

    /// Protects: the privacy distinction. A `remote_host` means the user's
    /// unreleased lyrics leave their machine, which the UI must surface -- and
    /// the reported `size` is a stub manifest, so showing it as disk usage
    /// would read as "this 2.8T model takes 308 bytes".
    #[test]
    fn test_remote_models_are_flagged_and_report_no_disk_size() {
        let models = models();
        let cloud = models
            .iter()
            .find(|m| m.name.ends_with(":cloud"))
            .expect("a cloud model");
        assert!(cloud.is_remote());
        assert_eq!(cloud.remote_host.as_deref(), Some("https://ollama.com"));
        assert_eq!(cloud.disk_size(), None);
        assert_eq!(cloud.size, 308, "the stub manifest size, kept as captured");

        let local = models
            .iter()
            .find(|m| m.name == "gemma4:12b-it-qat")
            .expect("a local model");
        let expected = vec!["gemma4".to_string()];
        assert_eq!(local.details.families.as_ref(), Some(&expected));
    }

    /// Protects: `parameter_size` is treated as a label. The same install
    /// reported `"1t"`, `"1T"`, `"756b"` and `"2.81T"`, and one model reported
    /// nothing at all -- so any code that parses or sorts on it is wrong.
    #[test]
    fn test_parameter_size_is_an_unnormalised_label() {
        let models = models();
        let empty = models
            .iter()
            .find(|m| m.name.starts_with("deepseek-v4-pro"))
            .expect("the model with no parameter size");
        assert_eq!(empty.details.parameter_size, "");
        assert!(empty.can_chat(), "still usable despite missing metadata");

        let cloud = models
            .iter()
            .find(|m| m.name == "kimi-k3:cloud")
            .expect("kimi");
        assert_eq!(cloud.details.parameter_size, "2.81T");
    }

    /// Protects: the wizard's one-URL setup. The user configures a single
    /// OpenAI-compatible base URL; the Ollama root is its parent when it ends
    /// in `/v1`, and nothing at all otherwise.
    #[test]
    fn test_ollama_root_is_derived_from_an_openai_base_url() {
        assert_eq!(
            OllamaNative::from_openai_base_url("http://127.0.0.1:11434/v1"),
            Some("http://127.0.0.1:11434".to_string())
        );
        assert_eq!(
            OllamaNative::from_openai_base_url("http://127.0.0.1:11434/v1/"),
            Some("http://127.0.0.1:11434".to_string())
        );
        assert_eq!(
            OllamaNative::from_openai_base_url("https://openrouter.ai/api/v1"),
            Some("https://openrouter.ai/api".to_string()),
        );
        assert_eq!(OllamaNative::from_openai_base_url("http://host/api"), None);
    }

    /// Protects: chat-model filtering happens on the list, not per model in
    /// the UI.
    #[test]
    fn test_chat_filter_drops_embedding_models() {
        let models = models();
        let chat: Vec<&str> = models
            .iter()
            .filter(|m| m.can_chat())
            .map(|m| m.name.as_str())
            .collect();
        assert!(!chat.iter().any(|n| n.starts_with("nomic-embed-text")));
        assert!(chat.contains(&"gemma4:12b-it-qat"));
    }

    /// Live check against a real server, excluded from CI.
    ///
    /// `cargo test -p llm-bridge -- --ignored` with Ollama running. Proves the
    /// captured fixture still matches what the server sends, which is the one
    /// thing a fixture cannot prove about itself.
    #[tokio::test]
    #[ignore = "requires a local Ollama on 127.0.0.1:11434"]
    async fn test_live_tags_still_match_the_captured_shape() {
        let client = OllamaNative::new("http://127.0.0.1:11434").expect("client");

        let version = client.version().await.expect("version");
        assert!(!version.is_empty(), "version string was empty");

        let models = client.list_models().await.expect("tags");
        assert!(!models.is_empty(), "server lists no models");
        assert!(
            models.iter().any(|m| m.can_chat()),
            "no model reports the completion capability"
        );
        for model in &models {
            assert!(!model.name.is_empty());
            assert_eq!(model.is_remote(), model.remote_host.is_some());
        }

        let chat = client.list_chat_models().await.expect("chat models");
        assert!(chat.iter().all(OllamaModel::can_chat));
    }
}
```

### 2. `crates/llm-bridge/src/lib.rs` — changed lines only
**Do not reproduce the whole file.** Make exactly these two edits and leave every other
line, including the module doc comment, byte-for-byte as it is.

Add one module line, so the block reads:

```rust
pub mod error;
pub mod ollama;
pub mod openai;
pub mod sse;
pub mod wire;
```

Add two re-exports immediately **before** the `openai::OpenAiCompat` one, keeping the
file's alphabetical-by-module order:

```rust
/// Re-export of [`ollama::OllamaModel`].
pub use ollama::OllamaModel;
/// Re-export of [`ollama::OllamaNative`].
pub use ollama::OllamaNative;
```

## Acceptance criteria
- [ ] `cargo test -p llm-bridge` passes: **29 tests, 2 ignored**
- [ ] `cargo test -p llm-bridge -- --ignored` passes with Ollama running (producer check)
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `npm run gate` green, and green with **nothing** running on port 11434
- [ ] no changes outside the two listed files; **no new dependencies**
- [ ] `lib.rs`'s module doc comment is unchanged, section sign included

## Out of scope
- `/api/pull` and its NDJSON framing — that is **T-109b**.
- `/api/show`, `/api/ps`, and `/api/ps`'s loaded shape (uncaptured, LLM-SURFACE 10).
- Any shared `LlmProvider` trait. T-109 settled that this module is not a provider
  (ARCHITECTURE 4).
- Wizard UI and Tauri commands (T-112).

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --read crates/llm-bridge/src/error.rs --file crates/llm-bridge/src/ollama.rs --file crates/llm-bridge/src/lib.rs
```
`error.rs` is `--read` because the reference code calls `http_error` and builds two
`LlmError` variants; it must not be edited (WORKFLOW 3).
