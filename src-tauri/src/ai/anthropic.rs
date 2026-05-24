//! Anthropic `/v1/messages` provider.
//!
//! Honors `cache_control: { type: "ephemeral" }` on system blocks marked `cache = true`.
//! For OpenAI-compatible providers see `crate::ai::openai`.

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::ai::{CompletionRequest, CompletionResponse, Usage};
use crate::error::{AppError, AppResult};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

pub struct AnthropicProvider {
    http: reqwest::Client,
    api_key: SecretString,
    base_url: String,
    model_id: String,
}

impl AnthropicProvider {
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
            "model": self.model_id,
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
                .unwrap_or("(no message)")
                .to_string();
            return Err(AppError::Ai(format!("HTTP {status}: {msg}")));
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
            .ok_or_else(|| AppError::Ai("response had no text content".into()))?;

        let model = payload["model"]
            .as_str()
            .unwrap_or(&self.model_id)
            .to_string();
        let usage: Usage = serde_json::from_value(payload["usage"].clone()).unwrap_or_default();

        tracing::info!(
            provider = "anthropic",
            model = %model,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cache_creation = ?usage.cache_creation_input_tokens,
            cache_read = ?usage.cache_read_input_tokens,
            "completion done"
        );

        Ok(CompletionResponse { text, model, usage })
    }
}
