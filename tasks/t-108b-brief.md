# T-108b: the chat-completion wire format, and the reasoning split
**Depends:** T-108a | **Crate/dir:** `crates/llm-bridge`
**Files to create/modify:**
- `crates/llm-bridge/src/wire.rs` (create)
- `crates/llm-bridge/src/lib.rs` (modify: one `pub mod`, four `pub use`)

## Goal
The OpenAI-compatible chunk types, and the decision about what each field *means* to the
app. This is the correctness core of lyric generation: it decides what text is allowed to
reach the user's song. Read [docs/LLM-SURFACE.md](../docs/LLM-SURFACE.md) sections 2, 3 and
5 first — every rule below is a captured fact, not a preference.

## Spec
Exactly the reference implementation below.

**The finding this task exists for.** Prompted `Reply with exactly: tulip`,
`gemma4:12b-it-qat` — the model docs/MODELS.md recommends for lyrics — sent **163
characters of `delta.reasoning` and 5 of `delta.content`**. So:

- `ChatDelta::Content` is the **only** variant whose text may be appended to the user's
  document. Merging reasoning into it writes the model's deliberation into the song.
- `ChatDelta::Reasoning` must still be emitted. For the 40 frames before content begins, a
  client that drops it shows a frozen UI on a perfectly healthy stream. It is status text.
- **Both spellings are read.** `reasoning` (Ollama, OpenRouter, current vLLM) and
  `reasoning_content` (DeepSeek, older vLLM) mean the same thing, and a client that knows
  one silently drops the thinking stream on the other (LLM-SURFACE 3).
- `refusal` is its own variant: when a model declines, the text lands there and `content`
  stays empty, so a client watching only `content` shows a blank answer with no reason.

**Two shapes that break the obvious implementation:**

- `choices` is `#[serde(default)]` and **routinely empty** — the final usage frame carries
  `"choices":[]`, so `chunk.choices[0]` fails on the last frame of every metered stream.
- `"content":""` is sent on nearly every frame alongside a non-empty `reasoning`, so
  presence is not text. `non_empty` exists for exactly this: `Option::is_some` would emit a
  stream of empty content deltas.

`deltas()` returns a `Vec` because one chunk can mean several things at once — text plus a
finish reason — and the usage frame means exactly one thing while having no choices at all.

## Fixture
`testdata/llm/ollama-chat-stream.sse` is already committed: a byte-for-byte capture of the
stream described above, with its provenance in `testdata/llm/README.md`. **Do not
regenerate or edit it** — the replay test asserts against its exact contents (163 reasoning
characters, `tulip`, `finish_reason: "stop"`).

## Reference implementation
Transcribe verbatim. This compiles, `cargo fmt` is a no-op on it, `cargo clippy
--all-targets -- -D warnings` is clean, and its 6 tests pass.

### 1. `crates/llm-bridge/src/wire.rs` (new file, complete)
```rust
//! The OpenAI-compatible chat-completions wire format, and what the app makes
//! of it.
//!
//! Every shape here was captured from a live endpoint on 2026-08-24 and
//! cross-checked against the OpenAI SDK's own chunk type -- see
//! docs/LLM-SURFACE.md. Nothing is written from the documentation alone.

use serde::{Deserialize, Serialize};

/// Who said a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// One message in the conversation sent to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// A chat-completion request.
///
/// `stream` and `stream_options` are set by the client, not the caller, so a
/// streaming call cannot accidentally be sent non-streaming.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Model id as the endpoint names it, e.g. `"gemma4:12b-it-qat"`.
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Upper bound on generated tokens. Note that reasoning tokens count
    /// against it: a 10-token budget on a reasoning model was spent entirely
    /// on chain-of-thought and produced no lyrics at all (LLM-SURFACE 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// Token counts, present only when the client asks for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

/// One streamed chunk, as it arrives on the wire.
///
/// `choices` is **`#[serde(default)]` and routinely empty**: the final usage
/// frame carries `"choices":[]`, so any code reaching for `choices[0]` fails
/// on the last frame of every stream that asks for token counts.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}

/// One choice inside a chunk. Only index 0 is ever used here; `n > 1` is not
/// something the lyric flow asks for.
#[derive(Debug, Clone, Deserialize)]
pub struct ChunkChoice {
    #[serde(default)]
    pub delta: Delta,
    /// `"stop"`, `"length"`, `"content_filter"`, `"tool_calls"`, or
    /// `"function_call"`. Arrives on a chunk whose `delta` is `{}`.
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// The incremental payload of one choice.
///
/// Unknown fields are ignored deliberately (no `deny_unknown_fields`): this
/// struct is fed by five different server implementations and gains fields
/// without warning.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Delta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    /// Chain-of-thought under the name Ollama, OpenRouter and current vLLM
    /// use.
    #[serde(default)]
    pub reasoning: Option<String>,
    /// The same thing under the name DeepSeek and older vLLM use. Both are
    /// read, because a client that knows only one silently drops the other.
    #[serde(default)]
    pub reasoning_content: Option<String>,
    /// Set when the model declines. The text lands here instead of `content`,
    /// so a client watching only `content` shows an empty answer and no
    /// explanation.
    #[serde(default)]
    pub refusal: Option<String>,
}

