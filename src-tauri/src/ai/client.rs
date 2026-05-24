//! Thin wrapper around `reqwest` for the Anthropic Messages API.
//!
//! Designed for the MVP's call pattern: one summary / classify / translate / draft per
//! message, with the *system* prompt cached (5-minute ephemeral TTL). The user-supplied
//! email body is the dynamic portion and is NOT cached.
//!
//! No third-party SDK by design — we want explicit control over caching, retries, and the
//! eventual streaming/batching switches. CLAUDE.md § "Tech decisions" locks this in.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult};

/// Anthropic API client. Reuses a single `reqwest::Client` (and therefore one HTTP
/// connection pool) across calls; cheap to clone.
#[derive(Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: SecretString,
    base_url: String,
}

/// One system-prompt segment. `cache = true` adds `cache_control: { type: "ephemeral" }`
/// — Anthropic caches every prefix up to and including that block for 5 minutes.
pub struct SystemBlock {
    pub text: String,
    pub cache: bool,
}

pub struct UserMessage {
    pub content: String,
}

pub struct CompletionRequest {
    pub model: String,
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

impl AnthropicClient {
    /// Read API key + optional base URL from the environment. In dev `.env` populates these;
    /// in prod they come from the OS keychain (Sprint 5+ will move the key over).
    pub fn from_env() -> AppResult<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| AppError::Config("ANTHROPIC_API_KEY not set".into()))?;
        let base_url = std::env::var("ANTHROPIC_API_BASE")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("ai-email/0.1.0 (+https://github.com/zhuxbo/ai-email)")
            .build()
            .map_err(|e| AppError::Anthropic(format!("failed to build http client: {e}")))?;

        Ok(Self {
            http,
            api_key: SecretString::from(api_key),
            base_url,
        })
    }

    /// POST `/v1/messages`. Returns the joined text of all `content[].text` blocks (we never
    /// ask for tool use at MVP) and the token-usage breakdown so the UI can surface cache
    /// hit rates.
    pub async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let system_blocks: Vec<_> = req
            .system
            .iter()
            .map(|s| {
                let mut block = json!({ "type": "text", "text": s.text });
                if s.cache {
                    block["cache_control"] = json!({ "type": "ephemeral" });
                }
                block
            })
            .collect();
        let messages: Vec<_> = req
            .messages
            .iter()
            .map(|m| json!({ "role": "user", "content": m.content }))
            .collect();

        let body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": system_blocks,
            "messages": messages,
        });

        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", self.api_key.expose_secret())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let payload: serde_json::Value = resp.json().await?;

        if !status.is_success() {
            let msg = payload["error"]["message"]
                .as_str()
                .or_else(|| payload.as_str())
                .unwrap_or("(no message)")
                .to_string();
            return Err(AppError::Anthropic(format!("HTTP {status}: {msg}")));
        }

        let text = payload["content"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Anthropic("response had no text content".into()))?;

        let model = payload["model"].as_str().unwrap_or(&req.model).to_string();

        let usage: Usage = serde_json::from_value(payload["usage"].clone()).unwrap_or_default();

        tracing::info!(
            model = %model,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cache_creation = ?usage.cache_creation_input_tokens,
            cache_read = ?usage.cache_read_input_tokens,
            "anthropic call complete"
        );

        Ok(CompletionResponse { text, model, usage })
    }
}
