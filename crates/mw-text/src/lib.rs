//! TEXT backend — skeleton.
//!
//! Implements [`TextBackend`]: renders dialogue for an already-committed speak
//! act (latent dialogue, DESIGN.md §4). The v0 target is a shared llama.cpp
//! GGUF model behind a priority queue. No behavior yet.

use mw_core::{SpeakRequest, TextBackend};

/// Shared instruct-model backend (Qwen3-0.6B Q4 baseline per DESIGN.md §6).
#[derive(Default)]
pub struct LlamaTextBackend;

impl TextBackend for LlamaTextBackend {
    fn render(&self, _request: &SpeakRequest<'_>) -> String {
        todo!("render committed speak act via the shared TEXT model")
    }
}