/// What the app does with a chunk.
///
/// The split between [`ChatDelta::Content`] and [`ChatDelta::Reasoning`] is
/// the whole point of this type. On a live capture, "Reply with exactly:
/// tulip" produced **163 characters of reasoning and 5 of content** from the
/// model this app recommends for lyrics. Concatenating both would put the
/// model's thinking into the user's song; ignoring both fields' distinction
/// would make a working stream look empty until reasoning ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatDelta {
    /// Text for the user's document.
    Content(String),
    /// The model thinking out loud. Shown as status, never appended to lyrics.
    Reasoning(String),
    /// The model declined to answer.
    Refusal(String),
    /// The model stopped, with its reason.
    Finished { reason: Option<String> },
    /// Token counts, when requested.
    Usage(TokenUsage),
}

impl ChatChunk {
    /// Everything this chunk means to the app, in wire order.
    ///
    /// One chunk can produce several deltas -- text plus a finish reason --
    /// and the usage frame produces exactly one despite having no choices.
    pub fn deltas(&self) -> Vec<ChatDelta> {
        let mut out = Vec::new();
        for choice in &self.choices {
            let delta = &choice.delta;
            if let Some(text) = non_empty(&delta.content) {
                out.push(ChatDelta::Content(text));
            }
            let thinking =
                non_empty(&delta.reasoning).or_else(|| non_empty(&delta.reasoning_content));
            if let Some(text) = thinking {
                out.push(ChatDelta::Reasoning(text));
            }
            if let Some(text) = non_empty(&delta.refusal) {
                out.push(ChatDelta::Refusal(text));
            }
            if choice.finish_reason.is_some() {
                out.push(ChatDelta::Finished {
                    reason: choice.finish_reason.clone(),
                });
            }
        }
        if let Some(usage) = self.usage {
            out.push(ChatDelta::Usage(usage));
        }
        out
    }
}

