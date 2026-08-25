//! LLM providers for lyric writing behind one trait.
//!
//! `openai_compat` is the universal baseline (Ollama, LM Studio, llama.cpp,
//! OpenRouter, vLLM); native providers are conveniences (ARCHITECTURE.md section 4).
//! Populated in Phase 1.

pub mod error;
pub mod ollama;
pub mod openai;
pub mod pull;
pub mod sse;
pub mod wire;

/// Re-export of [`error::LlmError`].
pub use error::LlmError;
/// Re-export of [`ollama::OllamaModel`].
pub use ollama::OllamaModel;
/// Re-export of [`ollama::OllamaNative`].
pub use ollama::OllamaNative;
/// Re-export of [`openai::OpenAiCompat`].
pub use openai::OpenAiCompat;
/// Re-export of [`pull::PullProgress`].
pub use pull::PullProgress;
/// Re-export of [`sse::SseDecoder`].
pub use sse::SseDecoder;
/// Re-export of [`sse::SseEvent`].
pub use sse::SseEvent;
/// Re-export of [`wire::ChatDelta`].
pub use wire::ChatDelta;
/// Re-export of [`wire::ChatMessage`].
pub use wire::ChatMessage;
/// Re-export of [`wire::ChatRequest`].
pub use wire::ChatRequest;
/// Re-export of [`wire::Role`].
pub use wire::Role;

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "llm-bridge");
    }
}
