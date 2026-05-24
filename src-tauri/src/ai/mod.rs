//! Anthropic API integration.
//!
//! Layout:
//!   • [`client`]    — `reqwest` wrapper, prompt-caching aware.
//!   • [`prompts`]   — static system prompts (Chinese, per SPEC § 11 Q4).
//!   • [`summarize`] — `ai_summarize` orchestrator (cache lookup → API → persist).
//!
//! Each operation (`summarize`, `classify`, `translate`, `draft`) gets its own submodule
//! so model choice, prompt shape, and JSON schema stay decoupled.

pub mod client;
pub mod prompts;
pub mod summarize;
