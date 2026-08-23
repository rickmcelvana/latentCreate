//! LLM providers for lyric writing behind one trait.
//!
//! `openai_compat` is the universal baseline (Ollama, LM Studio, llama.cpp,
//! OpenRouter, vLLM); native providers are conveniences (ARCHITECTURE.md §4).
//! Populated in Phase 1.

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "llm-bridge");
    }
}
