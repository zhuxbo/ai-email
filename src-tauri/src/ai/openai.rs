//! OpenAI-compatible `/v1/chat/completions` provider.
//!
//! Covers OpenAI itself plus every domestic vendor that ships an OpenAI-compatible endpoint —
//! DeepSeek, 智谱 GLM, Moonshot Kimi, 通义 Qwen, 字节豆包, Groq, Together, etc. The only
//! per-vendor difference is `base_url` and the `model_id` they expect.
//!
//! Differences vs Anthropic provider:
//!   • System prompts are sent as `messages[0]` with `role: "system"` — multiple system
//!     blocks are concatenated with "\n\n".
//!   • No `cache_control` field; the OpenAI server auto-caches long prefixes (≥1024 tokens
//!     on first-party OpenAI; most others have no caching). We surface `cache_read_tokens`
//!     from `usage.prompt_tokens_details.cached_tokens` when present.

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::ai::{CompletionRequest, CompletionResponse, Usage};
use crate::error::{AppError, AppResult};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com";

pub struct OpenAIProvider {
    http: reqwest::Client,
    api_key: SecretString,
    base_url: String,
    model_id: String,
}

impl OpenAIProvider {
    pub fn new(
        http: reqwest::Client,
        api_key: SecretString,
        base_url: String,
        model_id: String,
    ) -> Self {
        Self {
            http,
            api_key,
            base_url,
            model_id,
        }
    }

    pub async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let system_text: String = req
            .system
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut messages: Vec<serde_json::Value> = Vec::with_capacity(req.messages.len() + 1);
        if !system_text.is_empty() {
            messages.push(json!({ "role": "system", "content": system_text }));
        }
        for m in &req.messages {
            messages.push(json!({ "role": "user", "content": m.content }));
        }

        let body = json!({
            "model": self.model_id,
            "max_tokens": req.max_tokens,
            "messages": messages,
        });

        let resp = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .bearer_auth(self.api_key.expose_secret())
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let payload: serde_json::Value = resp.json().await?;

        if !status.is_success() {
            // OpenAI shape is { error: { message, type, ... } }. Some compatible servers
            // (DeepSeek, 智谱) drop `type` but always include `message`.
            let msg = payload["error"]["message"]
                .as_str()
                .unwrap_or_else(|| payload["error"].as_str().unwrap_or("(no message)"))
                .to_string();
            return Err(AppError::Ai(format!("HTTP {status}: {msg}")));
        }

        let text = payload["choices"][0]["message"]["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Ai("response had no choices[0].message.content".into()))?
            .to_string();

        let model = payload["model"]
            .as_str()
            .unwrap_or(&self.model_id)
            .to_string();

        let usage = parse_usage(&payload["usage"]);

        tracing::info!(
            provider = "openai",
            model = %model,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cache_read = ?usage.cache_read_input_tokens,
            "completion done"
        );

        Ok(CompletionResponse { text, model, usage })
    }
}

/// OpenAI's usage shape:
///   { "prompt_tokens": N, "completion_tokens": M,
///     "prompt_tokens_details": { "cached_tokens": K } }
/// Domestic vendors mostly skip `prompt_tokens_details`; we just leave the cache field None.
fn parse_usage(v: &serde_json::Value) -> Usage {
    let input_tokens = v["prompt_tokens"].as_u64().unwrap_or(0) as u32;
    let output_tokens = v["completion_tokens"].as_u64().unwrap_or(0) as u32;
    let cache_read_input_tokens = v["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .map(|n| n as u32);
    Usage {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens: None,
        cache_read_input_tokens,
    }
}