/// `Some(text)` only when the field is present and not the empty string.
///
/// Every content-bearing chunk from Ollama sets `"content":""` alongside a
/// non-empty `reasoning`, so treating "present" as "has text" would emit a
/// stream of empty content deltas.
fn non_empty(field: &Option<String>) -> Option<String> {
    field
        .as_ref()
        .filter(|text| !text.is_empty())
        .map(|text| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sse::{SseDecoder, SseEvent};

    /// A real Ollama stream: `gemma4:12b-it-qat` answering "Reply with
    /// exactly: tulip", captured 2026-08-24 (testdata/llm/README.md).
    const OLLAMA_STREAM: &str = include_str!("../../../testdata/llm/ollama-chat-stream.sse");

    fn chunk(json: &str) -> ChatChunk {
        serde_json::from_str(json).expect("chunk decodes")
    }

    /// Protects: chain-of-thought never reaches the user's lyrics. This frame
    /// is verbatim from the capture -- note `"content":""` sitting beside a
    /// non-empty `reasoning`, which is why presence is not enough to treat a
    /// field as text.
    #[test]
    fn test_reasoning_never_becomes_content() {
        let chunk = chunk(
            r#"{"choices":[{"index":0,"delta":{"content":"","reasoning":" user"},"finish_reason":null}]}"#,
        );
        assert_eq!(
            chunk.deltas(),
            vec![ChatDelta::Reasoning(" user".to_string())]
        );
    }

    /// Protects: the other spelling. DeepSeek and older vLLM send
    /// `reasoning_content`; a client reading only `reasoning` drops the whole
    /// thinking stream on those servers, which is a bug several projects have
    /// shipped (docs/LLM-SURFACE.md 3).
    #[test]
    fn test_reasoning_content_alias_is_read() {
        let chunk = chunk(
            r#"{"choices":[{"index":0,"delta":{"reasoning_content":"weighing it up"},"finish_reason":null}]}"#,
        );
        assert_eq!(
            chunk.deltas(),
            vec![ChatDelta::Reasoning("weighing it up".to_string())]
        );
    }

    /// Protects: the last frame of every metered stream. `choices` is empty
    /// there, so indexing `choices[0]` -- the obvious way to write this --
    /// fails on the one frame that carries the token counts.
    #[test]
    fn test_usage_frame_has_no_choices() {
        let chunk = chunk(
            r#"{"choices":[],"usage":{"prompt_tokens":24,"completion_tokens":10,"total_tokens":34}}"#,
        );
        assert_eq!(
            chunk.deltas(),
            vec![ChatDelta::Usage(TokenUsage {
                prompt_tokens: 24,
                completion_tokens: 10,
                total_tokens: 34,
            })]
        );
    }

    /// Protects: the stop signal is not attached to any text. It arrives on a
    /// chunk whose delta is `{}`, so code that only inspects text-bearing
    /// chunks never learns the stream ended or why.
    #[test]
    fn test_finish_reason_arrives_on_an_empty_delta() {
        let chunk = chunk(r#"{"choices":[{"index":0,"delta":{},"finish_reason":"length"}]}"#);
        assert_eq!(
            chunk.deltas(),
            vec![ChatDelta::Finished {
                reason: Some("length".to_string()),
            }]
        );
    }

    /// Protects: a refusal is surfaced rather than shown as an empty answer.
    /// The text lands in `refusal`, not `content`.
    #[test]
    fn test_refusal_is_surfaced() {
        let chunk =
            chunk(r#"{"choices":[{"index":0,"delta":{"refusal":"I can't help with that"}}]}"#);
        assert_eq!(
            chunk.deltas(),
            vec![ChatDelta::Refusal("I can't help with that".to_string())]
        );
    }

    /// Protects: the whole pipeline against a byte-for-byte real stream --
    /// decoder, chunk decode, and the reasoning/content split together.
    ///
    /// The numbers are the point: 163 characters of thinking, 5 of answer,
    /// from the model this app recommends for lyrics. A client that merged
    /// the two would put 163 characters of the model's deliberation into the
    /// user's song, and one that dropped `reasoning` would show a frozen UI
    /// for the 40 chunks before any content arrived.
    #[test]
    fn test_replaying_a_real_stream_separates_thinking_from_the_answer() {
        let mut decoder = SseDecoder::new();
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut finished = None;
        let mut done = false;

        // 64-byte reads, so events land mid-JSON exactly as they do on a socket.
        for piece in OLLAMA_STREAM.as_bytes().chunks(64) {
            for event in decoder.push(piece).expect("decodes") {
                match event {
                    SseEvent::Done => done = true,
                    SseEvent::Data(json) => {
                        for delta in chunk(&json).deltas() {
                            match delta {
                                ChatDelta::Content(text) => content.push_str(&text),
                                ChatDelta::Reasoning(text) => reasoning.push_str(&text),
                                ChatDelta::Finished { reason } => finished = reason,
                                other => panic!("unexpected delta: {other:?}"),
                            }
                        }
                    }
                }
            }
        }

        assert_eq!(content, "tulip");
        assert_eq!(reasoning.chars().count(), 163);
        assert_eq!(finished.as_deref(), Some("stop"));
        assert!(done, "the [DONE] sentinel must be seen");
    }
}
```

### 2. `crates/llm-bridge/src/lib.rs` (the two changes)
Add `pub mod wire;` after `pub mod sse;`, and these four re-exports **after** the `sse::`
ones, keeping the file's alphabetical-by-module order:

```rust
/// Re-export of [`wire::ChatDelta`].
pub use wire::ChatDelta;
/// Re-export of [`wire::ChatMessage`].
pub use wire::ChatMessage;
/// Re-export of [`wire::ChatRequest`].
pub use wire::ChatRequest;
/// Re-export of [`wire::Role`].
pub use wire::Role;
```

## Acceptance criteria
- [ ] `cargo test -p llm-bridge` passes: **16 tests** (10 after T-108a, plus 6 here)
- [ ] `test_replaying_a_real_stream_separates_thinking_from_the_answer` passes against the
      committed fixture, unmodified
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `npm run gate` green
- [ ] no changes outside the two listed files; **no new dependencies**

## Out of scope
- The HTTP client that feeds these types (T-108c).
- `tool_calls` / `function_call` deltas. The lyric flow does not use them and they were not
  captured, so they are not modelled (LLM-SURFACE 8).
- Anything reading these types in the Tauri layer (T-112).

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --read crates/llm-bridge/src/sse.rs --file crates/llm-bridge/src/wire.rs --file crates/llm-bridge/src/lib.rs
```
`sse.rs` is `--read` because the replay test constructs an `SseDecoder` and matches on
`SseEvent`; it must not be edited (WORKFLOW 3). The fixture is not passed to Aider at all —
it is 10 KB of captured data the executor has no reason to open.
