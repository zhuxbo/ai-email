//! AI provider abstraction.
//!
//! Two providers are wired at MVP:
//!   • [`anthropic`] — native `/v1/messages`, ephemeral prompt cache aware.
//!   • [`openai`]    — `/v1/chat/completions`. Covers OpenAI, DeepSeek, 智谱 GLM,
//!                    Moonshot Kimi, 通义 Qwen, 字节豆包, Groq, Together, etc. via
//!                    `base_url` override.
//!
//! Dispatch is an `enum AiClient`, not `dyn AiProvider` — closed set, no async-trait dance.
//! Build one with [`AiClient::build`] from a stored `AiModel` row + a SecretString key.
//!
//! The shared `CompletionRequest` / `CompletionResponse` / `Usage` types live here so
//! provider modules and call-site code agree on shape.

pub mod anthropic;
pub mod classify;
pub mod draft;
pub mod openai;
pub mod prompts;
pub mod summarize;
pub mod translate;

use std::time::Duration;

use secrecy::SecretString;
use serde::Deserialize;

use crate::db::ai_models::AiModel;
use crate::error::{AppError, AppResult};

// ── Shared request / response shape ──────────────────────────────────────────

/// One system-prompt segment. `cache = true` is honored by [`anthropic`] (adds
/// `cache_control: { type: "ephemeral" }`) and silently ignored by [`openai`] —
/// OpenAI handles prompt caching automatically; domestic vendors mostly don't cache.
pub struct SystemBlock {
    pub text: String,
    pub cache: bool,
}

pub struct UserMessage {
    pub content: String,
}

pub struct CompletionRequest {
    pub max_tokens: u32,
    pub system: Vec<SystemBlock>,
    pub messages: Vec<UserMessage>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub model: String,
    pub usage: Usage,
}

/// Provider-agnostic usage breakdown. Each provider fills the fields it tracks; missing
/// values surface as `0` for the required tokens and `None` for the cache fields.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub enum AiClient {
    Anthropic(anthropic::AnthropicProvider),
    OpenAI(openai::OpenAIProvider),
}

impl AiClient {
    /// Build a fresh client for an `AiModel` row. The reqwest client inside is also fresh
    /// — at MVP call rate (single-digit calls per minute) the pool reuse savings are
    /// negligible and a fresh client picks up any rotated keys/proxies immediately.
    pub fn build(model: &AiModel, api_key: SecretString) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent("ai-email/0.1.0 (+https://github.com/zhuxbo/ai-email)")
            .build()
            .map_err(|e| AppError::Ai(format!("failed to build http client: {e}")))?;

        match model.provider.as_str() {
            "anthropic" => {
                let base_url = model
                    .base_url
                    .clone()
                    .unwrap_or_else(|| anthropic::DEFAULT_BASE_URL.to_string());
                Ok(Self::Anthropic(anthropic::AnthropicProvider::new(
                    http,
                    api_key,
                    base_url,
                    model.model_id.clone(),
                )))
            }
            "openai" => {
                let base_url = model
                    .base_url
                    .clone()
                    .unwrap_or_else(|| openai::DEFAULT_BASE_URL.to_string());
                Ok(Self::OpenAI(openai::OpenAIProvider::new(
                    http,
                    api_key,
                    base_url,
                    model.model_id.clone(),
                )))
            }
            other => Err(AppError::Config(format!(
                "unknown ai provider: {other} (expected 'anthropic' or 'openai')"
            ))),
        }
    }

    pub async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        match self {
            Self::Anthropic(p) => p.complete(req).await,
            Self::OpenAI(p) => p.complete(req).await,
        }
    }
}
